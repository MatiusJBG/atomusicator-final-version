use atomusicator_domain::{AudioBufferF32, DomainError, SeparationResult, Stem, StemType};

use crate::{
    CancellationToken,
    ports::{RhythmStems, SeparationProgressPort, SpectralBleedGatePort, StemInferencePort},
};

/// Coordinates the three model inferences and exact residual subtractions.
/// Model selection and runtime details remain behind `StemInferencePort`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VocalPolicy {
    #[default]
    CompatibilityV1,
    /// Evaluation only; not qualified for promotion to the desktop default.
    ContextPreservingCandidateV1,
}
impl VocalPolicy {
    pub fn id(self) -> &'static str {
        match self {
            Self::CompatibilityV1 => "native-v1",
            Self::ContextPreservingCandidateV1 => "context-candidate-v1",
        }
    }
}

pub struct SeparateFiveStemsUseCase<I, P, G> {
    inference: I,
    progress: P,
    bleed_gate: G,
    vocal_policy: VocalPolicy,
}

impl<I, P, G> SeparateFiveStemsUseCase<I, P, G>
where
    I: StemInferencePort,
    P: SeparationProgressPort,
    G: SpectralBleedGatePort,
{
    pub fn new(inference: I, progress: P, bleed_gate: G) -> Self {
        Self {
            inference,
            progress,
            bleed_gate,
            vocal_policy: VocalPolicy::default(),
        }
    }

    pub fn with_vocal_policy(mut self, policy: VocalPolicy) -> Self {
        self.vocal_policy = policy;
        self
    }

    pub fn execute(
        &self,
        mix: &AudioBufferF32,
        cancel: &CancellationToken,
    ) -> Result<SeparationResult, DomainError> {
        check_cancelled(cancel)?;
        self.progress.report(0, 3, "Isolating vocal bus");
        let vocal_bus = self.inference.infer_vocal_bus(mix, cancel)?;
        self.progress.report(1, 3, "Separating vocal stems");
        check_alignment(mix, &vocal_bus)?;

        let (lead, backing, active_vocal_bus) = match self.vocal_policy {
            VocalPolicy::CompatibilityV1 => {
                infer_vocals_with_vad(&self.inference, &self.bleed_gate, &vocal_bus, cancel)?
            }
            VocalPolicy::ContextPreservingCandidateV1 => {
                let lead = self.inference.infer_lead_vocal(&vocal_bus, cancel)?;
                check_alignment(&vocal_bus, &lead)?;
                let backing = subtract(&vocal_bus, &lead)?;
                (lead, backing, vocal_bus)
            }
        };

        self.progress.report(2, 3, "Separating rhythm stems");
        check_cancelled(cancel)?;
        let instrumental_master = subtract(mix, &active_vocal_bus)?;
        let RhythmStems { drums, bass } = self
            .inference
            .infer_rhythm_section(&instrumental_master, cancel)?;
        check_alignment(mix, &drums)?;
        check_alignment(mix, &bass)?;
        let instrumental = subtract(&instrumental_master, &add(&drums, &bass)?)?;

        check_cancelled(cancel)?;
        self.progress.report(3, 3, "Validating reconstruction");
        SeparationResult::try_new(
            mix,
            [
                Stem {
                    kind: StemType::LeadVocal,
                    audio: lead,
                },
                Stem {
                    kind: StemType::BackingVocals,
                    audio: backing,
                },
                Stem {
                    kind: StemType::Drums,
                    audio: drums,
                },
                Stem {
                    kind: StemType::Bass,
                    audio: bass,
                },
                Stem {
                    kind: StemType::Instrumental,
                    audio: instrumental,
                },
            ],
        )
    }
}

fn infer_vocals_with_vad<I, G>(
    inference: &I,
    bleed_gate: &G,
    vocal_bus: &AudioBufferF32,
    cancel: &CancellationToken,
) -> Result<(AudioBufferF32, AudioBufferF32, AudioBufferF32), DomainError>
where
    I: StemInferencePort,
    G: SpectralBleedGatePort,
{
    const CHUNK_SECONDS: usize = 8;
    const THRESHOLD_DBFS: f64 = -65.0;
    let channels = usize::from(vocal_bus.format().channels);
    let chunk_frames = vocal_bus.format().sample_rate_hz as usize * CHUNK_SECONDS;
    let mut lead_samples = Vec::with_capacity(vocal_bus.samples().len());
    let mut backing_samples = Vec::with_capacity(vocal_bus.samples().len());
    let mut active_vocal_samples = Vec::with_capacity(vocal_bus.samples().len());

    for samples in vocal_bus.samples().chunks(chunk_frames * channels) {
        check_cancelled(cancel)?;
        let chunk = AudioBufferF32::new(vocal_bus.format(), samples.to_vec())?;
        let rms = (samples
            .iter()
            .map(|sample| f64::from(*sample) * f64::from(*sample))
            .sum::<f64>()
            / samples.len() as f64)
            .sqrt();
        let dbfs = if rms <= f64::MIN_POSITIVE {
            f64::NEG_INFINITY
        } else {
            20.0 * rms.log10()
        };
        if dbfs < THRESHOLD_DBFS {
            lead_samples.resize(lead_samples.len() + samples.len(), 0.0);
            backing_samples.resize(backing_samples.len() + samples.len(), 0.0);
            active_vocal_samples.resize(active_vocal_samples.len() + samples.len(), 0.0);
            continue;
        }
        active_vocal_samples.extend_from_slice(samples);
        let lead_chunk = inference.infer_lead_vocal(&chunk, cancel)?;
        check_alignment(&chunk, &lead_chunk)?;
        let raw_backing_chunk = subtract(&chunk, &lead_chunk)?;
        let (gated_lead, gated_backing) = bleed_gate.rebalance(&lead_chunk, &raw_backing_chunk)?;
        check_alignment(&chunk, &gated_lead)?;
        check_alignment(&chunk, &gated_backing)?;
        lead_samples.extend_from_slice(gated_lead.samples());
        backing_samples.extend_from_slice(gated_backing.samples());
    }

    Ok((
        AudioBufferF32::new(vocal_bus.format(), lead_samples)?,
        AudioBufferF32::new(vocal_bus.format(), backing_samples)?,
        AudioBufferF32::new(vocal_bus.format(), active_vocal_samples)?,
    ))
}

fn check_cancelled(cancel: &CancellationToken) -> Result<(), DomainError> {
    if cancel.is_cancelled() {
        Err(DomainError::SeparationCancelled)
    } else {
        Ok(())
    }
}

fn check_alignment(
    reference: &AudioBufferF32,
    candidate: &AudioBufferF32,
) -> Result<(), DomainError> {
    if reference.format() != candidate.format() || reference.frames() != candidate.frames() {
        return Err(DomainError::AudioAlignmentMismatch);
    }
    Ok(())
}

fn subtract(left: &AudioBufferF32, right: &AudioBufferF32) -> Result<AudioBufferF32, DomainError> {
    check_alignment(left, right)?;
    AudioBufferF32::new(
        left.format(),
        left.samples()
            .iter()
            .zip(right.samples())
            .map(|(a, b)| a - b)
            .collect(),
    )
}

fn add(left: &AudioBufferF32, right: &AudioBufferF32) -> Result<AudioBufferF32, DomainError> {
    check_alignment(left, right)?;
    AudioBufferF32::new(
        left.format(),
        left.samples()
            .iter()
            .zip(right.samples())
            .map(|(a, b)| a + b)
            .collect(),
    )
}
