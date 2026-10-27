use atomusicator_infrastructure::{
    audio_tools::TemporaryDirectory,
    preferences::{LegacyProjectsReader, ProjectsPreferences},
    storage::paths::WorkspacePathPolicy,
};
use std::path::PathBuf;
struct Legacy(Vec<PathBuf>);
impl LegacyProjectsReader for Legacy {
    fn candidates(&self) -> Vec<PathBuf> {
        self.0.clone()
    }
}

#[test]
fn canonical_local_destinations_can_be_validated_repeatedly() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let first =
        WorkspacePathPolicy::validate_projects_destination(&root.0.join("projects")).unwrap();
    assert_eq!(
        WorkspacePathPolicy::validate_projects_destination(&first).unwrap(),
        first
    );
}

#[test]
fn adopts_legacy_directory_once_and_native_settings_take_precedence() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let policy = WorkspacePathPolicy::from_local_data_dir(&root.0);
    let old = root.0.join("historical-projects");
    let preferences = ProjectsPreferences::new(policy.clone(), Legacy(vec![old.clone()]));
    let adopted = preferences.load().unwrap();
    assert_eq!(adopted, old.canonicalize().unwrap());
    assert!(policy.app_data_root().join("settings.json").is_file());
    let next = root.0.join("native-projects");
    preferences.save(&next).unwrap();
    assert_eq!(preferences.load().unwrap(), next.canonicalize().unwrap());
}

#[test]
fn rejects_legacy_roots_and_uses_a_valid_local_default() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let policy = WorkspacePathPolicy::from_local_data_dir(&root.0);
    let store = ProjectsPreferences::new(
        policy.clone(),
        Legacy(vec![PathBuf::from("\\\\server\\share"), PathBuf::from("/")]),
    );
    let loaded = store.load().unwrap();
    assert_eq!(
        loaded,
        policy.default_projects_dir().canonicalize().unwrap()
    );
}
