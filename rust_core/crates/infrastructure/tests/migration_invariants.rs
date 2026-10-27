use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use atomusicator_application::ports::ArtifactCachePort;
use atomusicator_domain::{
    AudioAsset, AudioAssetId, AudioAssetMetadata, AudioBufferF32, AudioFormat,
};
use atomusicator_infrastructure::{
    cache::{ArtifactIdentity, ContentAddressedCache},
    model_downloader::ModelManifest,
    session_workspace::LocalSessionWorkspace,
};

fn temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "atomusicator-{label}-{}-{nonce}",
        std::process::id()
    ))
}

#[test]
fn copied_model_manifest_parses_and_references_verified_profiles() {
    let manifest = ModelManifest::parse(include_str!(
        "../../../../assets/manifests/model_license_manifest.json"
    ))
    .unwrap();
    assert_eq!(
        manifest.profile("uvr-mdxnet-main-v1").unwrap().sha256.len(),
        64
    );
    assert_eq!(
        manifest.profile("uvr-mdxnet-kara-2-v1").unwrap().stage,
        "lead_and_backing"
    );
    assert_eq!(
        manifest
            .profile("kuielab-mdxnet-drums-v1")
            .unwrap()
            .license_status,
        "reviewed"
    );
}

#[test]
fn cache_identity_changes_when_any_reuse_dimension_changes() {
    let identity = ArtifactIdentity {
        input_sha256: "a".repeat(64),
        model_sha256: "b".repeat(64),
        runtime_signature: "onnxruntime-cpu-v1".into(),
    };
    let changed_runtime = ArtifactIdentity {
        runtime_signature: "onnxruntime-directml-v1".into(),
        ..identity.clone()
    };
    assert_ne!(
        identity.content_key().unwrap(),
        changed_runtime.content_key().unwrap()
    );
}

#[test]
fn checkpoint_round_trips_through_the_mmap_disk_cache() {
    let root = temp_dir("cache");
    let cache = ContentAddressedCache::new(&root, 2, 1_000_000).unwrap();
    let format = AudioFormat::new(48_000, 2).unwrap();
    let original = AudioBufferF32::new(format, vec![0.25, -0.5, 1.0, 0.0]).unwrap();
    cache.put_checkpoint("test-key", &original).unwrap();
    let restored = cache.get_checkpoint("test-key").unwrap().unwrap();
    assert_eq!(restored, original);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn session_workspace_writes_legacy_metadata_and_outputs_atomically() {
    let root = temp_dir("session");
    let workspace = LocalSessionWorkspace::new(&root);
    let asset = AudioAsset {
        id: AudioAssetId::new("source-a").unwrap(),
        path: PathBuf::from("C:/audio/song.wav"),
        display_name: "song.wav".into(),
    };
    let metadata = AudioAssetMetadata {
        format: AudioFormat::new(48_000, 2).unwrap(),
        frame_count: 96_000,
    };
    let session = workspace.create_session(&asset, metadata, 384_000).unwrap();
    assert_eq!(session.id.len(), 32);
    assert!(session.directory.join("outputs").is_dir());
    let record = workspace.read_metadata(&session).unwrap();
    assert_eq!(record.session_id, session.id);
    assert_eq!(record.duration_seconds, 2.0);
    assert_eq!(record.sample_rate_hz, 48_000);
    assert_eq!(record.channels, 2);
    assert!(session.directory.join("session.json").is_file());
    fs::remove_dir_all(root).unwrap();
}
