use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use atomusicator_application::ports::SessionWorkspacePort;
use atomusicator_domain::{
    AudioAsset, AudioAssetMetadata, DomainError, SeparationResult, StemType,
};
use fs4::fs_std::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::InfrastructureError;
use crate::{audio_io::wav_codec::write_wav_f32, storage::paths::WorkspacePathPolicy};

#[derive(Debug, Clone)]
pub struct LocalSessionWorkspace {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSession {
    pub id: String,
    pub directory: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub session_id: String,
    pub source_path: PathBuf,
    pub file_name: String,
    pub duration_seconds: f64,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub size_bytes: u64,
    #[serde(flatten)]
    pub additional: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
struct PersistedStem {
    path: String,
    sample_rate_hz: u32,
    channels: u16,
    frames: usize,
}

#[derive(Debug, Clone, Serialize)]
struct PersistedSeparation {
    state: &'static str,
    stems: BTreeMap<StemType, PersistedStem>,
}

impl LocalSessionWorkspace {
    /// Uses `%LOCALAPPDATA%\Atomusicator` on Windows via the platform's local data directory.
    pub fn user_default() -> Result<Self, InfrastructureError> {
        let policy = WorkspacePathPolicy::system_default()?;
        Ok(Self::new(policy.default_projects_dir()))
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn create_session(
        &self,
        asset: &AudioAsset,
        metadata: AudioAssetMetadata,
        size_bytes: u64,
    ) -> Result<WorkspaceSession, InfrastructureError> {
        self.create_session_inner(asset, metadata, size_bytes, None)
    }

    pub fn persist_separation_result(
        &self,
        asset: &AudioAsset,
        result: &SeparationResult,
    ) -> Result<WorkspaceSession, InfrastructureError> {
        let representative_stem = result.stems().next().ok_or_else(|| {
            InfrastructureError::Domain("separation result contains no stems".into())
        })?;
        let metadata = AudioAssetMetadata {
            format: representative_stem.audio.format(),
            frame_count: representative_stem.audio.frames() as u64,
        };
        let size_bytes = fs::metadata(&asset.path)
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        self.create_session_inner(asset, metadata, size_bytes, Some(result))
    }

    fn create_session_inner(
        &self,
        asset: &AudioAsset,
        metadata: AudioAssetMetadata,
        size_bytes: u64,
        separation: Option<&SeparationResult>,
    ) -> Result<WorkspaceSession, InfrastructureError> {
        let root = WorkspacePathPolicy::validate_projects_destination(&self.root)?;
        fs::create_dir_all(&root).map_err(|error| InfrastructureError::io(&root, error))?;
        let _lock = Self::lock_workspace_at(&root)?;
        let id = unique_session_id();
        let final_dir = root.join(&id);
        let staging_dir = root.join(format!(".{id}.staging"));
        fs::create_dir(&staging_dir)
            .map_err(|error| InfrastructureError::io(&staging_dir, error))?;

        let publication = (|| {
            let outputs_dir = staging_dir.join("outputs");
            fs::create_dir(&outputs_dir)
                .map_err(|error| InfrastructureError::io(&outputs_dir, error))?;
            let mut persisted_stems = BTreeMap::new();
            if let Some(result) = separation {
                for stem in result.stems() {
                    let file_name = stem_file_name(stem.kind);
                    let output_path = outputs_dir.join(file_name);
                    write_wav_f32(&output_path, &stem.audio)
                        .map_err(|error| InfrastructureError::Domain(error.to_string()))?;
                    persisted_stems.insert(
                        stem.kind,
                        PersistedStem {
                            path: format!("outputs/{file_name}"),
                            sample_rate_hz: stem.audio.format().sample_rate_hz,
                            channels: stem.audio.format().channels,
                            frames: stem.audio.frames(),
                        },
                    );
                }
            }
            let metadata_path = staging_dir.join("session.json");
            let temporary_path = staging_dir.join("session.json.tmp");
            let duration_seconds =
                metadata.frame_count as f64 / metadata.format.sample_rate_hz as f64;
            let record = SessionMetadata {
                session_id: id.clone(),
                source_path: asset.path.clone(),
                file_name: asset.display_name.clone(),
                duration_seconds,
                sample_rate_hz: metadata.format.sample_rate_hz,
                channels: metadata.format.channels,
                size_bytes,
                additional: separation
                    .map(|_| {
                        serde_json::to_value(PersistedSeparation {
                            state: "complete",
                            stems: persisted_stems,
                        })
                        .map(|value| {
                            let mut additional = serde_json::Map::new();
                            additional.insert("separation".into(), value);
                            additional
                        })
                        .map_err(|error| InfrastructureError::InvalidManifest(error.to_string()))
                    })
                    .transpose()?
                    .unwrap_or_default(),
            };
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary_path)
                .map_err(|error| InfrastructureError::io(&temporary_path, error))?;
            serde_json::to_writer_pretty(&mut file, &record)
                .map_err(|error| InfrastructureError::InvalidManifest(error.to_string()))?;
            file.write_all(b"\n")
                .map_err(|error| InfrastructureError::io(&temporary_path, error))?;
            file.sync_all()
                .map_err(|error| InfrastructureError::io(&temporary_path, error))?;
            drop(file);
            fs::rename(&temporary_path, &metadata_path)
                .map_err(|error| InfrastructureError::io(&metadata_path, error))?;
            sync_directory(&staging_dir)?;
            fs::rename(&staging_dir, &final_dir)
                .map_err(|error| InfrastructureError::io(&final_dir, error))?;
            sync_directory(&root)?;
            Ok(())
        })();

        if let Err(error) = publication {
            let _ = fs::remove_dir_all(&staging_dir);
            return Err(error);
        }
        Ok(WorkspaceSession {
            id,
            directory: final_dir,
        })
    }

