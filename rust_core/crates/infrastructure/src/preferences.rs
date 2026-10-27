//! Native preference persistence and read-only adoption of historical QSettings.
use crate::{
    audio_tools::TemporaryDirectory, errors::InfrastructureError,
    storage::paths::WorkspacePathPolicy,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub trait LegacyProjectsReader {
    /// Preferred historical application first, then its predecessor.
    fn candidates(&self) -> Vec<PathBuf>;
}
pub struct NativeLegacyProjects;
impl LegacyProjectsReader for NativeLegacyProjects {
    fn candidates(&self) -> Vec<PathBuf> {
        #[cfg(windows)]
        {
            [
                r"Software\Atomusicator\Atomusicator\projects",
                r"Software\Separador Vocal\Separador Vocal\projects",
            ]
            .iter()
            .filter_map(|key| {
                windows_registry::CURRENT_USER
                    .open(key)
                    .ok()?
                    .get_string("directory")
                    .ok()
            })
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .collect()
        }
        #[cfg(not(windows))]
        {
            Vec::new()
        }
    }
}

pub struct ProjectsPreferences<L> {
    policy: WorkspacePathPolicy,
    legacy: L,
}
impl<L: LegacyProjectsReader> ProjectsPreferences<L> {
    pub fn new(policy: WorkspacePathPolicy, legacy: L) -> Self {
        Self { policy, legacy }
    }
    pub fn load(&self) -> Result<PathBuf, InfrastructureError> {
        let file = self.policy.app_data_root().join("settings.json");
        if let Some(saved) = fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PathBuf>(&bytes).ok())
            && let Ok(valid) = WorkspacePathPolicy::validate_projects_destination(&saved)
        {
            return Ok(valid);
        }
        for candidate in self.legacy.candidates() {
            if let Ok(valid) = WorkspacePathPolicy::validate_projects_destination(&candidate) {
                self.save(&valid)?;
                return Ok(valid);
            }
        }
        WorkspacePathPolicy::validate_projects_destination(&self.policy.default_projects_dir())
    }
    pub fn save(&self, path: &Path) -> Result<PathBuf, InfrastructureError> {
        let valid = WorkspacePathPolicy::validate_projects_destination(path)?;
        fs::create_dir_all(self.policy.app_data_root())
            .map_err(|e| InfrastructureError::io(self.policy.app_data_root(), e))?;
        let stage = TemporaryDirectory::new(self.policy.app_data_root())
            .map_err(|e| InfrastructureError::Domain(e.to_string()))?;
        let temporary = stage.0.join("settings.json");
        let bytes =
            serde_json::to_vec(&valid).map_err(|e| InfrastructureError::Domain(e.to_string()))?;
        fs::write(&temporary, bytes).map_err(|e| InfrastructureError::io(&temporary, e))?;
        fs::OpenOptions::new()
            .write(true)
            .open(&temporary)
            .and_then(|file| file.sync_all())
            .map_err(|e| InfrastructureError::io(&temporary, e))?;
        fs::rename(
            &temporary,
            self.policy.app_data_root().join("settings.json"),
        )
        .map_err(|e| InfrastructureError::io(&temporary, e))?;
        Ok(valid)
    }
}
