use super::tests::{backend, legacy_session};
use super::*;
#[test]
fn imported_atomusicator_session_keeps_pitch_and_historical_bookmarks_work() {
    use atomusicator_infrastructure::storage::legacy_import::PREVIOUS_APP_DIRECTORY;
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let old = root.0.join(PREVIOUS_APP_DIRECTORY).join("projects/song");
    fs::create_dir_all(&old).unwrap();
    legacy_session(&old);
    let policy = WorkspacePathPolicy::from_local_data_dir(&root.0);
    let backend = backend(&root.0);
    let source =
        atomusicator_infrastructure::session_reader::read_session_files(&old, &Default::default())
            .unwrap();
    let old_dto = crate::session_preparation::prepare_tracks(
        &backend.mixer,
        source.id,
        source.directory.to_string_lossy().into_owned(),
        source.name,
        &source.stems,
        0,
    )
    .unwrap()
    .dto;
    backend
        .live_pitch_store
        .queue(
            pitch_persistence_key(&old_dto).unwrap(),
            PitchShiftSpec::try_new(7.0, false).unwrap(),
        )
        .unwrap();
    backend.live_pitch_store.flush().unwrap();
    let imported = backend.load_directory_with_policy(&old, &policy).unwrap();
    assert_eq!(imported.pitch_semitones, 7);
    assert!(
        Path::new(&imported.directory)
            .starts_with(policy.native_projects_dir().canonicalize().unwrap())
    );
    assert!(!backend.live_pitch_status().unwrap().preserve_formants);
    assert_eq!(
        backend
            .load_directory_with_policy(Path::new(&imported.directory), &policy)
            .unwrap()
            .pitch_semitones,
        7
    );
}
