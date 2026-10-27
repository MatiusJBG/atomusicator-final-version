use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use atomusicator_application::ports::{
    ModelRegistryError, ModelRegistryPort, StageArtifactCachePort, StageCacheKey,
};
use atomusicator_domain::{
    AudioAsset, AudioAssetId, AudioBufferF32, AudioFormat, SeparationResult, Stem, StemType,
};
use atomusicator_infrastructure::{
    cache::ContentAddressedCache, models::manifest_manager::ManifestModelManager,
    session_workspace::LocalSessionWorkspace, storage::paths::WorkspacePathPolicy,
};

fn temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "atomusicator-phase2-{label}-{}-{nonce}",
        std::process::id()
    ))
}

fn manifest(file_name: &str, sha256: &str) -> String {
    serde_json::json!({
        "manifest_version": 1,
        "models": [{
            "profile_id": "test-profile",
            "stage": "test-stage",
            "filename": file_name,
            "sha256": sha256,
            "download_url": "https://example.invalid/model.onnx",
            "license": "MIT",
            "license_status": "reviewed",
            "attribution": "Test attribution"
        }]
    })
    .to_string()
}

fn sample_buffer(samples: Vec<f32>) -> AudioBufferF32 {
    AudioBufferF32::new(AudioFormat::new(48_000, 2).unwrap(), samples).unwrap()
}