    pub fn read_metadata(
        &self,
        session: &WorkspaceSession,
    ) -> Result<SessionMetadata, InfrastructureError> {
        let path = session.directory.join("session.json");
        let bytes = fs::read(&path).map_err(|error| InfrastructureError::io(&path, error))?;
        let metadata: SessionMetadata = serde_json::from_slice(&bytes).map_err(|error| {
            InfrastructureError::InvalidManifest(format!("{}: {error}", path.display()))
        })?;
        if metadata.session_id != session.id {
            return Err(InfrastructureError::InvalidManifest(
                "session id does not match its metadata".into(),
            ));
        }
        Ok(metadata)
    }

    pub fn record_separation(
        &self,
        session: &WorkspaceSession,
        record: Value,
    ) -> Result<(), InfrastructureError> {
        let root = WorkspacePathPolicy::validate_projects_destination(&self.root)?;
        let _lock = Self::lock_workspace_at(&root)?;
        let mut metadata = self.read_metadata(session)?;
        metadata.additional.insert("separation".into(), record);
        let path = session.directory.join("session.json");
        let temporary_path = session.directory.join("session.json.tmp");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary_path)
            .map_err(|error| InfrastructureError::io(&temporary_path, error))?;
        serde_json::to_writer_pretty(&mut file, &metadata)
            .map_err(|error| InfrastructureError::InvalidManifest(error.to_string()))?;
        file.write_all(b"\n")
            .map_err(|error| InfrastructureError::io(&temporary_path, error))?;
        file.sync_all()
            .map_err(|error| InfrastructureError::io(&temporary_path, error))?;
        drop(file);
        fs::rename(&temporary_path, &path)
            .map_err(|error| InfrastructureError::io(&path, error))?;
        sync_directory(&session.directory)?;
        Ok(())
    }

    fn lock_workspace_at(root: &Path) -> Result<WorkspaceLock, InfrastructureError> {
        let lock_path = root.join(".workspace.lock");
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| InfrastructureError::io(&lock_path, error))?;
        file.try_lock_exclusive().map_err(|error| {
            if error.kind() == io::ErrorKind::WouldBlock {
                InfrastructureError::WorkspaceLocked
            } else {
                InfrastructureError::io(&lock_path, error)
            }
        })?;
        Ok(WorkspaceLock(file))
    }
}

impl SessionWorkspacePort for LocalSessionWorkspace {
    fn publish_separation_result(
        &self,
        asset: &AudioAsset,
        result: &SeparationResult,
    ) -> Result<(), DomainError> {
        self.persist_separation_result(asset, result)
            .map(|_| ())
            .map_err(|error| DomainError::WorkspaceFailure(error.to_string()))
    }
}

fn stem_file_name(stem_type: StemType) -> &'static str {
    match stem_type {
        StemType::LeadVocal => "01_lead_vocal.wav",
        StemType::BackingVocals => "02_backing_vocals.wav",
        StemType::Drums => "03_drums.wav",
        StemType::Bass => "04_bass.wav",
        StemType::Instrumental => "05_instrumental.wav",
    }
}

struct WorkspaceLock(File);

impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

fn unique_session_id() -> String {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{:016x}{:08x}{:08x}",
        timestamp as u64,
        std::process::id(),
        NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as u32
    )
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), InfrastructureError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| InfrastructureError::io(path, error))
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), InfrastructureError> {
    Ok(())
}
