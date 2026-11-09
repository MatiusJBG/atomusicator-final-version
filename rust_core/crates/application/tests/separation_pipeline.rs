use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use atomusicator_application::{
    CancellationToken,
    ports::{RhythmStems, SeparationProgressPort, SpectralBleedGatePort, StemInferencePort},
    use_cases::separate_five_stems::SeparateFiveStemsUseCase,
};
use atomusicator_domain::{AudioBufferF32, AudioFormat, DomainError, StemType};

struct MockInference {
    vocal_bus: AudioBufferF32,
    lead: AudioBufferF32,
    drums: AudioBufferF32,
    bass: AudioBufferF32,
    lead_calls: Arc<AtomicUsize>,
}

impl StemInferencePort for MockInference {
    fn infer_vocal_bus(
        &self,
        _: &AudioBufferF32,
        _: &CancellationToken,
    ) -> Result<AudioBufferF32, DomainError> {
        Ok(self.vocal_bus.clone())
    }
    fn infer_lead_vocal(
        &self,
        _: &AudioBufferF32,
        _: &CancellationToken,
    ) -> Result<AudioBufferF32, DomainError> {
        self.lead_calls.fetch_add(1, Ordering::Relaxed);
        Ok(self.lead.clone())
    }
    fn infer_rhythm_section(
        &self,
        _: &AudioBufferF32,
        _: &CancellationToken,
    ) -> Result<RhythmStems, DomainError> {
        Ok(RhythmStems {
            drums: self.drums.clone(),
            bass: self.bass.clone(),
        })
    }
}

struct NoopProgress;
impl SeparationProgressPort for NoopProgress {
    fn report(&self, _: u8, _: u8, _: &str) {}
}

struct IdentityBleedGate;
impl SpectralBleedGatePort for IdentityBleedGate {
    fn rebalance(
        &self,
        lead: &AudioBufferF32,
        backing: &AudioBufferF32,
    ) -> Result<(AudioBufferF32, AudioBufferF32), DomainError> {
        Ok((lead.clone(), backing.clone()))
    }
}

fn audio(format: AudioFormat, samples: &[f32]) -> AudioBufferF32 {
    AudioBufferF32::new(format, samples.to_vec()).unwrap()
}

#[test]
fn pipeline_returns_five_stems_that_reconstruct_the_mix() {
    let format = AudioFormat::new(48_000, 1).unwrap();
    let mix = audio(format, &[1.0, -0.5, 0.25, 0.75]);
    let inference = MockInference {
        vocal_bus: audio(format, &[0.3, -0.2, 0.1, 0.2]),
        lead: audio(format, &[0.1, -0.1, 0.05, 0.1]),
        drums: audio(format, &[0.2, 0.0, 0.05, 0.1]),
        bass: audio(format, &[0.1, -0.1, 0.0, 0.05]),
        lead_calls: Arc::new(AtomicUsize::new(0)),
    };
    let use_case = SeparateFiveStemsUseCase::new(inference, NoopProgress, IdentityBleedGate);
    let result = use_case
        .execute(&mix, &CancellationToken::default())
        .unwrap();
    assert_eq!(result.stems().len(), 5);
    assert_eq!(
        result.stem(StemType::LeadVocal).audio.frames(),
        mix.frames()
    );
}

#[test]
fn vad_skips_quiet_chunks_and_routes_their_energy_to_the_instrumental_residual() {
    let format = AudioFormat::new(48_000, 1).unwrap();
    let mix = audio(format, &[0.8, -0.4, 0.25, 0.1]);
    let lead_calls = Arc::new(AtomicUsize::new(0));
    let inference = MockInference {
        vocal_bus: audio(format, &[0.000001; 4]),
        lead: audio(format, &[0.0; 4]),
        drums: audio(format, &[0.0; 4]),
        bass: audio(format, &[0.0; 4]),
        lead_calls: lead_calls.clone(),
    };
    let use_case = SeparateFiveStemsUseCase::new(inference, NoopProgress, IdentityBleedGate);
    let result = use_case
        .execute(&mix, &CancellationToken::default())
        .unwrap();
    assert_eq!(lead_calls.load(Ordering::Relaxed), 0);
    assert!(
        result
            .stem(StemType::LeadVocal)
            .audio
            .samples()
            .iter()
            .all(|sample| *sample == 0.0)
    );
    assert!(
        result
            .stem(StemType::BackingVocals)
            .audio
            .samples()
            .iter()
            .all(|sample| *sample == 0.0)
    );
    assert_eq!(
        result.stem(StemType::Instrumental).audio.samples(),
        mix.samples()
    );
}

