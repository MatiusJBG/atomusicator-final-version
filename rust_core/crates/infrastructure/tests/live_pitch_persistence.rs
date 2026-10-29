use atomusicator_domain::PitchShiftSpec;
use atomusicator_infrastructure::{
    audio_tools::TemporaryDirectory, live_pitch_store::LivePitchStore,
};

#[test]
fn latest_settings_are_saved_outside_the_audio_thread_and_reopen_safely() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let store = LivePitchStore::new(root.0.clone()).unwrap();
    assert!(store.load("session-a").unwrap().is_none());
    for i in 0..100 {
        store
            .queue(
                "session-a".into(),
                PitchShiftSpec::try_new((i % 25) as f32 - 12.0, true).unwrap(),
            )
            .unwrap();
    }
    store.flush().unwrap();
    assert_eq!(store.load("session-a").unwrap().unwrap().semitones(), 12.0);
    assert!(store.load("session-b").unwrap().is_none());
}

#[test]
fn incompatible_and_invalid_saved_settings_cannot_reach_the_live_processor() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let store = LivePitchStore::new(root.0.clone()).unwrap();
    store
        .queue(
            "session".into(),
            PitchShiftSpec::try_new(7.0, true).unwrap(),
        )
        .unwrap();
    store.flush().unwrap();
    let path = std::fs::read_dir(&root.0)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    saved["engine"] = "incompatible-engine".into();
    std::fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
    assert!(store.load("session").unwrap().is_none());
    saved["engine"] = atomusicator_infrastructure::live_pitch::ENGINE_ID.into();
    saved["spec"]["semitones"] = 100.into();
    std::fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
    assert!(store.load("session").is_err());
    std::fs::write(&path, vec![b' '; 4097]).unwrap();
    assert!(store.load("session").is_err());
}
