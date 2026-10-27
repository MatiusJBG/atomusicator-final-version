use atomusicator_application::{
    CancellationToken,
    ports::*,
    use_cases::{
        bounded_separation::BoundedSeparation, separate_five_stems::SeparateFiveStemsUseCase,
    },
};
use atomusicator_domain::{AudioAssetMetadata, AudioBufferF32, AudioFormat, DomainError};
use atomusicator_infrastructure::{
    audio_tools::TemporaryDirectory, storage::float_spool::FloatSpoolStore,
};
use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
struct Inference(Arc<AtomicUsize>);
fn scale(a: &AudioBufferF32, gain: f32) -> AudioBufferF32 {
    AudioBufferF32::new(a.format(), a.samples().iter().map(|v| v * gain).collect()).unwrap()
}
impl StemInferencePort for Inference {
    fn infer_vocal_bus(
        &self,
        a: &AudioBufferF32,
        _: &CancellationToken,
    ) -> Result<AudioBufferF32, DomainError> {
        Ok(scale(a, 0.4))
    }
    fn infer_lead_vocal(
        &self,
        a: &AudioBufferF32,
        _: &CancellationToken,
    ) -> Result<AudioBufferF32, DomainError> {
        Ok(scale(a, 0.6))
    }
    fn infer_rhythm_section(
        &self,
        a: &AudioBufferF32,
        _: &CancellationToken,
    ) -> Result<RhythmStems, DomainError> {
        let drums = scale(a, 0.2);
        let residual = AudioBufferF32::new(
            a.format(),
            a.samples()
                .iter()
                .zip(drums.samples())
                .map(|(a, b)| a - b)
                .collect(),
        )?;
        Ok(RhythmStems {
            drums,
            bass: scale(&residual, 0.3),
        })
    }
}
impl BlockStemInferencePort for Inference {
    fn stage_identity(&self, stage: InferenceStage) -> String {
        format!("test-v1/{stage:?}")
    }
    fn infer_blocks(
        &self,
        stage: InferenceStage,
        source: &dyn AudioBlockSource,
        target: &mut dyn AudioBlockWriter,
        _: &CancellationToken,
    ) -> Result<(), DomainError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let gain = match stage {
            InferenceStage::VocalBus => 0.4,
            InferenceStage::LeadVocal => 0.6,
            InferenceStage::Drums => 0.2,
            InferenceStage::Bass => 0.3,
        };
        let mut samples = vec![0.0; 8192];
        let mut offset = 0;
        loop {
            let count = source.read_frames(offset, &mut samples)?;
            if count == 0 {
                break;
            }
            for v in &mut samples[..count * 2] {
                *v *= gain;
            }
            target.write(&samples[..count * 2])?;
            offset += count as u64;
        }
        Ok(())
    }
}
struct Gate;
impl SpectralBleedGatePort for Gate {
    fn rebalance(
        &self,
        a: &AudioBufferF32,
        b: &AudioBufferF32,
    ) -> Result<(AudioBufferF32, AudioBufferF32), DomainError> {
        Ok((a.clone(), b.clone()))
    }
}
struct Progress;
impl SeparationProgressPort for Progress {
    fn report(&self, _: u8, _: u8, _: &str) {}
}
#[test]
fn bounded_pipeline_preserves_vad_residual_order_and_reuses_verified_stages() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let store = FloatSpoolStore::new(root.0.clone());
    let frames = 44100 * 9;
    let format = AudioFormat::new(44100, 2).unwrap();
    let samples: Vec<f32> = (0..frames * 2)
        .map(|i| {
            if i < 44100 * 8 * 2 {
                1e-6
            } else {
                (i as f32 * 0.1).sin() * 0.5
            }
        })
        .collect();
    let audio = AudioBufferF32::new(format, samples.clone()).unwrap();
    let mut writer = store
        .create(
            "source",
            AudioAssetMetadata {
                format,
                frame_count: frames as u64,
            },
        )
        .unwrap();
    writer.write(&samples).unwrap();
    let source = writer.finish().unwrap();
    let cancel = CancellationToken::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let expected = SeparateFiveStemsUseCase::new(Inference(Arc::clone(&calls)), Progress, Gate)
        .execute(&audio, &cancel)
        .unwrap();
    let bounded = BoundedSeparation::new(Inference(Arc::clone(&calls)), Progress, Gate, store);
    let actual = bounded.execute(Arc::clone(&source), &cancel).unwrap();
    for (kind, artifact) in actual {
        let mut values = vec![0.0; samples.len()];
        artifact.read_frames(0, &mut values).unwrap();
        assert_eq!(values, expected.stem(kind).audio.samples());
    }
    let first = calls.load(Ordering::SeqCst);
    bounded.execute(source, &cancel).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), first);
}

#[test]
#[ignore = "requires the four manifest-verified local ONNX models"]
fn bounded_real_models_match_the_frozen_memory_pipeline() {
    use atomusicator_infrastructure::{
        mdx::MdxInference, models::manifest_manager::ManifestModelManager, onnx::OrtSessionPool,
        storage::paths::WorkspacePathPolicy, use_case_adapters::StftSpectralBleedGate,
    };
    let policy = WorkspacePathPolicy::system_default().unwrap();
    let manager = ManifestModelManager::from_embedded_manifest(policy.clone()).unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut models = BTreeMap::new();
    for profile in &manager.manifest().models {
        let path = policy
            .model_candidates(&profile.filename)
            .unwrap()
            .into_iter()
            .find(|p| p.is_file())
            .unwrap();
        assert_eq!(
            runtime
                .block_on(ManifestModelManager::verify_model_file(&path))
                .unwrap(),
            profile.sha256
        );
        models.insert(profile.profile_id.clone(), (path, profile.sha256.clone()));
    }
    let pool = Arc::new(OrtSessionPool::new(2).unwrap());
    let cancel = CancellationToken::default();
    let format = AudioFormat::new(44100, 2).unwrap();
    let audio = AudioBufferF32::new(
        format,
        (0..44100)
            .flat_map(|i| {
                let v = (i as f32 * 0.137).sin() * 0.2;
                [v, v * 0.7]
            })
            .collect(),
    )
    .unwrap();
    let reference = SeparateFiveStemsUseCase::new(
        MdxInference::with_session_pool(models.clone(), None, Arc::clone(&pool)).unwrap(),
        Progress,
        StftSpectralBleedGate::new(2048, 0.05, 0.25).unwrap(),
    )
    .execute(&audio, &cancel)
    .unwrap();
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let store = FloatSpoolStore::new(root.0.clone());
    let mut writer = store
        .create(
            "source",
            AudioAssetMetadata {
                format,
                frame_count: audio.frames() as u64,
            },
        )
        .unwrap();
    writer.write(audio.samples()).unwrap();
    let source = writer.finish().unwrap();
    let bounded = BoundedSeparation::new(
        MdxInference::with_session_pool(models, None, pool).unwrap(),
        Progress,
        StftSpectralBleedGate::new(2048, 0.05, 0.25).unwrap(),
        store,
    )
    .execute(source, &cancel)
    .unwrap();
    for (kind, artifact) in bounded {
        let mut samples = vec![0.0; audio.samples().len()];
        artifact.read_frames(0, &mut samples).unwrap();
        let error = samples
            .iter()
            .zip(reference.stem(kind).audio.samples())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(error <= 1e-6, "{kind:?}: {error}");
    }
}
