use atomusicator_application::{
    CancellationToken,
    ports::{AudioBlockSource, AudioBlockStore},
};
use atomusicator_domain::{AudioAssetMetadata, AudioFormat};
#[test]
fn impossible_spool_reservation_is_typed_and_never_creates_an_audio_file() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let store = FloatSpoolStore::new(root.0.clone());
    let result = store.create(
        "too-large",
        AudioAssetMetadata {
            format: AudioFormat::new(44100, 2).unwrap(),
            frame_count: 1u64 << 60,
        },
    );
    assert!(matches!(
        result,
        Err(atomusicator_domain::DomainError::InsufficientResources { .. })
    ));
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 0);
}
use atomusicator_infrastructure::{
    audio_tools::TemporaryDirectory,
    storage::float_spool::{FloatSpool, FloatSpoolStore},
};

#[test]
fn spool_round_trips_frames_and_rejects_modified_or_incomplete_artifacts() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let store = FloatSpoolStore::new(root.0.clone());
    let metadata = AudioAssetMetadata {
        format: AudioFormat::new(44100, 2).unwrap(),
        frame_count: 3,
    };
    let mut writer = store.create("source", metadata).unwrap();
    writer.write(&[0.1, -0.2, 0.3, -0.4]).unwrap();
    writer.write(&[0.5, -0.6]).unwrap();
    let source = writer.finish().unwrap();
    assert_eq!(source.metadata(), metadata);
    assert_eq!(source.peak(), 0.6);
    let mut buffer = [99.0; 8];
    assert_eq!(source.read_frames(1, &mut buffer).unwrap(), 2);
    assert_eq!(&buffer, &[0.3, -0.4, 0.5, -0.6, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(source.read_frames(u64::MAX, &mut buffer).unwrap(), 0);
    let manifest = std::fs::read_dir(&root.0)
        .unwrap()
        .filter_map(Result::ok)
        .find(|p| p.path().extension().is_some_and(|e| e == "json"))
        .unwrap()
        .path();
    let opened = FloatSpool::open(&manifest, &CancellationToken::default()).unwrap();
    assert_eq!(source.content_hash(), opened.content_hash());
    std::fs::write(manifest.with_extension("f32"), [0u8; 24]).unwrap();
    assert!(FloatSpool::open(&manifest, &CancellationToken::default()).is_err());
    let mut incomplete = store.create("incomplete", metadata).unwrap();
    incomplete.write(&[0.0, 0.0]).unwrap();
    assert!(incomplete.finish().is_err());
    let mut nonfinite = store.create("nonfinite", metadata).unwrap();
    assert!(nonfinite.write(&[f32::NAN, 0.0]).is_err());
}

#[test]
fn cache_cleanup_cannot_remove_a_leased_audio_artifact() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let store = FloatSpoolStore::new(root.0.clone());
    let metadata = AudioAssetMetadata {
        format: AudioFormat::new(44100, 2).unwrap(),
        frame_count: 16,
    };
    let mut writer = store.create("leased", metadata).unwrap();
    writer.write(&[0.25; 32]).unwrap();
    let source = writer.finish().unwrap();
    assert_eq!(store.prune(0).unwrap(), 0);
    let mut buffer = [0.0; 32];
    assert_eq!(source.read_frames(0, &mut buffer).unwrap(), 16);
    drop(source);
    assert_eq!(store.prune(0).unwrap(), 128);
}