#[test]
fn context_candidate_preserves_quiet_vocals_without_the_compatibility_gate() {
    use atomusicator_application::use_cases::separate_five_stems::VocalPolicy;
    struct ForbiddenGate;
    impl SpectralBleedGatePort for ForbiddenGate {
        fn rebalance(
            &self,
            _: &AudioBufferF32,
            _: &AudioBufferF32,
        ) -> Result<(AudioBufferF32, AudioBufferF32), DomainError> {
            panic!("context candidate must not apply an unqualified bleed gate");
        }
    }
    let format = AudioFormat::new(48000, 1).unwrap();
    let lead_calls = Arc::new(AtomicUsize::new(0));
    let inference = MockInference {
        vocal_bus: audio(format, &[0.000001; 4]),
        lead: audio(format, &[0.0000006; 4]),
        drums: audio(format, &[0.0; 4]),
        bass: audio(format, &[0.0; 4]),
        lead_calls: Arc::clone(&lead_calls),
    };
    let pipeline = SeparateFiveStemsUseCase::new(inference, NoopProgress, ForbiddenGate)
        .with_vocal_policy(VocalPolicy::ContextPreservingCandidateV1);
    let result = pipeline
        .execute(&audio(format, &[0.1; 4]), &CancellationToken::default())
        .unwrap();
    assert_eq!(lead_calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        result.stem(StemType::LeadVocal).audio.samples(),
        &[0.0000006; 4]
    );
    assert!(
        result
            .stem(StemType::BackingVocals)
            .audio
            .samples()
            .iter()
            .all(|sample| *sample > 0.0)
    );
}

