//! Prepare aligned, common-gain references without modifying supplied stems.
use atomusicator_application::{
    CancellationToken,
    ports::{AudioBlockSource, AudioBlockStore},
};
use atomusicator_infrastructure::{
    audio_io::wav_codec::write_block_wav_hashed, audio_tools::AudioTools,
    storage::float_spool::FloatSpoolStore,
};
use std::{path::PathBuf, sync::Arc};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err(
            "usage: prepare_ground_truth <reference-directory> <new-preparation-directory>".into(),
        );
    }
    let root = PathBuf::from(&args[0]).canonicalize()?;
    let output = PathBuf::from(&args[1]);
    std::fs::create_dir(&output)?;
    let tools = AudioTools::discover()?;
    let cancel = CancellationToken::default();
    let keys = [
        "lead_vocal",
        "backing_vocals",
        "drums",
        "bass",
        "instrumental",
    ];
    let mut sources: Vec<Arc<dyn AudioBlockSource>> = Vec::new();
    let mut inputs = Vec::new();
    for key in keys {
        let paths: Vec<_> = ["wav", "flac"]
            .iter()
            .map(|ext| root.join(format!("{key}.{ext}")))
            .filter(|p| p.is_file())
            .collect();
        if paths.len() != 1 {
            return Err(format!("expected exactly one WAV or FLAC for {key}").into());
        }
        let source = Arc::new(tools.decode_to_spool(
            &paths[0],
            &output.join(format!("decoded-{key}.f32")),
            &cancel,
        )?);
        let hash =
            atomusicator_infrastructure::job_journal::JobJournal::hash_source(&paths[0], &cancel)?;
        inputs.push(serde_json::json!({"key":key,"source":paths[0],"sha256":hash,"frames":source.metadata().frame_count,"peak":source.peak()}));
        sources.push(source);
    }
    let mut metadata = sources[0].metadata();
    if sources
        .iter()
        .any(|s| s.metadata().format != metadata.format)
    {
        return Err("reference formats differ".into());
    }
    metadata.frame_count = sources
        .iter()
        .map(|s| s.metadata().frame_count)
        .max()
        .unwrap();
    let channels = metadata.format.channels as usize;
    let store = FloatSpoolStore::new(output.join("spool"));
    let mut buffers = vec![vec![0.0f32; 65536 * channels]; 5];
    let mut sum = vec![0.0f32; 65536 * channels];
    let mut raw = store.create("raw-sum", metadata)?;
    for start in (0..metadata.frame_count).step_by(65536) {
        let frames = (metadata.frame_count - start).min(65536) as usize;
        sum[..frames * channels].fill(0.0);
        for (source, buffer) in sources.iter().zip(&mut buffers) {
            let expected = source
                .metadata()
                .frame_count
                .saturating_sub(start)
                .min(frames as u64) as usize;
            if source.read_frames(start, &mut buffer[..frames * channels])? != expected {
                return Err("truncated reference".into());
            }
            for (total, value) in sum[..frames * channels].iter_mut().zip(buffer.iter()) {
                *total += value;
            }
        }
        raw.write(&sum[..frames * channels])?;
    }
    let raw = raw.finish()?;
    let peak = sources.iter().map(|s| s.peak()).fold(raw.peak(), f32::max);
    let gain = if peak > 1.0 { 0.99 / peak } else { 1.0 };
    let mut prepared = Vec::new();
    let mut refs = serde_json::Map::new();
    for (index, source) in sources.iter().enumerate() {
        let mut writer = store.create(&keys[index].replace('_', "-"), metadata)?;
        for start in (0..metadata.frame_count).step_by(65536) {
            let frames = (metadata.frame_count - start).min(65536) as usize;
            source.read_frames(start, &mut sum[..frames * channels])?;
            for sample in &mut sum[..frames * channels] {
                *sample *= gain;
            }
            writer.write(&sum[..frames * channels])?;
        }
        let source = writer.finish()?;
        let path = output.join(format!("{}.wav", keys[index]));
        let hash = write_block_wav_hashed(&path, source.as_ref(), &cancel)?;
        refs.insert(keys[index].into(),serde_json::json!({"path":path,"sha256":hash,"padded_frames":metadata.frame_count-sources[index].metadata().frame_count}));
        prepared.push(source);
    }
    let mut writer = store.create("mix", metadata)?;
    for start in (0..metadata.frame_count).step_by(65536) {
        let frames = (metadata.frame_count - start).min(65536) as usize;
        sum[..frames * channels].fill(0.0);
        for (source, buffer) in prepared.iter().zip(&mut buffers) {
            source.read_frames(start, &mut buffer[..frames * channels])?;
            for (total, value) in sum[..frames * channels].iter_mut().zip(buffer.iter()) {
                *total += value;
            }
        }
        writer.write(&sum[..frames * channels])?;
    }
    let mix = writer.finish()?;
    if mix.peak() > 1.0 {
        return Err("prepared mix exceeds full scale".into());
    }
    if root.join("mix.wav").exists() {
        let existing = tools.decode_to_spool(
            &root.join("mix.wav"),
            &output.join("existing-mix.f32"),
            &cancel,
        )?;
        let raw_comparison = atomusicator_infrastructure::audio_evidence::compare_sources(
            raw.as_ref(),
            &existing,
            &cancel,
        )?;
        let prepared_comparison = atomusicator_infrastructure::audio_evidence::compare_sources(
            mix.as_ref(),
            &existing,
            &cancel,
        )?;
        if [raw_comparison, prepared_comparison]
            .iter()
            .all(|c| c.channels.iter().any(|c| c.max_abs_error > 1e-6))
        {
            return Err("existing mix differs from both raw and common-gain reference sums; originals preserved".into());
        }
    }
    let path = output.join("mix.wav");
    let hash = write_block_wav_hashed(&path, mix.as_ref(), &cancel)?;
    if !root.join("mix.wav").exists() {
        std::fs::copy(&path, root.join("mix.wav"))?;
    }
    let report = serde_json::json!({"schema_version":1,"reference_provenance":"user-supplied reference stems; independent recording provenance not independently verified",
        "alignment":"start at sample zero; pad shorter tails with silence; no time shift or cropping",
        "format":metadata.format,"frames":metadata.frame_count,"raw_sum_peak":raw.peak(),"common_gain":gain,"prepared_mix_peak":mix.peak(),
        "mix":{"path":path,"sha256":hash},"inputs":inputs,"references":refs});
    std::fs::write(
        output.join("preparation.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
