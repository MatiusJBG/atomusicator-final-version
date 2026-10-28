//! Immutable pitch batches keyed by original content, specification and runtime.
use crate::{
    audio_io::wav_codec::wav_metadata,
    audio_tools::{TemporaryDirectory, unique_id},
    storage::checksum::file_hash,
};
use atomusicator_application::CancellationToken;
use atomusicator_domain::{DomainError, PitchShiftSpec, StemType};
use fs4::fs_std::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct LeasedPitchBatch {
    pub paths: Vec<(StemType, PathBuf)>,
    _lease: Arc<File>,
}

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
    ) -> Result<LeasedPitchBatch, DomainError> {
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
        fs::create_dir_all(&self.root).map_err(failure)?;
        let _mutation = self.mutation_lease(cancel)?;
        let lease = Arc::new(self.entry_lease(&key)?);
        FileExt::lock_shared(lease.as_ref()).map_err(failure)?;
        self.prune_locked(2 * 1024 * 1024 * 1024)?;
        if let Some(paths) = verified_batch(&target, cancel) {
            check(cancel)?;
            fs::write(target.join(".cache-key"), &key).map_err(failure)?;
            return Ok(LeasedPitchBatch {
                paths,
                _lease: lease,
            });
        }
        let metadata = metadata.ok_or(DomainError::AudioAlignmentMismatch)?;
        let needed = metadata
            .frame_count
            .checked_mul(u64::from(metadata.format.channels) * 4 * 5)
            .and_then(|n| n.checked_add(32 * 1024 * 1024))
            .ok_or_else(|| failure("pitch storage size overflow"))?;
        crate::resource_probe::ensure_disk(&self.root, needed)?;
        let stage = TemporaryDirectory::new(&self.root)?;
        fs::write(stage.0.join(".cache-key"), &key).map_err(failure)?;
        let mut outputs = BTreeMap::new();
        for (kind, source) in &sources {
            check(cancel)?;
            let output = stage.0.join(format!("{kind:?}.wav"));
            render(source, &output)?;
            if wav_metadata(&output)? != metadata {
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
                return Ok(LeasedPitchBatch {
                    paths,
                    _lease: lease,
                });
            }
            // Preserve corrupt or older files for diagnostics; never recursively
            // delete a session or overwrite files that another reader may own.
            fs::write(target.join(".cache-key"), &key).map_err(failure)?;
            fs::rename(
                &target,
                self.root.join(format!(".invalid-{key}-{}", unique_id())),
            )
            .map_err(failure)?;
        }
        fs::rename(&stage.0, &target).map_err(failure)?;
        Ok(LeasedPitchBatch {
            paths: StemType::ALL
                .into_iter()
                .map(|kind| (kind, target.join(format!("{kind:?}.wav"))))
                .collect(),
            _lease: lease,
        })
    }
    fn entry_lease(&self, key: &str) -> Result<File, DomainError> {
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join(format!("pitch-{key}.lease")))
            .map_err(failure)
    }
    fn mutation_lease(&self, cancel: &CancellationToken) -> Result<File, DomainError> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("mutation.lock"))
            .map_err(failure)?;
        loop {
            check(cancel)?;
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(file),
                Err(e) if contended(&e) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(e) => return Err(failure(e)),
            }
        }
    }
    /// Inactive batches are bounded; live playback/export leases can exceed the retention quota.
    pub fn prune(&self, max_bytes: u64) -> Result<(), DomainError> {
        if !self.root.exists() {
            return Ok(());
        }
        let mutation = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("mutation.lock"))
            .map_err(failure)?;
        match mutation.try_lock_exclusive() {
            Ok(()) => self.prune_locked(max_bytes),
            Err(e) if contended(&e) => Ok(()),
            Err(e) => Err(failure(e)),
        }
    }
    fn prune_locked(&self, max_bytes: u64) -> Result<(), DomainError> {
        let root = self.root.canonicalize().map_err(failure)?;
        let mut entries = Vec::new();
        let mut total = 0u64;
        for entry in fs::read_dir(&root).map_err(failure)? {
            let entry = entry.map_err(failure)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !entry.file_type().map_err(failure)?.is_dir()
                || !(name.starts_with("pitch-") || name.starts_with(".invalid-"))
            {
                continue;
            }
            let path = entry.path().canonicalize().map_err(failure)?;
            if path.parent() != Some(root.as_path()) {
                continue;
            }
            let marker = path.join(".cache-key");
            if !marker.is_file() || fs::metadata(&marker).map_err(failure)?.len() != 64 {
                continue;
            }
            let key = fs::read_to_string(marker).map_err(failure)?;
            if !key
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || !(name == format!("pitch-{key}")
                    || name.starts_with(&format!(".invalid-{key}-")))
            {
                continue;
            }
            let mut bytes = 0u64;
            let mut owned = true;
            for child in fs::read_dir(&path).map_err(failure)? {
                let child = child.map_err(failure)?;
                if !child.file_type().map_err(failure)?.is_file() {
                    owned = false;
                    break;
                }
                bytes = bytes.saturating_add(child.metadata().map_err(failure)?.len());
            }
            if !owned {
                continue;
            }
            total = total.saturating_add(bytes);
            entries.push((
                entry.metadata().map_err(failure)?.modified().ok(),
                path,
                key,
                bytes,
            ));
        }
        entries.sort_by_key(|entry| entry.0);
        for (_, path, key, bytes) in entries {
            if total <= max_bytes {
                break;
            }
            let lease = self.entry_lease(&key)?;
            match lease.try_lock_exclusive() {
                Ok(()) => {
                    fs::remove_dir_all(&path).map_err(failure)?;
                    total = total.saturating_sub(bytes);
                }
                Err(e) if contended(&e) => {}
                Err(e) => return Err(failure(e)),
            }
        }
        Ok(())
    }
}
fn contended(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::WouldBlock
        || error.raw_os_error() == fs4::lock_contended_error().raw_os_error()
}
fn verified_batch(
    directory: &Path,
    cancel: &CancellationToken,
) -> Option<Vec<(StemType, PathBuf)>> {
    if fs::metadata(directory.join("batch.json")).ok()?.len() > 65536 {
        return None;
    }
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
