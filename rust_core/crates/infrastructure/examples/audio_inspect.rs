//! Inspect local five-stem renders without requiring ground-truth stems.
//! Reconstruction is a technical invariant, not a measure of source quality.
use atomusicator_application::CancellationToken;
use atomusicator_domain::{AudioBufferF32, DomainError};
use atomusicator_infrastructure::{
    audio_evidence::{compare_audio, signal_statistics},
    audio_io::wav_codec::read_wav_f32,
    audio_tools::AudioTools,
    job_journal::JobJournal,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args_os().collect::<Vec<_>>();
    if args.len() == 4 && args[1] == "--stage-report" {
        return inspect_stage_report(Path::new(&args[2]), Path::new(&args[3]));
    }
    if args.len() != 4 && args.len() != 5 {
        return Err(
            "usage: audio_inspect <original-audio> <stem-directory> <local-report.json> [baseline-stem-directory]".into(),
        );
    }
    let source = AudioTools::discover()?
        .decode_for_separation(Path::new(&args[1]), &CancellationToken::default())?;
    let directory = PathBuf::from(&args[2]);
    let mut sum = vec![0.0; source.samples().len()];
    let mut stems = BTreeMap::new();
    let cancel = CancellationToken::default();
    for key in [
        "lead_vocal",
        "backing_vocals",
        "drums",
        "bass",
        "instrumental",
    ] {
        let path = directory.join(format!("{key}.wav"));
        let audio = read_wav_f32(&path)?;
        if audio.format() != source.format() || audio.frames() != source.frames() {
            return Err(DomainError::AudioAlignmentMismatch.into());
        }
        for (total, sample) in sum.iter_mut().zip(audio.samples()) {
            *total += sample;
        }
        let channels = usize::from(audio.format().channels);
        let interval = audio.format().sample_rate_hz as usize * 8;
        let boundaries = (interval..audio.frames()).step_by(interval).map(|frame| {
            let steps = (0..channels).map(|channel| (f64::from(audio.samples()[frame*channels+channel]) - f64::from(audio.samples()[(frame-1)*channels+channel])).abs()).collect::<Vec<_>>();
            serde_json::json!({"seconds":frame as f64 / f64::from(audio.format().sample_rate_hz),"absolute_adjacent_step":steps})
        }).collect::<Vec<_>>();
        let mut diagnostic = serde_json::json!({"channels":signal_statistics(&audio)?,"eight_second_boundaries":boundaries});
        diagnostic["sha256"] = JobJournal::hash_source(&path, &cancel)?.into();
        if args.len() == 5 {
            let baseline_path = Path::new(&args[4]).join(format!("{key}.wav"));
            let baseline = read_wav_f32(&baseline_path)?;
            diagnostic["baseline_sha256"] =
                JobJournal::hash_source(&baseline_path, &cancel)?.into();
            diagnostic["difference_from_baseline_not_ground_truth"] =
                serde_json::to_value(compare_audio(&baseline, &audio)?)?;
        }
        stems.insert(key, diagnostic);
    }
    let reconstructed = AudioBufferF32::new(source.format(), sum)?;
    let reconstruction = compare_audio(&source, &reconstructed)?;
    let reconstruction_passed = reconstruction.channels.iter().all(|channel| {
        channel.max_abs_error <= f64::from(atomusicator_domain::quality::RECONSTRUCTION_TOLERANCE)
    });
    let report = serde_json::json!({"schema_version":1,"evaluation":"local diagnostics without ground truth; no perceptual quality claim","frames":source.frames(),"sample_rate_hz":source.format().sample_rate_hz,"duration_seconds":source.frames() as f64/f64::from(source.format().sample_rate_hz),"mix":signal_statistics(&source)?,"stems":stems,"reconstruction":reconstruction,"reconstruction_passed":reconstruction_passed});
    fs::write(&args[3], serde_json::to_vec_pretty(&report)?)?;
    if !reconstruction_passed {
        return Err("five-stem sum exceeds the reconstruction tolerance".into());
    }
    Ok(())
}

fn inspect_stage_report(input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let report: serde_json::Value = serde_json::from_slice(&fs::read(input)?)?;
    let cases = report["cases"]
        .as_array()
        .ok_or("bounded benchmark cases required")?;
    let mut diagnostics = Vec::new();
    for case in cases {
        let levels = &case["stage_diagnostics"]["levels"];
        let mut rhythm = BTreeMap::new();
        for (stem, upstream, model) in [
            ("drums", "instrumental-master", "kuielab-mdxnet-drums-v1"),
            ("bass", "drum-free", "kuielab-mdxnet-bass-v1"),
        ] {
            let channels = case["metrics"][stem]["metrics"]["channels"]
                .as_array()
                .ok_or("reference metrics missing")?;
            let reference_rms = (channels
                .iter()
                .map(|v| v["reference_rms"].as_f64().unwrap_or(0.0).powi(2))
                .sum::<f64>()
                / channels.len() as f64)
                .sqrt();
            let final_rms = (channels
                .iter()
                .map(|v| v["estimate_rms"].as_f64().unwrap_or(0.0).powi(2))
                .sum::<f64>()
                / channels.len() as f64)
                .sqrt();
            let staged = levels[format!("stage_pcm/{stem}")]["rms"]
                .as_f64()
                .ok_or("stage RMS missing")?;
            let upstream_rms = levels[format!("stage_pcm/{upstream}")]["rms"]
                .as_f64()
                .ok_or("upstream RMS missing")?;
            let restored = levels[format!("{model}/restored_compensated_pcm")]["rms"]
                .as_f64()
                .ok_or("MDX restored RMS missing")?;
            let gain_db = if reference_rms > 0.0 && final_rms > 0.0 {
                Some(20.0 * (final_rms / reference_rms).log10())
            } else {
                None
            };
            let exact_wav = case["wav_roundtrip"][stem]["channels"]
                .as_array()
                .ok_or("WAV verification missing")?
                .iter()
                .all(|v| v["max_abs_error"].as_f64() == Some(0.0));
            rhythm.insert(stem,serde_json::json!({"reference_rms":reference_rms,"upstream_pcm_rms":upstream_rms,"post_mdx_restored_rms":restored,"staged_rms":staged,"wav_rms":final_rms,"gain_relative_reference_db":gain_db,"wav_is_sample_exact":exact_wav,
                "tensor_spectral_statistics":levels[format!("{model}/tensor_output_spectral_units")],"normalized_istft_window_statistics":levels[format!("{model}/post_istft_normalized_windows")],
                "gate":"bypassed: SpectralBleedGate only processes lead and backing vocals",
                "localization":if exact_wav && (staged-restored).abs()<1e-10 {"WAV and spool preserve MDX amplitude; any deficit is already present at restored MDX output. Compare upstream and direct-model probes to distinguish cascade suppression from model behavior."}else{"storage or restoration mismatch requires investigation"}}));
        }
        diagnostics.push(serde_json::json!({"case_id":case["case_id"],"rhythm":rhythm,"vocal_gate":{"before_lead":levels["gate_before/lead"],"after_lead":levels["gate_after/lead"],"before_backing":levels["gate_before/backing"],"after_backing":levels["gate_after/backing"]},"rhythm_intermediates":case["rhythm_intermediates"]}));
    }
    fs::write(
        output,
        serde_json::to_vec_pretty(
            &serde_json::json!({"schema_version":1,"source_report":input,"diagnostics":diagnostics,"unit_warning":"ONNX tensor RMS is spectral; iSTFT window RMS is normalized and includes overlap/padding. Neither equals whole-track reference RMS."}),
        )?,
    )?;
    Ok(())
}
