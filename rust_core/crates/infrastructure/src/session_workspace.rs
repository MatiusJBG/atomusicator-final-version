use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use atomusicator_domain::{AudioAsset, AudioAssetMetadata};
use directories::BaseDirs;
use fs4::fs_std::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::InfrastructureError;

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

impl LocalSessionWorkspace {
    /// Uses `%LOCALAPPDATA%\Atomusicator` on Windows via the platform's local data directory.
    pub fn user_default() -> Result<Self, InfrastructureError> {
        let base_dirs = BaseDirs::new().ok_or_else(|| {
            InfrastructureError::InvalidManifest("local data directory is unavailable".into())
        })?;
        Ok(Self::new(base_dirs.data_local_dir().join("Atomusicator")))
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
        fs::create_dir_all(&self.root)
            .map_err(|error| InfrastructureError::io(&self.root, error))?;
        let _lock = self.lock_workspace()?;
        let id = unique_session_id();
        let final_dir = self.root.join(&id);
        let staging_dir = self.root.join(format!(".{id}.staging"));
        fs::create_dir(&staging_dir)
            .map_err(|error| InfrastructureError::io(&staging_dir, error))?;

        let publication = (|| {
            let outputs_dir = staging_dir.join("outputs");
            fs::create_dir(&outputs_dir)
                .map_err(|error| InfrastructureError::io(&outputs_dir, error))?;
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
                additional: serde_json::Map::new(),
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
            sync_directory(&self.root)?;
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
        let _lock = self.lock_workspace()?;
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

    fn lock_workspace(&self) -> Result<WorkspaceLock, InfrastructureError> {
        let lock_path = self.root.join(".workspace.lock");
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
