//! Opt-in end-to-end CPU benchmark through the same API used by Flutter.
//! Inputs, models and outputs stay local; the user's projects preference is restored.
use atomusicator_bootstrap_ffi::api::{self, UiJobStatus};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: desktop_bench <local-source-audio> <local-output-directory>".into());
    }
    let source = PathBuf::from(&args[0])
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let output = PathBuf::from(&args[1]);
    std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let previous = api::projects_directory()?;
    api::set_projects_directory(output.to_string_lossy().into_owned())?;
    let result = (|| {
        let started = Instant::now();
        let id = api::start_separation(source.to_string_lossy().into_owned(), false)?;
        let mut message = String::new();
        loop {
            let job = api::job_status(id.clone())?;
            if job.message != message {
                eprintln!("{}: {:.1}% {}", job.id, job.progress * 100.0, job.message);
                message = job.message.clone();
            }
            match job.status {
                UiJobStatus::Completed => {
                    let session = job
                        .session
                        .ok_or("separation completed without a session")?;
                    if session.tracks.len() != 5 {
                        return Err("expected five tracks".into());
                    }
                    api::seek(100.0f64.min(session.tracks[0].duration_seconds))?;
                    let playback = api::playback_status()?;
                    if playback.failure.is_some() {
                        return Err("playback preparation failed".into());
                    }
                    println!(
                        "{}",
                        serde_json::json!({"job_id":id,"elapsed_seconds":started.elapsed().as_secs_f64(),
                        "session_directory":session.directory,"tracks":session.tracks.len(),
                        "duration_seconds":session.tracks[0].duration_seconds,"seek_seconds":playback.position_seconds,
                        "engine":"desktop-bounded-native-v1","provider":"CPUExecutionProvider"})
                    );
                    return Ok(());
                }
                UiJobStatus::Failed | UiJobStatus::Cancelled | UiJobStatus::Recoverable => {
                    return Err(job.message);
                }
                _ => {}
            }
            if started.elapsed() > Duration::from_secs(14400) {
                api::cancel_job(id)?;
                return Err("benchmark deadline exceeded".into());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    })();
    let restored = api::set_projects_directory(previous.clone());
    if let Err(error) = restored {
        return Err(format!(
            "restore projects folder to {previous}: {error}; benchmark result: {result:?}"
        ));
    }
    result
}
