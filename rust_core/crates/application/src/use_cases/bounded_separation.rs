//! Serial, duration-independent orchestration. Ports own storage and neural runtime details.
use crate::{
    CancellationToken,
    ports::{
        AudioBlockSource, AudioBlockStore, AudioBlockWriter, BlockStemInferencePort,
        InferenceStage, SeparationProgressPort, SpectralBleedGatePort, StemInferencePort,
    },
};
use atomusicator_domain::{AudioBufferF32, DomainError, SeparationResult, Stem, StemType};
use std::{collections::BTreeMap, sync::Arc};
type Source = Arc<dyn AudioBlockSource>;
const BLOCK: usize = 65536;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RhythmPolicy {
    #[default]
    CompatibilityV1,
    OriginalMixBassV2,
}
impl RhythmPolicy {
    pub fn from_profile(
        profile: &atomusicator_domain::quality::SeparationProfile,
    ) -> Result<Self, DomainError> {
        use atomusicator_domain::quality::SeparationProfile;
        if *profile == SeparationProfile::compatibility() {
            Ok(Self::CompatibilityV1)
        } else if *profile == SeparationProfile::production() {
            Ok(Self::OriginalMixBassV2)
        } else {
            Err(DomainError::DspFailure(
                "unsupported separation quality profile".into(),
            ))
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SeparationMode {
    TwoStems,
    #[default]
    FiveStems,
}

pub struct BoundedSeparation<I, P, G, S> {
    inference: I,
    progress: P,
    gate: G,
    store: S,
    rhythm_policy: RhythmPolicy,
    mode: SeparationMode,
}
impl<I, P, G, S> BoundedSeparation<I, P, G, S>
where
    I: StemInferencePort + BlockStemInferencePort,
    P: SeparationProgressPort,
    G: SpectralBleedGatePort,
    S: AudioBlockStore,
{
    pub fn new(inference: I, progress: P, gate: G, store: S) -> Self {
        Self {
            inference,
            progress,
            gate,
            store,
            rhythm_policy: RhythmPolicy::CompatibilityV1,
            mode: SeparationMode::FiveStems,
        }
    }
    pub fn with_rhythm_policy(mut self, policy: RhythmPolicy) -> Self {
        self.rhythm_policy = policy;
        self
    }
    pub fn with_separation_mode(mut self, mode: SeparationMode) -> Self {
        self.mode = mode;
        self
    }
    fn stage(
        &self,
        tag: &str,
        signature: &str,
        parents: &[Source],
        cancel: &CancellationToken,
        produce: impl FnOnce(&mut dyn AudioBlockWriter) -> Result<(), DomainError>,
    ) -> Result<Source, DomainError> {
        check(cancel)?;
        if let Some(cached) = self.store.cached(tag, signature, parents, cancel)? {
            return aligned(&parents[0], cached);
        }
        let mut writer = self.store.create(tag, parents[0].metadata())?;
        produce(writer.as_mut())?;
        check(cancel)?;
        let output = aligned(&parents[0], writer.finish()?)?;
        self.store.remember(tag, signature, parents, &output)?;
        Ok(output)
    }
    fn silent_stem(
        &self,
        tag: &str,
        mix: &Source,
        cancel: &CancellationToken,
    ) -> Result<Source, DomainError> {
        let parents = [Arc::clone(mix)];
        self.stage(tag, "silent-v1", &parents, cancel, |writer| {
            let metadata = mix.metadata();
            let channels = usize::from(metadata.format.channels);
            let zeros = vec![0.0f32; BLOCK * channels];
            for start in (0..metadata.frame_count).step_by(BLOCK) {
                check(cancel)?;
                let count = (metadata.frame_count - start).min(BLOCK as u64) as usize;
                writer.write(&zeros[..count * channels])?;
            }
            Ok(())
        })
    }
    fn infer(
        &self,
        tag: &str,
        stage: InferenceStage,
        stage_index: u8,
        total_stages: u8,
        stage_label: &str,
        source: Source,
        cancel: &CancellationToken,
    ) -> Result<Source, DomainError> {
        self.stage(
            tag,
            &self.inference.stage_identity(stage),
            &[Arc::clone(&source)],
            cancel,
            |writer| {
                self.inference.infer_blocks_with_progress(
                    stage,
                    source.as_ref(),
                    writer,
                    cancel,
                    &|done_w, total_w| {
                        let frac = done_w as f32 / total_w.max(1) as f32;
                        self.progress.report_fraction(
                            stage_index,
                            total_stages,
                            frac,
                            &format!("{stage_label} ({done_w}/{total_w})"),
                        );
                    },
                )
            },
        )
    }
    fn combine(
        &self,
        tag: &str,
        sources: &[Source],
        signature: &str,
        cancel: &CancellationToken,
        operation: impl Fn(&[f32]) -> f32,
    ) -> Result<Source, DomainError> {
        self.stage(tag, signature, sources, cancel, |writer| {
            let channels = usize::from(sources[0].metadata().format.channels);
            let frames = sources[0].metadata().frame_count;
            let mut buffers: Vec<Vec<f32>> = sources
                .iter()
                .map(|_| vec![0.0; BLOCK * channels])
                .collect();
            let mut output = vec![0.0; BLOCK * channels];
            let mut values = vec![0.0; sources.len()];
            for start in (0..frames).step_by(BLOCK) {
                check(cancel)?;
                let count = (frames - start).min(BLOCK as u64) as usize;
                let sample_count = count * channels;
                for (source, buffer) in sources.iter().zip(&mut buffers) {
                    if source.metadata() != sources[0].metadata()
                        || source.read_frames(start, &mut buffer[..sample_count])? != count
                    {
                        return Err(DomainError::AudioAlignmentMismatch);
                    }
                }
                if sources.len() == 2 && signature == "subtract-v1" {
                    let (b0, b1) = (&buffers[0][..sample_count], &buffers[1][..sample_count]);
                    for ((target, &a), &b) in output[..sample_count].iter_mut().zip(b0).zip(b1) {
                        *target = a - b;
                    }
                } else if sources.len() == 3 && signature == "subtract-sum-v1" {
                    let (b0, b1, b2) = (
                        &buffers[0][..sample_count],
                        &buffers[1][..sample_count],
                        &buffers[2][..sample_count],
                    );
                    for (((target, &a), &b), &c) in
                        output[..sample_count].iter_mut().zip(b0).zip(b1).zip(b2)
                    {
                        *target = a - (b + c);
                    }
                } else {
                    for (index, target) in output[..sample_count].iter_mut().enumerate() {
                        for (value, buffer) in values.iter_mut().zip(&buffers) {
                            *value = buffer[index];
                        }
                        *target = operation(&values);
                    }
                }
                writer.write(&output[..sample_count])?;
            }
            Ok(())
        })
    }
    fn vocals(
        &self,
        bus: Source,
        cancel: &CancellationToken,
    ) -> Result<(Source, Source, Source), DomainError> {
        let signature = format!(
            "{};native-v1;vad8/-65;gate2048/.05/.25",
            self.inference.stage_identity(InferenceStage::LeadVocal)
        );
        let parents = [Arc::clone(&bus)];
        let lead = self.store.cached("lead", &signature, &parents, cancel)?;
        let backing = self.store.cached("backing", &signature, &parents, cancel)?;
        let active = self
            .store
            .cached("active-vocals", &signature, &parents, cancel)?;
        if let (Some(lead), Some(backing), Some(active)) = (lead, backing, active) {
            return Ok((
                aligned(&bus, lead)?,
                aligned(&bus, backing)?,
                aligned(&bus, active)?,
            ));
        }
        let metadata = bus.metadata();
        let chunk = if cfg!(target_os = "android") {
            256_000usize
        } else {
            metadata.format.sample_rate_hz as usize * 8
        };
        let total_chunks = metadata.frame_count.div_ceil(chunk as u64) as usize;
        let channels = usize::from(metadata.format.channels);
        let mut lead = self.store.create("lead", metadata)?;
        let mut backing = self.store.create("backing", metadata)?;
        let mut active = self.store.create("active-vocals", metadata)?;
        for (chunk_idx, start) in (0..metadata.frame_count).step_by(chunk).enumerate() {
            check(cancel)?;
            let frames = (metadata.frame_count - start).min(chunk as u64) as usize;
            let mut samples = vec![0.0; frames * channels];
            if bus.read_frames(start, &mut samples)? != frames {
                return Err(DomainError::AudioAlignmentMismatch);
            }
            let rms = (samples
                .iter()
                .map(|s| f64::from(*s) * f64::from(*s))
                .sum::<f64>()
                / samples.len() as f64)
                .sqrt();
            let dbfs = if rms <= f64::MIN_POSITIVE {
                f64::NEG_INFINITY
            } else {
                20.0 * rms.log10()
            };
            if dbfs < -65.0 {
                samples.fill(0.0);
                lead.write(&samples)?;
                backing.write(&samples)?;
                active.write(&samples)?;
            } else {
                active.write(&samples)?;
                let input = AudioBufferF32::new(metadata.format, samples)?;
                let main = self.inference.infer_lead_vocal(&input, cancel)?;
                if main.format() != input.format() || main.frames() != input.frames() {
                    return Err(DomainError::AudioAlignmentMismatch);
                }
                let raw = AudioBufferF32::new(
                    metadata.format,
                    input
                        .samples()
                        .iter()
                        .zip(main.samples())
                        .map(|(a, b)| a - b)
                        .collect(),
                )?;
                let (main, chorus) = self.gate.rebalance(&main, &raw)?;
                if main.format() != input.format()
                    || chorus.format() != input.format()
                    || main.frames() != frames
                    || chorus.frames() != frames
                {
                    return Err(DomainError::AudioAlignmentMismatch);
                }
                lead.write(main.samples())?;
                backing.write(chorus.samples())?;
            }
            self.progress.report_fraction(
                1,
                5,
                (chunk_idx + 1) as f32 / total_chunks.max(1) as f32,
                &format!("Separating vocal stems ({}/{total_chunks})", chunk_idx + 1),
            );
        }
        let lead = lead.finish()?;
        let backing = backing.finish()?;
        let active = active.finish()?;
        check(cancel)?;
        for (tag, output) in [
            ("lead", &lead),
            ("backing", &backing),
            ("active-vocals", &active),
        ] {
            self.store.remember(tag, &signature, &parents, output)?;
        }
        Ok((lead, backing, active))
    }
    pub fn execute(
        &self,
        mix: Source,
        cancel: &CancellationToken,
    ) -> Result<BTreeMap<StemType, Source>, DomainError> {
        check(cancel)?;
        if mix.metadata().frame_count == 0 {
            return Err(DomainError::AudioDecodeFailure("audio is empty".into()));
        }
        if self.mode == SeparationMode::TwoStems {
            self.progress.report(0, 1, "Isolating vocal bus");
            let vocal_bus = self.infer(
                "vocal-bus",
                InferenceStage::VocalBus,
                0,
                1,
                "Isolating vocal bus",
                Arc::clone(&mix),
                cancel,
            )?;
            let instrumental = self.combine(
                "instrumental",
                &[Arc::clone(&mix), Arc::clone(&vocal_bus)],
                "subtract-v1",
                cancel,
                |v| v[0] - v[1],
            )?;
            let metadata = mix.metadata();
            let silent = self.silent_stem("silent-stem", &mix, cancel)?;
            let outputs = BTreeMap::from([
                (StemType::LeadVocal, vocal_bus),
                (StemType::BackingVocals, Arc::clone(&silent)),
                (StemType::Drums, Arc::clone(&silent)),
                (StemType::Bass, Arc::clone(&silent)),
                (StemType::Instrumental, instrumental),
            ]);
            self.progress
                .report(1, 1, "Validating reconstruction in bounded blocks");
            let channels = usize::from(metadata.format.channels);
            for start in (0..metadata.frame_count).step_by(BLOCK) {
                check(cancel)?;
                let count = (metadata.frame_count - start).min(BLOCK as u64) as usize;
                let mut original = vec![0.0; count * channels];
                if mix.read_frames(start, &mut original)? != count {
                    return Err(DomainError::AudioAlignmentMismatch);
                }
                let original = AudioBufferF32::new(metadata.format, original)?;
                let mut stems = Vec::new();
                for kind in StemType::ALL {
                    let source = &outputs[&kind];
                    let mut samples = vec![0.0; count * channels];
                    if source.metadata() != metadata
                        || source.read_frames(start, &mut samples)? != count
                    {
                        return Err(DomainError::AudioAlignmentMismatch);
                    }
                    stems.push(Stem {
                        kind,
                        audio: AudioBufferF32::new(metadata.format, samples)?,
                    });
                }
                SeparationResult::try_new(&original, stems)?;
            }
            self.progress.report(1, 1, "Two aligned stems validated");
            return Ok(outputs);
        }
        self.progress.report(0, 5, "Isolating vocal bus");
        let vocal_bus = self.infer(
            "vocal-bus",
            InferenceStage::VocalBus,
            0,
            5,
            "Isolating vocal bus",
            Arc::clone(&mix),
            cancel,
        )?;
        self.progress.report(1, 5, "Separating vocal stems");
        let (lead, backing, active) = self.vocals(vocal_bus, cancel)?;
        let master = self.combine(
            "instrumental-master",
            &[Arc::clone(&mix), active],
            "subtract-v1",
            cancel,
            |v| v[0] - v[1],
        )?;
        self.progress.report(2, 5, "Separating drums");
        let drums = self.infer(
            "drums",
            InferenceStage::Drums,
            2,
            5,
            "Separating drums",
            Arc::clone(&master),
            cancel,
        )?;
        let bass_input = match self.rhythm_policy {
            RhythmPolicy::CompatibilityV1 => self.combine(
                "drum-free",
                &[Arc::clone(&master), Arc::clone(&drums)],
                "subtract-v1",
                cancel,
                |v| v[0] - v[1],
            )?,
            RhythmPolicy::OriginalMixBassV2 => Arc::clone(&mix),
        };
        let bass_label = match self.rhythm_policy {
            RhythmPolicy::CompatibilityV1 => "Separating bass from drum-free residual",
            RhythmPolicy::OriginalMixBassV2 => "Separating bass from original mix",
        };
        self.progress.report(3, 5, bass_label);
        let bass = self.infer(
            "bass",
            InferenceStage::Bass,
            3,
            5,
            bass_label,
            bass_input,
            cancel,
        )?;
        let instrumental = self.combine(
            "instrumental",
            &[master, Arc::clone(&drums), Arc::clone(&bass)],
            "subtract-sum-v1",
            cancel,
            |v| v[0] - (v[1] + v[2]),
        )?;
        let outputs = BTreeMap::from([
            (StemType::LeadVocal, lead),
            (StemType::BackingVocals, backing),
            (StemType::Drums, drums),
            (StemType::Bass, bass),
            (StemType::Instrumental, instrumental),
        ]);
        self.progress
            .report(4, 5, "Validating reconstruction in bounded blocks");
        let metadata = mix.metadata();
        let channels = usize::from(metadata.format.channels);
        for start in (0..metadata.frame_count).step_by(BLOCK) {
            check(cancel)?;
            let count = (metadata.frame_count - start).min(BLOCK as u64) as usize;
            let mut original = vec![0.0; count * channels];
            if mix.read_frames(start, &mut original)? != count {
                return Err(DomainError::AudioAlignmentMismatch);
            }
            let original = AudioBufferF32::new(metadata.format, original)?;
            let mut stems = Vec::new();
            for kind in StemType::ALL {
                let source = &outputs[&kind];
                let mut samples = vec![0.0; count * channels];
                if source.metadata() != metadata
                    || source.read_frames(start, &mut samples)? != count
                {
                    return Err(DomainError::AudioAlignmentMismatch);
                }
                stems.push(Stem {
                    kind,
                    audio: AudioBufferF32::new(metadata.format, samples)?,
                });
            }
            SeparationResult::try_new(&original, stems)?;
        }
        self.progress.report(5, 5, "Five aligned stems validated");
        Ok(outputs)
    }
}
fn aligned(reference: &Source, candidate: Source) -> Result<Source, DomainError> {
    if reference.metadata() != candidate.metadata() {
        Err(DomainError::AudioAlignmentMismatch)
    } else {
        Ok(candidate)
    }
}
fn check(cancel: &CancellationToken) -> Result<(), DomainError> {
    if cancel.is_cancelled() {
        Err(DomainError::SeparationCancelled)
    } else {
        Ok(())
    }
}
