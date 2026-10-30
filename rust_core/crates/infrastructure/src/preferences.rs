//! Native preference persistence and read-only adoption of historical QSettings.
use crate::{
    audio_tools::TemporaryDirectory, errors::InfrastructureError,
    storage::paths::WorkspacePathPolicy,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    #[default]
    Cpu,
    #[serde(rename = "directml")]
    DirectMl,
    Auto,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExecutionOptions {
    pub mode: ExecutionMode,
    pub device_id: u32,
}
pub struct ExecutionPreferences {
    root: PathBuf,
}
impl ExecutionPreferences {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    pub fn load(&self) -> Result<ExecutionOptions, InfrastructureError> {
        let path = self.root.join("execution.json");
        let options = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<ExecutionOptions>(&bytes).map_err(|e| {
                InfrastructureError::InvalidManifest(format!("Invalid execution preference: {e}"))
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => ExecutionOptions::default(),
            Err(e) => return Err(InfrastructureError::io(&path, e)),
        };
        validate_execution(options)?;
        Ok(options)
    }
    pub fn save(&self, options: ExecutionOptions) -> Result<(), InfrastructureError> {
        validate_execution(options)?;
        fs::create_dir_all(&self.root).map_err(|e| InfrastructureError::io(&self.root, e))?;
        let stage = TemporaryDirectory::new(&self.root)
            .map_err(|e| InfrastructureError::Domain(e.to_string()))?;
        let temporary = stage.0.join("execution.json");
        let bytes = serde_json::to_vec(&options)
            .map_err(|e| InfrastructureError::InvalidManifest(e.to_string()))?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| InfrastructureError::io(&temporary, e))?;
        use std::io::Write;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| InfrastructureError::io(&temporary, e))?;
        drop(file);
        fs::rename(&temporary, self.root.join("execution.json"))
            .map_err(|e| InfrastructureError::io(&temporary, e))
    }
    pub fn startup(&self) -> Result<ExecutionOptions, InfrastructureError> {
        let mut options = self.load()?;
        if let Some(value) = environment_value("ATOMUSICATOR_EXECUTION_PROVIDER")? {
            options.mode = match value.as_str() {
                "cpu" => ExecutionMode::Cpu,
                "directml" => ExecutionMode::DirectMl,
                "auto" => ExecutionMode::Auto,
                _ => {
                    return Err(InfrastructureError::InvalidManifest(
                        "execution provider must be cpu, directml or auto".into(),
                    ));
                }
            };
        }
        if let Some(value) = environment_value("ATOMUSICATOR_GPU_DEVICE_ID")? {
            options.device_id = value.parse().map_err(|_| {
                InfrastructureError::InvalidManifest("GPU device index must be unsigned".into())
            })?;
        }
        validate_execution(options)?;
        Ok(options)
    }
}
fn environment_value(name: &str) -> Result<Option<String>, InfrastructureError> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(e) => Err(InfrastructureError::InvalidManifest(e.to_string())),
    }
}
fn validate_execution(options: ExecutionOptions) -> Result<(), InfrastructureError> {
    if options.device_id > i32::MAX as u32 {
        Err(InfrastructureError::InvalidManifest(
            "GPU device index exceeds i32".into(),
        ))
    } else {
        Ok(())
    }
}

/// Separate from project-path preferences to preserve their historical format.
pub struct ComputePreferences {
    root: PathBuf,
}
impl ComputePreferences {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    pub fn load(
        &self,
    ) -> Result<atomusicator_application::compute_policy::ComputeProfile, InfrastructureError> {
        let path = self.root.join("compute-profile.json");
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Default::default());
            }
            Err(error) => return Err(InfrastructureError::io(&path, error)),
        };
        let value: String = serde_json::from_slice(&bytes).map_err(|error| {
            InfrastructureError::InvalidManifest(format!(
                "Invalid compute preference at {}: {error}",
                path.display()
            ))
        })?;
        value
            .parse()
            .map_err(|error: atomusicator_domain::DomainError| {
                InfrastructureError::InvalidManifest(error.to_string())
            })
    }
    pub fn save(
        &self,
        profile: atomusicator_application::compute_policy::ComputeProfile,
    ) -> Result<(), InfrastructureError> {
        fs::create_dir_all(&self.root).map_err(|e| InfrastructureError::io(&self.root, e))?;
        let stage = TemporaryDirectory::new(&self.root)
            .map_err(|e| InfrastructureError::Domain(e.to_string()))?;
        let temporary = stage.0.join("compute-profile.json");
        let bytes = serde_json::to_vec(profile.as_str())
            .map_err(|e| InfrastructureError::InvalidManifest(e.to_string()))?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| InfrastructureError::io(&temporary, e))?;
        use std::io::Write;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| InfrastructureError::io(&temporary, e))?;
        drop(file);
        fs::rename(&temporary, self.root.join("compute-profile.json"))
            .map_err(|e| InfrastructureError::io(&temporary, e))
    }
}

pub trait LegacyProjectsReader {
    /// Preferred historical application first, then its predecessor.
    fn candidates(&self) -> Vec<PathBuf>;
}
pub struct NativeLegacyProjects;
impl LegacyProjectsReader for NativeLegacyProjects {
    fn candidates(&self) -> Vec<PathBuf> {
        #[cfg(windows)]
        {
            crate::storage::legacy_import::registry_keys()
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
        self.policy.import_legacy_projects()?;
        let file = self.policy.app_data_root().join("settings.json");
        if let Some(saved) = fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PathBuf>(&bytes).ok())
            && !self.policy.is_historical_projects_path(&saved)
            && let Ok(valid) = WorkspacePathPolicy::validate_projects_destination(&saved)
        {
            return Ok(valid);
        }
        for candidate in self.legacy.candidates() {
            if self.policy.is_historical_projects_path(&candidate) {
                continue;
            }
            if let Ok(valid) = WorkspacePathPolicy::validate_projects_destination(&candidate) {
                self.save(&valid)?;
                return Ok(valid);
            }
        }
        self.save(&self.policy.native_projects_dir())
    }
    pub fn save(&self, path: &Path) -> Result<PathBuf, InfrastructureError> {
        if self.policy.is_historical_projects_path(path) {
            return Err(InfrastructureError::InvalidManifest("Historical application folders are read-only import sources; select an Atomusicator workspace".into()));
        }
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
