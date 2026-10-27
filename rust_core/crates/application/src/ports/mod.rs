use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
};

use atomusicator_domain::{
    AudioAssetMetadata, AudioBufferF32, AudioFormat, AudioSession, DomainError, Stem, StemType,
};

use crate::CancellationToken;

pub trait AudioDecoderPort: Send + Sync {
    fn probe_metadata(&self, path: &Path) -> Result<AudioAssetMetadata, DomainError>;
    fn decode_to_f32(&self, path: &Path) -> Result<AudioBufferF32, DomainError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct RhythmStems {
    pub drums: AudioBufferF32,
    pub bass: AudioBufferF32,
}

pub trait StemInferencePort: Send + Sync {
    fn infer_vocal_bus(
        &self,
        mix: &AudioBufferF32,
        cancel: &CancellationToken,
    ) -> Result<AudioBufferF32, DomainError>;
    fn infer_lead_vocal(
        &self,
        vocal_bus: &AudioBufferF32,
        cancel: &CancellationToken,
    ) -> Result<AudioBufferF32, DomainError>;
    fn infer_rhythm_section(
        &self,
        instrumental: &AudioBufferF32,
        cancel: &CancellationToken,
    ) -> Result<RhythmStems, DomainError>;
}

pub trait SpectralBleedGatePort: Send + Sync {
    fn rebalance(
        &self,
        lead: &AudioBufferF32,
        backing: &AudioBufferF32,
    ) -> Result<(AudioBufferF32, AudioBufferF32), DomainError>;
}

/// DSP boundary for exact phase subtraction and optional vocal bleed rebalance.
pub trait PhaseDspPort: Send + Sync {
    fn subtract_phase(
        &self,
        minuend: &AudioBufferF32,
        subtrahend: &AudioBufferF32,
    ) -> Result<AudioBufferF32, DomainError>;

    fn apply_spectral_bleed_gate(
        &self,
        lead_vocal: &AudioBufferF32,
        raw_backing_vocals: &AudioBufferF32,
    ) -> Result<(AudioBufferF32, AudioBufferF32), DomainError>;
}

pub trait ArtifactCachePort: Send + Sync {
    fn get_checkpoint(&self, key: &str) -> Result<Option<AudioBufferF32>, DomainError>;
    fn put_checkpoint(&self, key: &str, buffer: &AudioBufferF32) -> Result<(), DomainError>;
}

pub trait RealtimePlaybackPort: Send + Sync {
    fn load_session(&self, stems: Vec<Stem>) -> Result<(), DomainError>;
    fn set_track_state(&self, stem_id: StemType, gain_db: f32, pan: f32, mute: bool, solo: bool);
    fn set_pitch_semitones(&self, semitones: f32, preserve_formants: bool);
}

pub trait SessionRepositoryPort: Send + Sync {
    fn load(&self, id: &str) -> Result<Option<AudioSession>, DomainError>;
    fn save(&self, session: &AudioSession) -> Result<(), DomainError>;
}

/// Persistence boundary for publishing a validated separation result.
pub trait SessionWorkspacePort: Send + Sync {
    fn publish_separation_result(
        &self,
        asset: &atomusicator_domain::AudioAsset,
        result: &atomusicator_domain::SeparationResult,
    ) -> Result<(), DomainError>;
}

/// Verified metadata for a manifest profile available to the inference adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedModelSpec {
    pub profile_id: String,
    pub stage: String,
    pub local_path: PathBuf,
    pub sha256_hash: String,
    pub license: String,
    pub attribution: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelRegistryError {
    ProfileNotFound(String),
    Unavailable(String),
}

impl std::fmt::Display for ModelRegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProfileNotFound(profile_id) => {
                write!(formatter, "model profile `{profile_id}` was not found")
            }
            Self::Unavailable(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ModelRegistryError {}

pub type ModelRegistryFuture<'a> =
    Pin<Box<dyn Future<Output = Result<VerifiedModelSpec, ModelRegistryError>> + Send + 'a>>;

/// Application-facing boundary for locating or downloading a verified model profile.
pub trait ModelRegistryPort: Send + Sync {
    fn ensure_profile_ready<'a>(&'a self, profile_id: &'a str) -> ModelRegistryFuture<'a>;
}

/// Fully identifies the inputs that make a DSP/inference checkpoint reusable.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StageCacheKey {
    pub stage_tag: String,
    pub input_audio_sha256: String,
    pub model_sha256: String,
    pub stft_n_fft: usize,
    pub stft_hop_length: usize,
    pub runtime_signature: String,
}

impl StageCacheKey {
    pub fn content_key(&self) -> Result<String, DomainError> {
        for (label, digest) in [
            ("input audio", self.input_audio_sha256.as_str()),
            ("model", self.model_sha256.as_str()),
        ] {
            if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(DomainError::CacheFailure(format!(
                    "{label} hash must be a SHA-256 digest"
                )));
            }
        }
        if self.stage_tag.trim().is_empty()
            || self.runtime_signature.trim().is_empty()
            || self.stft_n_fft == 0
            || self.stft_hop_length == 0
            || self.stft_hop_length > self.stft_n_fft
        {
            return Err(DomainError::CacheFailure(
                "stage cache key has invalid stage, runtime, or STFT parameters".into(),
            ));
        }
        let input_hash = self.input_audio_sha256.to_ascii_lowercase();
        let model_hash = self.model_sha256.to_ascii_lowercase();
        let mut key = String::new();
        for component in [
            self.stage_tag.as_str(),
            input_hash.as_str(),
            model_hash.as_str(),
        ] {
            key.push_str(&format!("{}:{}", component.len(), component));
        }
        key.push_str(&format!(
            "{}:{}:{}:{}",
            self.stft_n_fft,
            self.stft_hop_length,
            self.runtime_signature.len(),
            self.runtime_signature
        ));
        Ok(key)
    }
}

/// Typed cache boundary; cache writes are advisory and never interrupt audio processing.
pub trait StageArtifactCachePort: Send + Sync {
    fn get_verified_checkpoint(
        &self,
        key: &StageCacheKey,
    ) -> Result<Option<AudioBufferF32>, DomainError>;

    fn store_checkpoint_non_fatal(&self, key: &StageCacheKey, buffer: &AudioBufferF32);
}

/// Reports user-facing progress without coupling a use case to Flutter streams.
pub trait ProgressReporterPort: Send + Sync {
    fn on_progress(&self, stage_name: &str, progress_ratio: f32);
}

pub trait SeparationStage: Send + Sync {
    fn name(&self) -> &'static str;
    fn execute(
        &self,
        input: &AudioBufferF32,
        cancel: &CancellationToken,
    ) -> Result<Vec<Stem>, DomainError>;
}

pub trait SeparationProgressPort: Send + Sync {
    fn report(&self, completed_stages: u8, total_stages: u8, message: &str);
}

pub trait WaveformCachePort: Send + Sync {
    fn peaks(&self, audio: &AudioBufferF32, bucket_frames: usize) -> Vec<(f32, f32)>;
}

pub trait ExportAudioPort: Send + Sync {
    fn write_wav_atomic(
        &self,
        path: &Path,
        audio: &AudioBufferF32,
        bits_per_sample: u16,
    ) -> Result<(), DomainError>;
}

pub trait AudioFormatSelectionPort: Send + Sync {
    fn preferred_format(&self) -> Result<AudioFormat, DomainError>;
}
