//! Writes and verifies a complete bundle before its single publication point.
use crate::{
    audio_io::wav_codec::write_block_wav,
    audio_tools::TemporaryDirectory,
    session_reader::stem_key,
    session_workspace::WorkspaceSession,
    storage::{checksum::file_hash, paths::WorkspacePathPolicy},
};
use atomusicator_application::{CancellationToken, ports::AudioBlockSource};
use atomusicator_domain::{AudioAsset, DomainError, StemType};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::Arc,
};

pub struct PublicationHooks<S, C> {
    pub staged: S,
    pub commit: C,
}
pub fn publish_blocks<S: FnOnce(&Path) -> Result<(), String>, C: FnOnce() -> Result<(), String>>(
    root: &Path,
    id: &str,
    asset: &AudioAsset,
    stems: &BTreeMap<StemType, Arc<dyn AudioBlockSource>>,
    engine: serde_json::Value,
    cancel: &CancellationToken,
    hooks: PublicationHooks<S, C>,
) -> Result<WorkspaceSession, DomainError> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err(failure("invalid session id"));
    }
    let root = WorkspacePathPolicy::validate_projects_destination(root).map_err(failure)?;
    let target = root.join(id);
    if target.exists() {
        return Err(failure("session destination already exists"));
    }
    let first = stems
        .get(&StemType::LeadVocal)
        .ok_or(DomainError::MissingStem(StemType::LeadVocal))?;
    let metadata = first.metadata();
    if stems.len() != 5 || stems.values().any(|s| s.metadata() != metadata) {
        return Err(DomainError::AudioAlignmentMismatch);
    }
    let stage = TemporaryDirectory::new(&root)?;
    let mut owner = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(stage.0.join(".job-owner"))
        .map_err(failure)?;
    owner.write_all(id.as_bytes()).map_err(failure)?;
    owner.sync_all().map_err(failure)?;
    drop(owner);
    (hooks.staged)(&stage.0).map_err(failure)?;
    let outputs = stage.0.join("outputs");
    fs::create_dir(&outputs).map_err(failure)?;
    let mut records = Vec::new();
    for kind in StemType::ALL {
        let source = stems.get(&kind).ok_or(DomainError::MissingStem(kind))?;
        let key = stem_key(kind);
        let path = outputs.join(format!("{key}.wav"));
        write_block_wav(&path, source.as_ref(), cancel)?;
        let hash = file_hash(&path, cancel)?;
        records.push(serde_json::json!({"kind":key,"path":format!("outputs/{key}.wav"),"frames":metadata.frame_count,
            "sample_rate_hz":metadata.format.sample_rate_hz,"channels":metadata.format.channels,"sha256":hash}));
    }
    let record = serde_json::json!({"session_id":id,"source_path":asset.path,"file_name":asset.display_name,
        "duration_seconds":metadata.frame_count as f64/f64::from(metadata.format.sample_rate_hz),
        "sample_rate_hz":metadata.format.sample_rate_hz,"channels":metadata.format.channels,
        "separation":{"state":"complete","engine":engine,"stems":records}});
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(stage.0.join("session.json"))
        .map_err(failure)?;
    file.write_all(&serde_json::to_vec_pretty(&record).map_err(failure)?)
        .map_err(failure)?;
    file.sync_all().map_err(failure)?;
    drop(file);
    if cancel.is_cancelled() {
        return Err(DomainError::SeparationCancelled);
    }
    (hooks.commit)().map_err(failure)?;
    fs::rename(&stage.0, &target).map_err(failure)?;
    Ok(WorkspaceSession {
        id: id.into(),
        directory: target,
    })
}
fn failure(e: impl std::fmt::Display) -> DomainError {
    DomainError::WorkspaceFailure(e.to_string())
}
