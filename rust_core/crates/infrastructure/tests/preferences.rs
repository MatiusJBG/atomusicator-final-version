use atomusicator_infrastructure::storage::legacy_import::PREVIOUS_APP_DIRECTORY;
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

#[cfg(windows)]
#[test]
fn inaccessible_legacy_default_falls_back_to_native_projects_and_repairs_preferences() {
    use std::process::Command;
    struct DeniedFolder(PathBuf);
    impl Drop for DeniedFolder {
        fn drop(&mut self) {
            let _ = Command::new("icacls")
                .arg(&self.0)
                .args(["/remove:d", "*S-1-1-0"])
                .output();
        }
    }
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let legacy = root
        .0
        .join(PREVIOUS_APP_DIRECTORY)
        .join(PREVIOUS_APP_DIRECTORY)
        .join("projects");
    std::fs::create_dir_all(&legacy).unwrap();
    let sentinel = legacy.join("existing-session.json");
    std::fs::write(&sentinel, b"original session").unwrap();
    let policy = WorkspacePathPolicy::from_local_data_dir(&root.0);
    std::fs::create_dir_all(policy.app_data_root()).unwrap();
    std::fs::write(
        policy.app_data_root().join("settings.json"),
        serde_json::to_vec(&legacy).unwrap(),
    )
    .unwrap();
    let denial = Command::new("icacls")
        .arg(&legacy)
        .args(["/deny", "*S-1-1-0:(OI)(CI)(W)"])
        .output()
        .unwrap();
    assert!(
        denial.status.success(),
        "{}",
        String::from_utf8_lossy(&denial.stderr)
    );
    let restore_permissions = DeniedFolder(legacy.clone());
    assert!(WorkspacePathPolicy::validate_projects_destination(&legacy).is_err());
    let preferences = ProjectsPreferences::new(policy.clone(), Legacy(vec![legacy.clone()]));
    let loaded = preferences
        .load()
        .expect("a denied legacy folder must not block startup/import");
    assert_eq!(
        loaded,
        policy
            .app_data_root()
            .join("projects")
            .canonicalize()
            .unwrap()
    );
    let saved: PathBuf = serde_json::from_slice(
        &std::fs::read(policy.app_data_root().join("settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(saved, loaded);
    assert!(preferences.save(&legacy).is_err());
    assert_eq!(preferences.load().unwrap(), loaded);
    // Windows generic-write denial also denies Synchronize, used by normal
    // read handles. Restore this fixture's ACL before checking original bytes.
    drop(restore_permissions);
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"original session");
}
