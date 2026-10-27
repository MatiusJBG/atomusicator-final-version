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
