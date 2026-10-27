//! Session preparation owns metadata, bounded waveform scanning and playback selection.
use crate::api::{UiSessionDto, UiStemTrackDto};
use atomusicator_application::ports::RealtimePlaybackPort;
use atomusicator_domain::{Stem, StemType};
use atomusicator_infrastructure::audio_io::{playback::CpalRealtimeMixer, wav_codec::read_wav_f32};
use std::path::PathBuf;
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
pub(crate) fn prepare_tracks(
    mixer: &CpalRealtimeMixer,
    id: String,
    directory: String,
    name: String,
    paths: &[(StemType, PathBuf)],
    pitch: i32,
) -> Result<UiSessionDto, String> {
    let mut tracks = Vec::new();
    let mut total_bytes = 0u64;
    let mut expected = None;
    for (kind, path) in paths {
        let (metadata, peaks) =
            atomusicator_infrastructure::waveform::wav_peaks(path, 1000).map_err(err)?;
        if metadata.frame_count == 0 || expected.is_some_and(|value| value != metadata) {
            return Err(
                "session stems must have identical nonempty formats and frame counts".into(),
            );
        }
        expected = Some(metadata);
        total_bytes = total_bytes.saturating_add(
            metadata
                .frame_count
                .saturating_mul(u64::from(metadata.format.channels) * 4),
        );
        tracks.push(UiStemTrackDto {
            stem_type: stem_key(*kind).into(),
            file_path: path.to_string_lossy().into_owned(),
            sample_rate: metadata.format.sample_rate_hz,
            channels: metadata.format.channels,
            duration_seconds: metadata.frame_count as f64
                / f64::from(metadata.format.sample_rate_hz),
            peaks,
        });
    }
    if total_bytes > 64 * 1024 * 1024 {
        mixer.load_streaming_session(paths.to_vec()).map_err(err)?;
    } else {
        let stems = paths
            .iter()
            .map(|(kind, path)| {
                Ok(Stem {
                    kind: *kind,
                    audio: read_wav_f32(path).map_err(err)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        mixer.load_session(stems).map_err(err)?;
    }
    Ok(UiSessionDto {
        id,
        directory,
        name,
        tracks,
        pitch_semitones: pitch,
    })
}

pub(crate) fn stem_key(kind: StemType) -> &'static str {
    match kind {
        StemType::LeadVocal => "lead_vocal",
        StemType::BackingVocals => "backing_vocals",
        StemType::Drums => "drums",
        StemType::Bass => "bass",
        StemType::Instrumental => "instrumental",
    }
}
