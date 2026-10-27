//! Flutter-facing API. Heavy work is exposed as asynchronous bridge calls.

use atomusicator_domain::StemType;

/// Flutter-facing description of one persisted stem track.
#[derive(Debug, Clone)]
pub struct UiStemTrackDto {
    pub stem_type: String,
    pub file_path: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_seconds: f64,
    /// Interleaved min/max waveform buckets; PCM stays in the native engine.
    pub peaks: Vec<f32>,
}

#[derive(Debug, Clone)]
pub struct UiSessionDto {
    pub id: String,
    pub directory: String,
    pub name: String,
    pub tracks: Vec<UiStemTrackDto>,
    pub pitch_semitones: i32,
}
#[derive(Debug, Clone)]
pub struct UiJobDto {
    pub id: String,
    pub status: UiJobStatus,
    pub retry_of: Option<String>,
    pub failure: Option<UiJobFailure>,
    pub progress: f32,
    pub message: String,
    pub session: Option<UiSessionDto>,
    pub output_directory: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiJobStatus {
    Queued,
    Running,
    Cancelling,
    Committing,
    Cancelled,
    Failed,
    Recoverable,
    Completed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiJobFailure {
    Cancelled,
    SourceUnavailable,
    InvalidArtifact,
    InsufficientResources,
    Storage,
    DeviceUnavailable,
    Processing,
    Interrupted,
}
#[derive(Debug, Clone)]
pub struct UiRecoverableJobDto {
    pub id: String,
    pub source_path: String,
    pub message: String,
    pub output_directory: Option<String>,
}
pub fn recoverable_jobs() -> Result<Vec<UiRecoverableJobDto>, String> {
    crate::studio::studio()?.recoverable()
}
pub fn resume_job(job_id: String) -> Result<String, String> {
    crate::studio::studio()?.resume(&job_id)
}
pub fn discard_job(job_id: String) -> Result<(), String> {
    crate::studio::studio()?.discard(&job_id)
}
#[derive(Debug, Clone)]
pub struct UiTrackMeterDto {
    pub stem_type: String,
    /// Post track gain/pan/mute/solo, before master gain and output clipping.
    pub peak: f32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiPlaybackFailure {
    DeviceUnavailable,
}
#[derive(Debug, Clone)]
pub struct UiPlaybackDto {
    pub playing: bool,
    pub position_seconds: f64,
    pub peak: f32,
    pub track_meters: Vec<UiTrackMeterDto>,
    pub failure: Option<UiPlaybackFailure>,
}

pub fn start_separation(source_path: String, allow_model_download: bool) -> Result<String, String> {
    crate::studio::studio()?.separate(source_path.into(), allow_model_download)
}
pub fn job_status(job_id: String) -> Result<UiJobDto, String> {
    crate::studio::studio()?.job(&job_id)
}
pub fn cancel_job(job_id: String) -> Result<(), String> {
    crate::studio::studio()?.cancel(&job_id)
}
pub fn open_audio_session(directory: String) -> Result<UiSessionDto, String> {
    crate::studio::studio()?.open(std::path::Path::new(&directory))
}
pub fn projects_directory() -> Result<String, String> {
    crate::studio::studio()?.projects_directory()
}
pub fn set_projects_directory(directory: String) -> Result<(), String> {
    crate::studio::studio()?.set_projects_directory(std::path::Path::new(&directory))
}
pub fn start_pitch(semitones: i32, preserve_formants: bool) -> Result<String, String> {
    crate::studio::studio()?.pitch(semitones, preserve_formants)
}
pub fn start_export(
    destination: String,
    selected_stems: Vec<String>,
    format: String,
    sample_rate: u32,
    channels: u16,
    bit_depth: u16,
    normalize: bool,
) -> Result<String, String> {
    let options = atomusicator_infrastructure::audio_tools::ExportOptions::new(
        &format,
        sample_rate,
        channels,
        bit_depth,
        normalize,
    )
    .map_err(|e| e.to_string())?;
    crate::studio::studio()?.export(destination.into(), selected_stems, options)
}
pub fn playback_status() -> Result<UiPlaybackDto, String> {
    Ok(crate::studio::studio()?.playback())
}
pub fn play() -> Result<(), String> {
    crate::studio::studio()?
        .mixer
        .start()
        .map_err(|e| e.to_string())
}
pub fn pause() -> Result<(), String> {
    crate::studio::studio()?.mixer.pause();
    Ok(())
}
pub fn stop() -> Result<(), String> {
    crate::studio::studio()?
        .mixer
        .stop()
        .map_err(|e| e.to_string())
}
pub fn seek(position_seconds: f64) -> Result<(), String> {
    crate::studio::studio()?
        .mixer
        .seek(position_seconds)
        .map_err(|e| e.to_string())
}
pub fn set_master(gain: f32) -> Result<(), String> {
    crate::studio::studio()?
        .mixer
        .set_master(gain)
        .map_err(|e| e.to_string())
}
pub fn set_track(
    stem_type: String,
    gain_db: f32,
    pan: f32,
    mute: bool,
    solo: bool,
) -> Result<(), String> {
    use atomusicator_application::ports::RealtimePlaybackPort;
    if !gain_db.is_finite()
        || !pan.is_finite()
        || !(-96.0..=24.0).contains(&gain_db)
        || !(-1.0..=1.0).contains(&pan)
    {
        return Err("invalid track gain or pan".into());
    }
    crate::studio::studio()?.mixer.set_track_state(
        crate::studio::parse_stem(&stem_type)?,
        gain_db,
        pan,
        mute,
        solo,
    );
    Ok(())
}

/// Health check for the Rust core and its canonical five-stem contract.
#[flutter_rust_bridge::frb]
pub fn get_engine_version() -> String {
    format!(
        "Atomusicator Native Core v{} (Stems: {})",
        env!("CARGO_PKG_VERSION"),
        StemType::ALL.len()
    )
}

#[flutter_rust_bridge::frb]
pub fn application_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[flutter_rust_bridge::frb]
pub fn supported_stems() -> Vec<String> {
    [
        "Lead Vocal",
        "Backing Vocals",
        "Drums",
        "Bass",
        "Instrumental",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn engine_version_reports_native_core_and_stem_count() {
        assert_eq!(
            super::get_engine_version(),
            "Atomusicator Native Core v0.6.0 (Stems: 5)"
        );
    }
}
