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
