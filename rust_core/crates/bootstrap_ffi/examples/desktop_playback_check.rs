//! Opt-in physical CPAL callback check; master is zero throughout, so no recording is played audibly.
use atomusicator_bootstrap_ffi::api;
use std::time::{Duration, Instant};

fn main() -> Result<(), String> {
    let directory = std::env::args()
        .nth(1)
        .ok_or("usage: desktop_playback_check <session-directory>")?;
    let result = (|| {
        let session = api::open_audio_session(directory)?;
        let duration = session
            .tracks
            .first()
            .ok_or("empty session")?
            .duration_seconds;
        if duration < 3.0 {
            return Err("use a session longer than three seconds".into());
        }
        api::set_master(0.0)?;
        api::play()?;
        wait_for(0.25)?;
        api::pause()?;
        let paused = api::playback_status()?.position_seconds;
        std::thread::sleep(Duration::from_millis(100));
        if (api::playback_status()?.position_seconds - paused).abs() > 0.001 {
            return Err("paused clock advanced".into());
        }
        for position in [duration * 0.5, 0.1, duration * 0.25] {
            api::seek(position)?;
            api::play()?;
            wait_for(position + 0.1)?;
        }
        api::seek(duration - 0.15)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = api::playback_status()?;
            if status.failure.is_some() {
                return Err(format!("audio output failed: {:?}", status.failure));
            }
            if !status.playing {
                if (status.position_seconds - duration).abs() > 0.002 {
                    return Err("premature EOF".into());
                }
                break;
            }
            if Instant::now() > deadline {
                return Err("EOF deadline exceeded".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        println!(
            "{}",
            serde_json::json!({"tracks":session.tracks.len(),"duration_seconds":duration,
            "physical_callback":true,"master_gain":0,"pause_seek_eof":true,"auditory_quality_evaluated":false})
        );
        Ok(())
    })();
    let stopped = api::stop();
    result.and(stopped)
}
fn wait_for(position: f64) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = api::playback_status()?;
        if status.failure.is_some() {
            return Err(format!("audio output failed: {:?}", status.failure));
        }
        if status.position_seconds >= position {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err("audio callback did not advance".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
