//! Inspect local five-stem renders without requiring ground-truth stems.
//! Reconstruction is a technical invariant, not a measure of source quality.
use atomusicator_application::CancellationToken;
use atomusicator_domain::{AudioBufferF32, DomainError};
use atomusicator_infrastructure::{
    audio_evidence::{compare_audio, signal_statistics},
    audio_io::wav_codec::read_wav_f32,
    audio_tools::AudioTools,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args_os().collect::<Vec<_>>();
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
    for key in [
        "lead_vocal",
        "backing_vocals",
        "drums",
        "bass",
        "instrumental",
    ] {
        let audio = read_wav_f32(&directory.join(format!("{key}.wav")))?;
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
        if args.len() == 5 {
            let baseline = read_wav_f32(&Path::new(&args[4]).join(format!("{key}.wav")))?;
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