#[test]
fn workspace_paths_use_portable_app_data_and_legacy_projects_when_present() {
    let base = temp_dir("paths");
    let policy = WorkspacePathPolicy::from_local_data_dir(&base);
    assert_eq!(policy.app_data_root(), base.join("Atomusicator"));
    assert_eq!(
        policy.default_projects_dir(),
        base.join("Atomusicator/projects")
    );

    let legacy_projects = base.join("Separador Vocal/projects");
    fs::create_dir_all(&legacy_projects).unwrap();
    let policy = WorkspacePathPolicy::from_local_data_dir(&base);
    assert_eq!(policy.default_projects_dir(), legacy_projects);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn workspace_adopts_the_actual_nested_python_cache_and_projects() {
    let base = temp_dir("python-paths");
    let legacy = base.join("Separador Vocal/Separador Vocal");
    fs::create_dir_all(legacy.join("models")).unwrap();
    fs::create_dir_all(legacy.join("projects")).unwrap();
    let policy = WorkspacePathPolicy::from_local_data_dir(&base);
    assert_eq!(policy.default_projects_dir(), legacy.join("projects"));
    assert!(
        policy
            .model_candidates("model.onnx")
            .unwrap()
            .contains(&legacy.join("models/model.onnx"))
    );
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn workspace_policy_rejects_unc_and_filesystem_roots() {
    assert!(
        WorkspacePathPolicy::validate_projects_destination(std::path::Path::new(
            r"\\server\share\projects"
        ))
        .is_err()
    );
    assert!(
        WorkspacePathPolicy::validate_projects_destination(std::path::Path::new(
            std::path::MAIN_SEPARATOR_STR
        ))
        .is_err()
    );
}

#[tokio::test]
async fn manifest_registry_accepts_a_verified_legacy_model_without_network_access() {
    use sha2::{Digest, Sha256};

    let base = temp_dir("manifest-legacy");
    let bytes = b"verified test model";
    let expected = format!("{:x}", Sha256::digest(bytes));
    let legacy_models = base.join("Separador Vocal/models");
    fs::create_dir_all(&legacy_models).unwrap();
    fs::write(legacy_models.join("model.onnx"), bytes).unwrap();
    let manager = ManifestModelManager::from_manifest_json(
        &manifest("model.onnx", &expected),
        WorkspacePathPolicy::from_local_data_dir(&base),
    )
    .unwrap();

    let model = ModelRegistryPort::ensure_profile_ready(&manager, "test-profile")
        .await
        .unwrap();
    assert_eq!(model.profile_id, "test-profile");
    assert_eq!(model.local_path, legacy_models.join("model.onnx"));
    assert_eq!(model.sha256_hash, expected);

    fs::remove_dir_all(base).unwrap();
}

#[tokio::test]
async fn manifest_preparation_can_be_cancelled_before_touching_the_network() {
    let cancel = atomusicator_application::CancellationToken::default();
    cancel.cancel();
    let manager = ManifestModelManager::from_manifest_json(
        &manifest("model.onnx", &"a".repeat(64)),
        WorkspacePathPolicy::from_local_data_dir(temp_dir("cancelled-model")),
    )
    .unwrap();
    let error = manager
        .ensure_profile_ready_cancellable("test-profile", &cancel)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("cancelled"));
}

#[tokio::test]
async fn manifest_registry_rejects_unknown_profiles_without_network_access() {
    let base = temp_dir("manifest-missing");
    fs::create_dir_all(&base).unwrap();
    let manager = ManifestModelManager::from_manifest_json(
        &manifest("model.onnx", &"a".repeat(64)),
        WorkspacePathPolicy::from_local_data_dir(&base),
    )
    .unwrap();

    assert!(matches!(
        ModelRegistryPort::ensure_profile_ready(&manager, "missing-profile").await,
        Err(ModelRegistryError::ProfileNotFound(profile)) if profile == "missing-profile"
    ));
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn typed_stage_cache_keys_cover_runtime_and_dsp_parameters() {
    let root = temp_dir("stage-cache");
    let cache = ContentAddressedCache::new(&root, 8, 1_000_000).unwrap();
    let key = StageCacheKey {
        stage_tag: "vocal_bus".into(),
        input_audio_sha256: "a".repeat(64),
        model_sha256: "b".repeat(64),
        stft_n_fft: 4096,
        stft_hop_length: 1024,
        runtime_signature: "ort-cpu-v1".into(),
    };
    let audio = sample_buffer(vec![0.25, -0.25, 0.5, -0.5]);
    cache.store_checkpoint_non_fatal(&key, &audio);
    assert_eq!(cache.get_verified_checkpoint(&key).unwrap(), Some(audio));

    let changed_runtime = StageCacheKey {
        runtime_signature: "ort-cpu-v2".into(),
        ..key.clone()
    };
    let changed_stft = StageCacheKey {
        stft_hop_length: 512,
        ..key.clone()
    };
    assert!(
        cache
            .get_verified_checkpoint(&changed_runtime)
            .unwrap()
            .is_none()
    );
    assert!(
        cache
            .get_verified_checkpoint(&changed_stft)
            .unwrap()
            .is_none()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn workspace_publishes_all_stems_as_relative_float_wav_files() {
    let root = temp_dir("session");
    let workspace = LocalSessionWorkspace::new(&root);
    let asset = AudioAsset {
        id: AudioAssetId::new("source-a").unwrap(),
        path: root.join("song.wav"),
        display_name: "song.wav".into(),
    };
    let original = sample_buffer(vec![0.1, -0.1, 0.2, -0.2]);
    let silence = sample_buffer(vec![0.0; 4]);
    let result = SeparationResult::try_new(
        &original,
        StemType::ALL.into_iter().map(|kind| Stem {
            kind,
            audio: if kind == StemType::LeadVocal {
                original.clone()
            } else {
                silence.clone()
            },
        }),
    )
    .unwrap();

    let session = workspace
        .persist_separation_result(&asset, &result)
        .unwrap();
    let metadata = workspace.read_metadata(&session).unwrap();
    let separation = &metadata.additional["separation"];
    assert_eq!(separation["stems"].as_object().unwrap().len(), 5);
    assert_eq!(
        separation["stems"]["LeadVocal"]["path"],
        "outputs/01_lead_vocal.wav"
    );
    for kind in StemType::ALL {
        let file_name = match kind {
            StemType::LeadVocal => "01_lead_vocal.wav",
            StemType::BackingVocals => "02_backing_vocals.wav",
            StemType::Drums => "03_drums.wav",
            StemType::Bass => "04_bass.wav",
            StemType::Instrumental => "05_instrumental.wav",
        };
        assert!(session.directory.join("outputs").join(file_name).is_file());
    }
    let output_path = session.directory.join("outputs/01_lead_vocal.wav");
    assert_eq!(
        atomusicator_infrastructure::audio_io::wav_codec::read_wav_f32(&output_path).unwrap(),
        original
    );
    assert!(
        !std::path::Path::new(separation["stems"]["LeadVocal"]["path"].as_str().unwrap())
            .is_absolute()
    );

    fs::remove_dir_all(root).unwrap();
}