#[test]
fn bounded_two_stems_and_five_stems_reconstruct_with_zero_loss() {
    use atomusicator_application::{
        ports::{
            AudioBlockSource, AudioBlockStore, AudioBlockWriter, BlockStemInferencePort,
            InferenceStage,
        },
        use_cases::bounded_separation::{BoundedSeparation, SeparationMode},
    };
    use atomusicator_domain::AudioAssetMetadata;

    #[derive(Clone)]
    struct MemSource {
        metadata: AudioAssetMetadata,
        samples: Arc<Vec<f32>>,
        hash: String,
    }
    impl AudioBlockSource for MemSource {
        fn as_any(&self) -> &(dyn std::any::Any + 'static) {
            self
        }
        fn metadata(&self) -> AudioAssetMetadata {
            self.metadata
        }
        fn content_hash(&self) -> &str {
            &self.hash
        }
        fn peak(&self) -> f32 {
            self.samples.iter().fold(0.0f32, |m, v| m.max(v.abs()))
        }
        fn read_frames(&self, start_frame: u64, out: &mut [f32]) -> Result<usize, DomainError> {
            let ch = usize::from(self.metadata.format.channels);
            let start = (start_frame as usize) * ch;
            if start >= self.samples.len() {
                return Ok(0);
            }
            let available_frames = (self.samples.len() - start) / ch;
            let requested_frames = out.len() / ch;
            let frames = available_frames.min(requested_frames);
            out[..frames * ch].copy_from_slice(&self.samples[start..start + frames * ch]);
            Ok(frames)
        }
    }

    struct MemWriter {
        metadata: AudioAssetMetadata,
        samples: Vec<f32>,
        tag: String,
    }
    impl AudioBlockWriter for MemWriter {
        fn write(&mut self, samples: &[f32]) -> Result<(), DomainError> {
            self.samples.extend_from_slice(samples);
            Ok(())
        }
        fn finish(self: Box<Self>) -> Result<Arc<dyn AudioBlockSource>, DomainError> {
            Ok(Arc::new(MemSource {
                metadata: self.metadata,
                samples: Arc::new(self.samples),
                hash: self.tag,
            }))
        }
    }

    #[derive(Default)]
    struct MemStore;
    impl AudioBlockStore for MemStore {
        fn create(
            &self,
            tag: &str,
            metadata: AudioAssetMetadata,
        ) -> Result<Box<dyn AudioBlockWriter>, DomainError> {
            Ok(Box::new(MemWriter {
                metadata,
                samples: Vec::new(),
                tag: tag.to_string(),
            }))
        }
        fn cached(
            &self,
            _: &str,
            _: &str,
            _: &[Arc<dyn AudioBlockSource>],
            _: &CancellationToken,
        ) -> Result<Option<Arc<dyn AudioBlockSource>>, DomainError> {
            Ok(None)
        }
        fn remember(
            &self,
            _: &str,
            _: &str,
            _: &[Arc<dyn AudioBlockSource>],
            _: &Arc<dyn AudioBlockSource>,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    struct BlockInf(Arc<AtomicUsize>);
    impl StemInferencePort for BlockInf {
        fn infer_vocal_bus(
            &self,
            a: &AudioBufferF32,
            _: &CancellationToken,
        ) -> Result<AudioBufferF32, DomainError> {
            AudioBufferF32::new(a.format(), a.samples().iter().map(|v| v * 0.4).collect())
        }
        fn infer_lead_vocal(
            &self,
            a: &AudioBufferF32,
            _: &CancellationToken,
        ) -> Result<AudioBufferF32, DomainError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            AudioBufferF32::new(a.format(), a.samples().iter().map(|v| v * 0.6).collect())
        }
        fn infer_rhythm_section(
            &self,
            a: &AudioBufferF32,
            _: &CancellationToken,
        ) -> Result<RhythmStems, DomainError> {
            Ok(RhythmStems {
                drums: AudioBufferF32::new(
                    a.format(),
                    a.samples().iter().map(|v| v * 0.2).collect(),
                )?,
                bass: AudioBufferF32::new(
                    a.format(),
                    a.samples().iter().map(|v| v * 0.3).collect(),
                )?,
            })
        }
    }
    impl BlockStemInferencePort for BlockInf {
        fn stage_identity(&self, stage: InferenceStage) -> String {
            format!("mem-{stage:?}")
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
            let ch = usize::from(source.metadata().format.channels);
            let mut buf = vec![0.0; 4096 * ch];
            let mut offset = 0;
            loop {
                let n = source.read_frames(offset, &mut buf)?;
                if n == 0 {
                    break;
                }
                for s in &mut buf[..n * ch] {
                    *s *= gain;
                }
                target.write(&buf[..n * ch])?;
                offset += n as u64;
            }
            Ok(())
        }
    }

    let format = AudioFormat::new(48_000, 2).unwrap();
    let frames = 10_000usize;
    let raw_samples: Vec<f32> = (0..frames * 2)
        .map(|i| (i as f32 * 0.031).sin() * 0.75)
        .collect();
    let mix: Arc<dyn AudioBlockSource> = Arc::new(MemSource {
        metadata: AudioAssetMetadata {
            format,
            frame_count: frames as u64,
        },
        samples: Arc::new(raw_samples.clone()),
        hash: "mix".into(),
    });

    // 1. TwoStems mode: 1 neural pass + exact 0% loss reconstruction
    let calls_2 = Arc::new(AtomicUsize::new(0));
    let two_stems = BoundedSeparation::new(
        BlockInf(Arc::clone(&calls_2)),
        NoopProgress,
        IdentityBleedGate,
        MemStore,
    )
    .with_separation_mode(SeparationMode::TwoStems)
    .execute(Arc::clone(&mix), &CancellationToken::default())
    .unwrap();
    assert_eq!(calls_2.load(Ordering::SeqCst), 1);

    // 2. FiveStems mode: 4 neural passes + exact 0% loss reconstruction
    let calls_5 = Arc::new(AtomicUsize::new(0));
    let five_stems = BoundedSeparation::new(
        BlockInf(Arc::clone(&calls_5)),
        NoopProgress,
        IdentityBleedGate,
        MemStore,
    )
    .with_separation_mode(SeparationMode::FiveStems)
    .execute(Arc::clone(&mix), &CancellationToken::default())
    .unwrap();
    assert_eq!(calls_5.load(Ordering::SeqCst), 4);

    for outputs in [&two_stems, &five_stems] {
        let mut sum = vec![0.0f32; raw_samples.len()];
        for kind in StemType::ALL {
            let mut buf = vec![0.0f32; raw_samples.len()];
            assert_eq!(outputs[&kind].read_frames(0, &mut buf).unwrap(), frames);
            for (s, v) in sum.iter_mut().zip(&buf) {
                *s += *v;
            }
        }
        for (idx, (&orig, &recon)) in raw_samples.iter().zip(&sum).enumerate() {
            assert!(
                (orig - recon).abs() <= 1e-6,
                "sample {idx} mismatch: {orig} vs {recon}"
            );
        }
    }
}
