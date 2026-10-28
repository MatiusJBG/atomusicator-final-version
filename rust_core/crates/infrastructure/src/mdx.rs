//! Native MDX adapter for the four SHA-256 pinned legacy profiles.
//! Tensor layout is [batch, L-real/L-imag/R-real/R-imag, frequency, time].
use crate::{
    cache::ContentAddressedCache,
    dsp::PhaseSubtractor,
    onnx::{OnnxSessionRunner, OrtSessionPool},
};
use atomusicator_application::{
    CancellationToken,
    ports::{RhythmStems, StageArtifactCachePort, StageCacheKey, StemInferencePort},
};
use atomusicator_domain::{AudioBufferF32, DomainError};
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex, num_complex::Complex32};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

#[derive(Debug, Clone, Copy)]
pub struct MdxConfig {
    pub n_fft: usize,
    pub hop: usize,
    pub dim_f: usize,
    pub dim_t: usize,
    pub compensate: f32,
    pub invert: bool,
}
impl MdxConfig {
    /// Parameters from the legacy cache's mdx_model_data.json, matched to manifest hashes.
    pub fn for_profile(id: &str) -> Result<Self, DomainError> {
        let model = atomusicator_domain::quality::SeparationProfile::compatibility()
            .models
            .into_iter()
            .find(|model| model.profile_id == id)
            .ok_or_else(|| fail("no reviewed MDX configuration for profile"))?;
        Ok(Self {
            n_fft: model.n_fft,
            hop: model.hop,
            dim_f: model.dim_f,
            dim_t: model.dim_t,
            compensate: model.compensate,
            invert: model.invert,
        })
    }
    pub fn chunk_frames(self) -> usize {
        self.hop * (self.dim_t - 1)
    }
}

pub struct MdxTransform {
    config: MdxConfig,
    window: Vec<f32>,
    forward_fft: Arc<dyn RealToComplex<f32>>,
    inverse_fft: Arc<dyn ComplexToReal<f32>>,
}
impl MdxTransform {
    pub fn new(config: MdxConfig) -> Result<Self, DomainError> {
        if config.n_fft < 4
            || !config.n_fft.is_multiple_of(2)
            || config.hop == 0
            || config.hop > config.n_fft / 2
            || config.dim_f == 0
            || config.dim_f > config.n_fft / 2 + 1
            || config.dim_t < 2
            || config.chunk_frames() <= config.n_fft
            || !config.compensate.is_finite()
            || config.compensate <= 0.0
        {
            return Err(fail("invalid MDX dimensions"));
        }
        let mut planner = RealFftPlanner::new();
        Ok(Self {
            config,
            window: (0..config.n_fft)
                .map(|i| {
                    0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / config.n_fft as f32).cos()
                })
                .collect(),
            forward_fft: planner.plan_fft_forward(config.n_fft),
            inverse_fft: planner.plan_fft_inverse(config.n_fft),
        })
    }
    pub fn forward(&self, stereo: &[f32]) -> Result<Vec<f32>, DomainError> {
        let c = self.config;
        if stereo.len() != c.chunk_frames() * 2 || stereo.iter().any(|v| !v.is_finite()) {
            return Err(fail("invalid MDX audio chunk"));
        }
        let mut out = vec![0.0; 4 * c.dim_f * c.dim_t];
        let mut time = self.forward_fft.make_input_vec();
        let mut freq = self.forward_fft.make_output_vec();
        let mut scratch = self.forward_fft.make_scratch_vec();
        for channel in 0..2 {
            for frame in 0..c.dim_t {
                let start = (frame * c.hop) as isize - (c.n_fft / 2) as isize;
                for (i, value) in time.iter_mut().enumerate() {
                    let mut source = start + i as isize;
                    let len = c.chunk_frames() as isize;
                    // torch.stft(center=True, pad_mode='reflect').
                    if source < 0 {
                        source = -source;
                    }
                    if source >= len {
                        source = 2 * len - source - 2;
                    }
                    *value = stereo[source as usize * 2 + channel] * self.window[i];
                }
                self.forward_fft
                    .process_with_scratch(&mut time, &mut freq, &mut scratch)
                    .map_err(|e| fail(&e.to_string()))?;
                for (bin, value) in freq.iter().enumerate().take(c.dim_f) {
                    out[((channel * 2) * c.dim_f + bin) * c.dim_t + frame] = value.re;
                    out[((channel * 2 + 1) * c.dim_f + bin) * c.dim_t + frame] = value.im;
                }
            }
        }
        Ok(out)
    }
    pub fn inverse(&self, spectrum: &[f32]) -> Result<Vec<f32>, DomainError> {
        let c = self.config;
        if spectrum.len() != 4 * c.dim_f * c.dim_t || spectrum.iter().any(|v| !v.is_finite()) {
            return Err(fail("invalid MDX output tensor"));
        }
        let mut out = vec![0.0; c.chunk_frames() * 2];
        let mut weights = vec![0.0; c.chunk_frames()];
        let mut freq = self.inverse_fft.make_input_vec();
        let mut time = self.inverse_fft.make_output_vec();
        let mut scratch = self.inverse_fft.make_scratch_vec();
        for channel in 0..2 {
            for frame in 0..c.dim_t {
                freq.fill(Complex32::new(0.0, 0.0));
                for (bin, value) in freq.iter_mut().enumerate().take(c.dim_f) {
                    *value = Complex32::new(
                        spectrum[((channel * 2) * c.dim_f + bin) * c.dim_t + frame],
                        spectrum[((channel * 2 + 1) * c.dim_f + bin) * c.dim_t + frame],
                    );
                }
                freq[0].im = 0.0;
                freq[c.n_fft / 2].im = 0.0;
                self.inverse_fft
                    .process_with_scratch(&mut freq, &mut time, &mut scratch)
                    .map_err(|e| fail(&e.to_string()))?;
                let start = (frame * c.hop) as isize - (c.n_fft / 2) as isize;
                for (i, sample) in time.iter().enumerate() {
                    let target = start + i as isize;
                    if target >= 0 && (target as usize) < weights.len() {
                        let target = target as usize;
                        out[target * 2 + channel] += sample * self.window[i] / c.n_fft as f32;
                        if channel == 0 {
                            weights[target] += self.window[i] * self.window[i];
                        }
                    }
                }
            }
        }
        for (frame, weight) in weights.into_iter().enumerate() {
            if weight > f32::EPSILON {
                out[frame * 2] /= weight;
                out[frame * 2 + 1] /= weight;
            }
        }
        Ok(out)
    }
}

