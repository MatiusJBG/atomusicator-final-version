use atomusicator_domain::{AudioBufferF32, AudioFormat};
use atomusicator_infrastructure::{
    audio_io::wav_codec::write_wav_f32,
    audio_tools::TemporaryDirectory,
    preferences::{LegacyProjectsReader, ProjectsPreferences},
    session_reader::read_session_files,
    storage::{legacy_import::PREVIOUS_APP_DIRECTORY, paths::WorkspacePathPolicy},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Historical(Vec<PathBuf>);
impl LegacyProjectsReader for Historical {
    fn candidates(&self) -> Vec<PathBuf> {
        self.0.clone()
    }
}
fn historical_session(base: &Path) -> PathBuf {
    let session = base
        .join(PREVIOUS_APP_DIRECTORY)
        .join(PREVIOUS_APP_DIRECTORY)
        .join("projects/song");
    fs::create_dir_all(session.join("outputs")).unwrap();
    let samples =
        AudioBufferF32::new(AudioFormat::new(44100, 2).unwrap(), vec![0.1; 2048]).unwrap();
    let mut stems = Vec::new();
    for kind in [
        "lead_vocal",
        "backing_vocals",
        "drums",
        "bass",
        "instrumental",
    ] {
        let path = session.join(format!("outputs/{kind}.wav"));
        write_wav_f32(&path, &samples).unwrap();
        stems.push(serde_json::json!({"kind":kind,"path":path.canonicalize().unwrap()}));
    }
    fs::write(session.join("session.json"),serde_json::to_vec(&serde_json::json!({"session_id":"old-session","file_name":"original.flac","separation":{"state":"complete","stems":stems}})).unwrap()).unwrap();
    session
}

#[test]
fn managed_projects_are_imported_once_into_atomusicator_without_changing_originals() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let source = historical_session(&root.0);
    let original = fs::read(source.join("session.json")).unwrap();
    let policy = WorkspacePathPolicy::from_local_data_dir(&root.0);
    assert_eq!(
        policy.active_projects_destination(source.parent().unwrap()),
        policy.native_projects_dir()
    );
    let custom = root.0.join("my-custom-location");
    assert_eq!(policy.active_projects_destination(&custom), custom);
    assert_eq!(policy.import_legacy_projects().unwrap(), 1);
    assert_eq!(policy.import_legacy_projects().unwrap(), 0);
    let target = policy.native_projects_dir().join("song");
    let session = read_session_files(&target, &Default::default()).unwrap();
    assert_eq!(session.stems.len(), 5);
    assert!(
        session
            .stems
            .iter()
            .all(|(_, path)| path.starts_with(target.canonicalize().unwrap()))
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(target.join("session.json")).unwrap()).unwrap();
    assert!(
        manifest["separation"]["stems"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| !Path::new(entry["path"].as_str().unwrap()).is_absolute())
    );
    assert_eq!(fs::read(source.join("session.json")).unwrap(), original);
    let preferences = ProjectsPreferences::new(
        policy.clone(),
        Historical(vec![source.parent().unwrap().to_path_buf()]),
    );
    // A saved historical directory must be redirected even when it is writable.
    fs::write(
        policy.app_data_root().join("settings.json"),
        serde_json::to_vec(source.parent().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        preferences.load().unwrap(),
        policy.native_projects_dir().canonicalize().unwrap()
    );
    assert!(preferences.save(source.parent().unwrap()).is_err());
    let custom = root.0.join("my-workspace");
    preferences.save(&custom).unwrap();
    assert_eq!(preferences.load().unwrap(), custom.canonicalize().unwrap());
}

#[test]
fn native_name_collisions_preserve_both_projects_and_do_not_duplicate_on_restart() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    historical_session(&root.0);
    let policy = WorkspacePathPolicy::from_local_data_dir(&root.0);
    let existing = policy.native_projects_dir().join("song");
    fs::create_dir_all(&existing).unwrap();
    fs::write(existing.join("native-user-file"), b"preserve").unwrap();
    assert_eq!(policy.import_legacy_projects().unwrap(), 1);
    assert_eq!(policy.import_legacy_projects().unwrap(), 0);
    assert_eq!(
        fs::read(existing.join("native-user-file")).unwrap(),
        b"preserve"
    );
    let imports: Vec<_> = fs::read_dir(policy.native_projects_dir())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("song-import-")
        })
        .collect();
    assert_eq!(imports.len(), 1);
    assert_eq!(
        read_session_files(&imports[0].path(), &Default::default())
            .unwrap()
            .stems
            .len(),
        5
    );
}

#[test]
fn invalid_or_escaping_historical_sessions_are_not_published() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let source = historical_session(&root.0);
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(source.join("session.json")).unwrap()).unwrap();
    manifest["separation"]["stems"][0]["path"] = "../../outside.wav".into();
    fs::write(
        source.join("session.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let policy = WorkspacePathPolicy::from_local_data_dir(&root.0);
    assert_eq!(policy.import_legacy_projects().unwrap(), 0);
    assert!(!policy.native_projects_dir().join("song").exists());
    assert!(source.join("session.json").is_file());
}
