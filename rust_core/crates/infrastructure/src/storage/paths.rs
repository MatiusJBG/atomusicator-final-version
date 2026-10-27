use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use directories::BaseDirs;

use crate::errors::InfrastructureError;

pub const APP_DIRECTORY_NAME: &str = "Atomusicator";
pub const LEGACY_APP_DIRECTORY_NAME: &str = "Separador Vocal";

static NEXT_PROBE_ID: AtomicU64 = AtomicU64::new(0);

/// Resolves app-owned files from the current OS profile, independent of install location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePathPolicy {
    app_data_root: PathBuf,
    legacy_data_root: Option<PathBuf>,
}

impl WorkspacePathPolicy {
    pub fn system_default() -> Result<Self, InfrastructureError> {
        let base_dirs = BaseDirs::new().ok_or_else(|| {
            InfrastructureError::InvalidManifest("local data directory is unavailable".into())
        })?;
        Ok(Self::from_local_data_dir(base_dirs.data_local_dir()))
    }

    /// Constructor for deterministic tests and callers with an explicit OS data directory.
    pub fn from_local_data_dir(local_data_dir: impl Into<PathBuf>) -> Self {
        let local_data_dir = local_data_dir.into();
        let legacy_root = local_data_dir.join(LEGACY_APP_DIRECTORY_NAME);
        Self {
            app_data_root: local_data_dir.join(APP_DIRECTORY_NAME),
            legacy_data_root: legacy_root.is_dir().then_some(legacy_root),
        }
    }

    pub fn app_data_root(&self) -> &Path {
        &self.app_data_root
    }

    pub fn models_dir(&self) -> PathBuf {
        self.app_data_root.join("models")
    }

    pub fn stage_cache_dir(&self) -> PathBuf {
        self.app_data_root.join("cache").join("stages")
    }

    pub fn default_projects_dir(&self) -> PathBuf {
        if let Some(legacy_projects) = self
            .legacy_data_root
            .as_ref()
            .map(|legacy_root| legacy_root.join("projects"))
            .filter(|path| path.is_dir())
        {
            return legacy_projects;
        }
        self.app_data_root.join("projects")
    }

    /// Returns current and legacy model locations in preference order.
    pub fn model_candidates(&self, file_name: &str) -> Result<Vec<PathBuf>, InfrastructureError> {
        let file_path = Path::new(file_name);
        if file_name.trim().is_empty()
            || file_name.contains(['/', '\\'])
            || !matches!(file_path.components().next(), Some(Component::Normal(_)))
            || file_path.components().count() != 1
        {
            return Err(InfrastructureError::InvalidManifest(
                "model filename must be a non-empty file name".into(),
            ));
        }

        let mut candidates = vec![self.models_dir().join(file_name)];
        if let Some(legacy_root) = &self.legacy_data_root {
            candidates.push(legacy_root.join("models").join(file_name));
        }
        Ok(candidates)
    }

    /// Creates and probes the selected project directory before it is used for persistence.
    pub fn validate_projects_destination(path: &Path) -> Result<PathBuf, InfrastructureError> {
        let raw = path.to_string_lossy();
        if raw.starts_with(r"\\") || raw.starts_with("//") {
            return Err(InfrastructureError::InvalidManifest(
                "UNC network paths cannot be used as a session workspace".into(),
            ));
        }
        if !path
            .components()
            .any(|component| matches!(component, Component::Normal(_)))
        {
            return Err(InfrastructureError::InvalidManifest(
                "filesystem roots cannot be used directly as a session workspace".into(),
            ));
        }

        fs::create_dir_all(path).map_err(|error| InfrastructureError::io(path, error))?;
        let probe = path.join(format!(
            ".atomusicator-write-probe-{}-{}.tmp",
            std::process::id(),
            NEXT_PROBE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)
            .map_err(|error| InfrastructureError::io(&probe, error))?;
        file.write_all(b"probe")
            .and_then(|()| file.sync_all())
            .map_err(|error| InfrastructureError::io(&probe, error))?;
        drop(file);
        fs::remove_file(&probe).map_err(|error| InfrastructureError::io(&probe, error))?;

        path.canonicalize()
            .map_err(|error| InfrastructureError::io(path, error))
    }
}