pub struct MdxInference {
    runner: OnnxSessionRunner,
    models: BTreeMap<String, (PathBuf, String)>,
    cache: Option<ContentAddressedCache>,
    runtime_signature: String,
}
impl MdxInference {
    pub fn new(
        models: BTreeMap<String, (PathBuf, String)>,
        cache: Option<ContentAddressedCache>,
    ) -> Result<Self, DomainError> {
        Self::with_session_pool(
            models,
            cache,
            Arc::new(OrtSessionPool::new(2).map_err(|e| fail(&e.to_string()))?),
        )
    }
    pub fn with_session_pool(
        models: BTreeMap<String, (PathBuf, String)>,
        cache: Option<ContentAddressedCache>,
        pool: Arc<OrtSessionPool>,
    ) -> Result<Self, DomainError> {
        let runtime_signature = format!(
            "{}-{:?}-intra{}-inter1-sequential-fp32",
            ort::info(),
            pool.provider_order(),
            pool.intra_threads()
        );
        Ok(Self {
            runner: OnnxSessionRunner::new(pool),
            models,
            cache,
            runtime_signature,
        })
    }
    fn infer(
        &self,
        id: &str,
        audio: &AudioBufferF32,
        cancel: &CancellationToken,
    ) -> Result<AudioBufferF32, DomainError> {
        if cancel.is_cancelled() {
            return Err(DomainError::SeparationCancelled);
        }
        if audio.format().sample_rate_hz != 44100
            || audio.format().channels != 2
            || audio.frames() == 0
        {
            return Err(fail("MDX requires nonempty 44100 Hz stereo audio"));
        }
        let (path, hash) = self
            .models
            .get(id)
            .ok_or_else(|| fail("verified MDX model is missing"))?;
        let c = MdxConfig::for_profile(id)?;
        let mut digest = Sha256::new();
        for value in audio.samples() {
            digest.update(value.to_le_bytes());
        }
        let key = StageCacheKey {
            stage_tag: id.into(),
            input_audio_sha256: format!("{:x}", digest.finalize()),
            model_sha256: hash.clone(),
            stft_n_fft: c.n_fft,
            stft_hop_length: c.hop,
            runtime_signature: format!(
                "native-mdx-v2-overlap035-dimt{}-comp{}-inverse{}-{}",
                c.dim_t, c.compensate, c.invert, self.runtime_signature
            ),
        };
        if let Some(cache) = &self.cache
            && let Ok(Some(cached)) = cache.get_verified_checkpoint(&key)
            && cached.format() == audio.format()
            && cached.frames() == audio.frames()
        {
            return Ok(cached);
        }
        let transform = MdxTransform::new(c)?;
        let chunk = c.chunk_frames();
        let trim = c.n_fft / 2;
        let generation = chunk - 2 * trim;
        let pad = generation + trim - audio.frames() % generation;
        let padded_frames = trim + audio.frames() + pad;
        let step = (0.65 * chunk as f64) as usize;
        let mut out = vec![0.0f32; padded_frames * 2];
        let mut weights = vec![0.0f32; padded_frames];
        // Normalize the input to unit peak and restore its amplitude afterwards.
        let peak = audio
            .samples()
            .iter()
            .map(|v| v.abs())
            .fold(0.0f32, f32::max);
        if peak == 0.0 {
            return Ok(audio.clone());
        }
        for start in (0..padded_frames).step_by(step) {
            if cancel.is_cancelled() {
                return Err(DomainError::SeparationCancelled);
            }
            let actual = chunk.min(padded_frames - start);
            let mut input = vec![0.0; chunk * 2];
            for frame in 0..actual {
                let source = start + frame;
                if source >= trim && source - trim < audio.frames() {
                    input[frame * 2] = audio.samples()[(source - trim) * 2] / peak;
                    input[frame * 2 + 1] = audio.samples()[(source - trim) * 2 + 1] / peak;
                }
            }
            let mut spectrum = transform.forward(&input)?;
            for component in 0..4 {
                spectrum[component * c.dim_f * c.dim_t..(component * c.dim_f + 3) * c.dim_t]
                    .fill(0.0);
            }
            let prediction = self
                .runner
                .run_f32(
                    path,
                    "input",
                    &[1, 4, c.dim_f as i64, c.dim_t as i64],
                    &spectrum,
                )
                .map_err(|e| fail(&e.to_string()))?;
            let decoded = transform.inverse(&prediction)?;
            for frame in 0..actual {
                let weight = if actual > 1 {
                    0.5 - 0.5
                        * (2.0 * std::f32::consts::PI * frame as f32 / (actual - 1) as f32).cos()
                } else {
                    1.0
                };
                weights[start + frame] += weight;
                for channel in 0..2 {
                    out[(start + frame) * 2 + channel] += decoded[frame * 2 + channel] * weight;
                }
            }
        }
        if cancel.is_cancelled() {
            return Err(DomainError::SeparationCancelled);
        }
        let mut result = vec![0.0; audio.samples().len()];
        for frame in 0..audio.frames() {
            let position = trim + frame;
            if weights[position] <= 0.0 {
                return Err(fail("MDX overlap left an uncovered frame"));
            }
            for channel in 0..2 {
                let value = out[position * 2 + channel] / weights[position] * peak * c.compensate;
                result[frame * 2 + channel] = if c.invert {
                    audio.samples()[frame * 2 + channel] - value
                } else {
                    value
                };
            }
        }
        let result = AudioBufferF32::new(audio.format(), result)?;
        if let Some(cache) = &self.cache {
            cache.store_checkpoint_non_fatal(&key, &result);
        }
        Ok(result)
    }
}
impl StemInferencePort for MdxInference {
    fn infer_vocal_bus(
        &self,
        audio: &AudioBufferF32,
        cancel: &CancellationToken,
    ) -> Result<AudioBufferF32, DomainError> {
        self.infer("uvr-mdxnet-main-v1", audio, cancel)
    }
    fn infer_lead_vocal(
        &self,
        audio: &AudioBufferF32,
        cancel: &CancellationToken,
    ) -> Result<AudioBufferF32, DomainError> {
        self.infer("uvr-mdxnet-kara-2-v1", audio, cancel)
    }
    fn infer_rhythm_section(
        &self,
        audio: &AudioBufferF32,
        cancel: &CancellationToken,
    ) -> Result<RhythmStems, DomainError> {
        let drums = self.infer("kuielab-mdxnet-drums-v1", audio, cancel)?;
        // Legacy stage four consumes the drum-free residual, not the original accompaniment.
        let residual = PhaseSubtractor::subtract(audio, &drums)?;
        let bass = self.infer("kuielab-mdxnet-bass-v1", &residual, cancel)?;
        Ok(RhythmStems { drums, bass })
    }
}
impl atomusicator_application::ports::BlockStemInferencePort for MdxInference {
    fn stage_identity(&self, stage: atomusicator_application::ports::InferenceStage) -> String {
        let id = block_profile(stage);
        let hash = self
            .models
            .get(id)
            .map(|(_, hash)| hash.as_str())
            .unwrap_or("missing");
        format!(
            "bounded-mdx-v1/{id}/{hash}/{:?}/{}",
            MdxConfig::for_profile(id),
            self.runtime_signature
        )
    }
    fn infer_blocks(
        &self,
        stage: atomusicator_application::ports::InferenceStage,
        source: &dyn atomusicator_application::ports::AudioBlockSource,
        target: &mut dyn atomusicator_application::ports::AudioBlockWriter,
        cancel: &CancellationToken,
    ) -> Result<(), DomainError> {
        let id = block_profile(stage);
        let (path, _) = self
            .models
            .get(id)
            .ok_or_else(|| fail("verified MDX model is missing"))?;
        let c = MdxConfig::for_profile(id)?;
        crate::mdx_bounded::run_bounded_mdx(c, source, target, cancel, |spectrum| {
            self.runner
                .run_f32(
                    path,
                    "input",
                    &[1, 4, c.dim_f as i64, c.dim_t as i64],
                    spectrum,
                )
                .map_err(|e| fail(&e.to_string()))
        })
    }
}
fn block_profile(stage: atomusicator_application::ports::InferenceStage) -> &'static str {
    use atomusicator_application::ports::InferenceStage;
    match stage {
        InferenceStage::VocalBus => "uvr-mdxnet-main-v1",
        InferenceStage::LeadVocal => "uvr-mdxnet-kara-2-v1",
        InferenceStage::Drums => "kuielab-mdxnet-drums-v1",
        InferenceStage::Bass => "kuielab-mdxnet-bass-v1",
    }
}
fn fail(message: &str) -> DomainError {
    DomainError::DspFailure(message.into())
}
