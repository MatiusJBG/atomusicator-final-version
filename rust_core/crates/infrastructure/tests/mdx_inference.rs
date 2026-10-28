use atomusicator_domain::{AudioBufferF32, AudioFormat};
use atomusicator_infrastructure::mdx::{MdxConfig, MdxTransform};

#[test]
#[ignore = "requires the four manifest-verified models in the local app cache"]
fn real_models_separate_a_short_stereo_signal_and_reconstruct_it() {
    use atomusicator_application::{
        CancellationToken, ports::SeparationProgressPort,
        use_cases::separate_five_stems::SeparateFiveStemsUseCase,
    };
    use atomusicator_infrastructure::{
        mdx::MdxInference, models::manifest_manager::ManifestModelManager,
        storage::paths::WorkspacePathPolicy, use_case_adapters::StftSpectralBleedGate,
    };
    struct Progress;
    impl SeparationProgressPort for Progress {
        fn report(&self, _: u8, _: u8, message: &str) {
            eprintln!("{message}");
        }
    }
    let manager = ManifestModelManager::from_embedded_manifest(
        WorkspacePathPolicy::system_default().unwrap(),
    )
    .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut models = std::collections::BTreeMap::new();
    for profile in &manager.manifest().models {
        // No network or silent downloads in this smoke test.
        let policy = WorkspacePathPolicy::system_default().unwrap();
        assert!(
            policy
                .model_candidates(&profile.filename)
                .unwrap()
                .iter()
                .any(|p| p.is_file())
        );
        let verified = runtime
            .block_on(manager.ensure_profile_ready(&profile.profile_id))
            .unwrap();
        models.insert(
            verified.profile_id,
            (verified.local_path, verified.sha256_hash),
        );
    }
    let audio = AudioBufferF32::new(
        AudioFormat::new(44100, 2).unwrap(),
        (0..22050)
            .flat_map(|i| {
                let value = (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / 44100.0).sin() * 0.2;
                [value, value * 0.7]
            })
            .collect(),
    )
    .unwrap();
    let pipeline = SeparateFiveStemsUseCase::new(
        MdxInference::new(models, None).unwrap(),
        Progress,
        StftSpectralBleedGate::new(2048, 0.05, 0.25).unwrap(),
    );
    let result = pipeline
        .execute(&audio, &CancellationToken::default())
        .unwrap();
    assert_eq!(result.stems().len(), 5);
    for stem in result.stems() {
        assert_eq!(stem.audio.frames(), audio.frames());
    }
    assert!(
        result
            .stems()
            .any(|s| s.audio.samples().iter().any(|x| x.abs() > 1e-5))
    );
}

#[test]
fn mdx_centered_stft_roundtrips_stereo_with_non_power_of_two_fft() {
    let config = MdxConfig {
        n_fft: 60,
        hop: 12,
        dim_f: 31,
        dim_t: 16,
        compensate: 1.0,
        invert: false,
    };
    let transform = MdxTransform::new(config).unwrap();
    let samples: Vec<f32> = (0..360).map(|i| (i as f32 * 0.18).sin() * 0.3).collect();
    let spectrum = transform.forward(&samples).unwrap();
    assert_eq!(spectrum.len(), 4 * 31 * 16);
    let result = transform.inverse(&spectrum).unwrap();
    assert_eq!(result.len(), samples.len());
    for (a, b) in result.iter().zip(&samples) {
        assert!((a - b).abs() < 1e-5, "{a} != {b}");
    }
}

#[test]
fn mdx_tensor_layout_matches_independent_torch_stft_fixture() {
    let config = MdxConfig {
        n_fft: 60,
        hop: 12,
        dim_f: 31,
        dim_t: 16,
        compensate: 1.0,
        invert: false,
    };
    let samples = (0..360)
        .map(|i| ((i % 13) as f32 - 6.0) / 32.0)
        .collect::<Vec<_>>();
    let spectrum = MdxTransform::new(config)
        .unwrap()
        .forward(&samples)
        .unwrap();
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/mdx_torch_stft.json")).unwrap();
    for pair in fixture["values"].as_array().unwrap() {
        let index = pair[0].as_u64().unwrap() as usize;
        let expected = pair[1].as_f64().unwrap() as f32;
        assert!(
            (spectrum[index] - expected).abs() < 1e-5,
            "tensor[{index}]: {} != {expected}",
            spectrum[index]
        );
    }
}

#[test]
fn reviewed_profiles_include_karas_inverse_target_and_native_shapes() {
    let main = MdxConfig::for_profile("uvr-mdxnet-main-v1").unwrap();
    assert_eq!((main.n_fft, main.dim_f, main.dim_t), (7680, 3072, 256));
    assert!(
        MdxConfig::for_profile("uvr-mdxnet-kara-2-v1")
            .unwrap()
            .invert
    );
    assert_eq!(
        MdxConfig::for_profile("kuielab-mdxnet-bass-v1")
            .unwrap()
            .dim_t,
        512
    );
    assert!(MdxConfig::for_profile("unknown").is_err());
}

#[test]
fn mdx_rejects_invalid_shapes_without_panicking() {
    let config = MdxConfig::for_profile("uvr-mdxnet-main-v1").unwrap();
    let transform = MdxTransform::new(config).unwrap();
    assert!(transform.forward(&[0.0, 1.0]).is_err());
    assert!(transform.inverse(&[f32::NAN]).is_err());
    assert!(AudioBufferF32::new(AudioFormat::new(44100, 2).unwrap(), vec![f32::NAN; 2]).is_err());
}
