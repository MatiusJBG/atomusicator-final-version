use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use directories::BaseDirs;

use crate::errors::InfrastructureError;

pub const APP_DIRECTORY_NAME: &str = "Atomusicator";

static NEXT_PROBE_ID: AtomicU64 = AtomicU64::new(0);

/// Resolves app-owned files from the current OS profile, independent of install location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePathPolicy {
    app_data_root: PathBuf,
    legacy_data_roots: Vec<PathBuf>,
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
        Self {
            legacy_data_roots: super::legacy_import::data_roots(&local_data_dir),
            app_data_root: local_data_dir.join(APP_DIRECTORY_NAME),
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
        self.native_projects_dir()
    }

    pub fn legacy_project_dirs(&self) -> Vec<PathBuf> {
        self.legacy_data_roots
            .iter()
            .map(|root| root.join("projects"))
            .filter(|path| path.is_dir())
            .collect()
    }

    pub fn is_historical_projects_path(&self, path: &Path) -> bool {
        let candidate = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.legacy_project_dirs().iter().any(|root| {
            root.canonicalize()
                .is_ok_and(|root| candidate.starts_with(root))
        })
    }

    /// Resume requests may contain a formerly active managed destination.
    pub fn active_projects_destination(&self, path: &Path) -> PathBuf {
        if self.is_historical_projects_path(path) {
            self.native_projects_dir()
        } else {
            path.to_path_buf()
        }
    }

    pub fn import_legacy_projects(&self) -> Result<usize, InfrastructureError> {
        super::legacy_import::import_projects(self)
    }

    /// App-owned fallback, independent of historical folders and their permissions.
    pub fn native_projects_dir(&self) -> PathBuf {
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
        for legacy_root in &self.legacy_data_roots {
            candidates.push(legacy_root.join("models").join(file_name));
        }
        Ok(candidates)
    }

    /// Creates and probes the selected project directory before it is used for persistence.
    pub fn validate_projects_destination(path: &Path) -> Result<PathBuf, InfrastructureError> {
        let raw = path.to_string_lossy();
        #[cfg(windows)]
        let extended_local = matches!(path.components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), std::path::Prefix::VerbatimDisk(_)));
        #[cfg(not(windows))]
        let extended_local = false;
        if (raw.starts_with(r"\\") || raw.starts_with("//")) && !extended_local {
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
