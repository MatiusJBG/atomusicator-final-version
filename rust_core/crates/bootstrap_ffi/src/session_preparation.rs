//! Session preparation owns metadata, bounded waveform scanning and playback selection.
use crate::api::{UiSessionDto, UiStemTrackDto};
use atomusicator_domain::StemType;
use atomusicator_infrastructure::audio_io::playback::CpalRealtimeMixer;
use atomusicator_infrastructure::waveform_index::WaveformIndex;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
pub(crate) struct PreparedSession {
    pub dto: UiSessionDto,
    pub waveforms: BTreeMap<StemType, Arc<WaveformIndex>>,
}
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
) -> Result<PreparedSession, String> {
    let mut tracks = Vec::new();
    let mut waveforms = BTreeMap::new();
    let mut expected = None;
    for (kind, path) in paths {
        let index = Arc::new(WaveformIndex::prepare(path, &std::env::temp_dir()).map_err(err)?);
        let metadata = index.metadata();
        let peaks = index
            .range(0, metadata.frame_count, 1000)
            .map_err(err)?
            .peaks;
        waveforms.insert(*kind, index);
        if metadata.frame_count == 0 || expected.is_some_and(|value| value != metadata) {
            return Err(
                "session stems must have identical nonempty formats and frame counts".into(),
            );
        }
        expected = Some(metadata);
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
    // Use the same filtered producer for previews and long songs. The explicit
    // in-memory port remains available as the frozen DSP/test reference.
    mixer.load_streaming_session(paths.to_vec()).map_err(err)?;
    Ok(PreparedSession {
        dto: UiSessionDto {
            id,
            directory,
            name,
            tracks,
            pitch_semitones: pitch,
        },
        waveforms,
    })
}

pub(crate) use atomusicator_infrastructure::session_reader::stem_key;
