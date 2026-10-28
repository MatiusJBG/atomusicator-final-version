//! Verifies a session bundle without changing playback or application state.
use crate::{audio_io::wav_codec::wav_metadata, storage::checksum::file_hash};
use atomusicator_application::CancellationToken;
use atomusicator_domain::{DomainError, StemType};
use std::{
    fs,
    path::{Path, PathBuf},
};
pub struct VerifiedSessionFiles {
    pub id: String,
    pub name: String,
    pub directory: PathBuf,
    pub stems: Vec<(StemType, PathBuf)>,
}
pub fn read_session_files(
    path: &Path,
    cancel: &CancellationToken,
) -> Result<VerifiedSessionFiles, DomainError> {
    let directory = path.canonicalize().map_err(failure)?;
    let manifest = directory.join("session.json");
    if fs::metadata(&manifest).map_err(failure)?.len() > 4 * 1024 * 1024 {
        return Err(failure("session metadata exceeds the supported size"));
    }
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(manifest).map_err(failure)?).map_err(failure)?;
    if value
        .pointer("/separation/state")
        .and_then(|s| s.as_str())
        .is_some_and(|s| s != "complete")
    {
        return Err(failure("session has no complete separation"));
    }
    let entries = value
        .pointer("/separation/stems")
        .ok_or_else(|| failure("session has no complete separation"))?;
    let count = entries
        .as_array()
        .map(Vec::len)
        .or_else(|| entries.as_object().map(serde_json::Map::len));
    if count != Some(5) {
        return Err(failure("session must contain exactly five unique stems"));
    }
    let mut stems = Vec::with_capacity(5);
    let mut expected = None;
    for kind in StemType::ALL {
        if cancel.is_cancelled() {
            return Err(DomainError::SeparationCancelled);
        }
        let key = stem_key(kind);
        let entry = if let Some(array) = entries.as_array() {
            array.iter().find(|s| s["kind"].as_str() == Some(key))
        } else {
            entries.get(format!("{kind:?}"))
        }
        .ok_or_else(|| failure(format!("session is missing {key}")))?;
        let relative = entry["path"]
            .as_str()
            .ok_or_else(|| failure("invalid stem path"))?;
        let path = directory.join(relative).canonicalize().map_err(failure)?;
        if !path.starts_with(&directory) {
            return Err(failure("stem path escapes the session directory"));
        }
        if let Some(expected) = entry["sha256"].as_str()
            && file_hash(&path, cancel)? != expected
        {
            return Err(failure(format!("stem checksum mismatch: {key}")));
        }
        let metadata = wav_metadata(&path)?;
        if metadata.frame_count == 0 || expected.is_some_and(|m| m != metadata) {
            return Err(DomainError::AudioAlignmentMismatch);
        }
        expected = Some(metadata);
        stems.push((kind, path));
    }
    Ok(VerifiedSessionFiles {
        id: value["session_id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| failure("missing session id"))?
            .to_owned(),
        name: value["file_name"]
            .as_str()
            .unwrap_or("Audio session")
            .to_owned(),
        directory,
        stems,
    })
}
pub fn stem_key(kind: StemType) -> &'static str {
    match kind {
        StemType::LeadVocal => "lead_vocal",
        StemType::BackingVocals => "backing_vocals",
        StemType::Drums => "drums",
        StemType::Bass => "bass",
        StemType::Instrumental => "instrumental",
    }
}
fn failure(error: impl std::fmt::Display) -> DomainError {
    DomainError::WorkspaceFailure(error.to_string())
}
