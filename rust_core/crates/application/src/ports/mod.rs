use std::path::Path;

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
