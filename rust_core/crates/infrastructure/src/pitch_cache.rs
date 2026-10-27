//! Immutable pitch batches keyed by original content, specification and runtime.
use crate::{
    audio_io::wav_codec::wav_metadata,
    audio_tools::{TemporaryDirectory, unique_id},
    storage::checksum::file_hash,
};
use atomusicator_application::CancellationToken;
use atomusicator_domain::{DomainError, PitchShiftSpec, StemType};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
struct BatchManifest {
    schema_version: u8,
    outputs: BTreeMap<StemType, String>,
}
pub struct PitchRenderCache {
    root: PathBuf,
}
impl PitchRenderCache {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    pub fn get_or_render(
        &self,
        originals: &[(StemType, PathBuf)],
        spec: PitchShiftSpec,
        runtime: &str,
        cancel: &CancellationToken,
        mut render: impl FnMut(&Path, &Path) -> Result<(), DomainError>,
    ) -> Result<Vec<(StemType, PathBuf)>, DomainError> {
        check(cancel)?;
        if originals.len() != 5 || runtime.is_empty() {
            return Err(failure(
                "pitch batch requires five originals and a runtime identity",
            ));
        }
        let mut sources = BTreeMap::new();
        for (kind, source) in originals {
            if sources.insert(*kind, source).is_some() {
                return Err(DomainError::DuplicateStem(*kind));
            }
        }
        let mut identities = BTreeMap::new();
        let mut metadata = None;
        for (kind, source) in &sources {
            let current = wav_metadata(source)?;
            if metadata
                .as_ref()
                .is_some_and(|expected| expected != &current)
            {
                return Err(DomainError::AudioAlignmentMismatch);
            }
            metadata = Some(current);
            identities.insert(*kind, file_hash(source, cancel)?);
        }
        let identity = serde_json::to_vec(&(1u8, spec, runtime, &identities)).map_err(failure)?;
        let key = format!("{:x}", Sha256::digest(identity));
        let target = self.root.join(format!("pitch-{key}"));
        if let Some(paths) = verified_batch(&target, cancel) {
            check(cancel)?;
            return Ok(paths);
        }
        fs::create_dir_all(&self.root).map_err(failure)?;
        let stage = TemporaryDirectory::new(&self.root)?;
        let mut outputs = BTreeMap::new();
        for (kind, source) in &sources {
            check(cancel)?;
            let output = stage.0.join(format!("{kind:?}.wav"));
            render(source, &output)?;
            if Some(wav_metadata(&output)?) != metadata {
                return Err(DomainError::AudioAlignmentMismatch);
            }
            fs::OpenOptions::new()
                .write(true)
                .open(&output)
                .and_then(|file| file.sync_all())
                .map_err(failure)?;
            outputs.insert(*kind, file_hash(&output, cancel)?);
        }
        let manifest = stage.0.join("batch.json");
        fs::write(
            &manifest,
            serde_json::to_vec(&BatchManifest {
                schema_version: 1,
                outputs,
            })
            .map_err(failure)?,
        )
        .map_err(failure)?;
        fs::OpenOptions::new()
            .write(true)
            .open(&manifest)
            .and_then(|file| file.sync_all())
            .map_err(failure)?;
        check(cancel)?;
        for (kind, source) in &sources {
            if identities.get(kind) != Some(&file_hash(source, cancel)?) {
                return Err(failure(
                    "an original changed while rendering pitch; retry the operation",
                ));
            }
        }
        if target.exists() {
            if let Some(paths) = verified_batch(&target, cancel) {
                check(cancel)?;
                return Ok(paths);
            }
            // Preserve corrupt or older files for diagnostics; never recursively
            // delete a session or overwrite files that another reader may own.
            fs::rename(&target, self.root.join(format!(".invalid-{}", unique_id())))
                .map_err(failure)?;
        }
        fs::rename(&stage.0, &target).map_err(failure)?;
        Ok(StemType::ALL
            .into_iter()
            .map(|kind| (kind, target.join(format!("{kind:?}.wav"))))
            .collect())
    }
}
fn verified_batch(
    directory: &Path,
    cancel: &CancellationToken,
) -> Option<Vec<(StemType, PathBuf)>> {
    let manifest: BatchManifest =
        serde_json::from_slice(&fs::read(directory.join("batch.json")).ok()?).ok()?;
    if manifest.schema_version != 1 || manifest.outputs.len() != 5 {
        return None;
    }
    let mut paths = Vec::with_capacity(5);
    for kind in StemType::ALL {
        let path = directory.join(format!("{kind:?}.wav"));
        if &file_hash(&path, cancel).ok()? != manifest.outputs.get(&kind)? {
            return None;
        }
        wav_metadata(&path).ok()?;
        paths.push((kind, path));
    }
    Some(paths)
}
fn check(cancel: &CancellationToken) -> Result<(), DomainError> {
    if cancel.is_cancelled() {
        Err(DomainError::SeparationCancelled)
    } else {
        Ok(())
    }
}
fn failure(error: impl std::fmt::Display) -> DomainError {
    DomainError::AudioExportFailure(error.to_string())
}
