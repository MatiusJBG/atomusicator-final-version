//! Pure audio and session domain types with serialization support.

pub mod jobs;
pub mod quality;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum DomainError {
    #[error("invalid sample rate: {0} Hz; must be greater than zero")]
    InvalidSampleRate(u32),
    #[error("invalid channel count: {0}; only mono and stereo are supported")]
    InvalidChannelCount(u16),
    #[error("identifier must not be empty")]
    EmptyIdentifier,
    #[error("sample count is not divisible by channel count {channels} ({samples} samples)")]
    MisalignedChannelBuffer { samples: usize, channels: u16 },
    #[error("sample count must contain complete interleaved frames")]
    IncompleteInterleavedFrame,
    #[error(
        "audio buffers do not match: expected {expected_frames} frames at {expected_sample_rate} Hz/{expected_channels} channels, got {actual_frames} frames at {actual_sample_rate} Hz/{actual_channels} channels"
    )]
    BufferFormatMismatch {
        expected_frames: usize,
        expected_sample_rate: u32,
        expected_channels: u16,
        actual_frames: usize,
        actual_sample_rate: u32,
        actual_channels: u16,
    },
    #[error("audio samples must be finite")]
    NonFiniteSample,
    #[error("required stem is missing: {0:?}")]
    MissingStem(StemType),
    #[error("stem was supplied more than once: {0:?}")]
    DuplicateStem(StemType),
    #[error("expected five stems, received {0}")]
    UnexpectedStemCount(usize),
    #[error("stem format does not match the original mix: {0:?}")]
    StemFormatMismatch(StemType),
    #[error("stem reconstruction differs at sample {frame} by {deviation}")]
    ReconstructionMismatch { frame: usize, deviation: f32 },
    #[error("separation was cancelled")]
    SeparationCancelled,
    #[error("pitch shift of {0} semitones is outside the supported range [-12, +12]")]
    PitchOutOfBounds(f32),
    #[error("audio buffers must have matching formats and frame counts")]
    AudioAlignmentMismatch,
    #[error("checkpoint cache failed: {0}")]
    CacheFailure(String),
    #[error("session workspace failed: {0}")]
    WorkspaceFailure(String),
    #[error("DSP operation failed: {0}")]
    DspFailure(String),
    #[error("audio decode failed: {0}")]
    AudioDecodeFailure(String),
    #[error("audio export failed: {0}")]
    AudioExportFailure(String),
    #[error("insufficient {resource}: requested {requested}, available {available}")]
    InsufficientResources {
        resource: String,
        requested: u64,
        available: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum StemType {
    LeadVocal,
    BackingVocals,
    Drums,
    Bass,
    Instrumental,
}

impl StemType {
    pub const ALL: [Self; 5] = [
        Self::LeadVocal,
        Self::BackingVocals,
        Self::Drums,
        Self::Bass,
        Self::Instrumental,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AudioAssetId(String);

impl AudioAssetId {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DomainError::EmptyIdentifier);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioAsset {
    pub id: AudioAssetId,
    pub path: PathBuf,
    pub display_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AudioFormat {
    pub sample_rate_hz: u32,
    pub channels: u16,
}

#[derive(Deserialize)]
struct AudioFormatRepr {
    sample_rate_hz: u32,
    channels: u16,
}

impl<'de> Deserialize<'de> for AudioFormat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let repr = AudioFormatRepr::deserialize(deserializer)?;
        Self::new(repr.sample_rate_hz, repr.channels).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioAssetMetadata {
    pub format: AudioFormat,
    pub frame_count: u64,
}

impl AudioFormat {
    pub fn new(sample_rate_hz: u32, channels: u16) -> Result<Self, DomainError> {
        if sample_rate_hz == 0 {
            return Err(DomainError::InvalidSampleRate(sample_rate_hz));
        }
        if !(1..=2).contains(&channels) {
            return Err(DomainError::InvalidChannelCount(channels));
        }
        Ok(Self {
            sample_rate_hz,
            channels,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PitchShiftSpec {
    semitones: f32,
    preserve_formants: bool,
}

#[derive(Deserialize)]
struct PitchShiftSpecRepr {
    semitones: f32,
    preserve_formants: bool,
}

impl<'de> Deserialize<'de> for PitchShiftSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let repr = PitchShiftSpecRepr::deserialize(deserializer)?;
        Self::try_new(repr.semitones, repr.preserve_formants).map_err(serde::de::Error::custom)
    }
}

impl PitchShiftSpec {
    pub fn try_new(semitones: f32, preserve_formants: bool) -> Result<Self, DomainError> {
        if !(-12.0..=12.0).contains(&semitones) {
            return Err(DomainError::PitchOutOfBounds(semitones));
        }
        Ok(Self {
            semitones,
            preserve_formants,
        })
    }

    pub fn semitones(&self) -> f32 {
        self.semitones
    }

    pub fn preserve_formants(&self) -> bool {
        self.preserve_formants
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AudioBufferF32 {
    format: AudioFormat,
    samples: Vec<f32>,
}

#[derive(Deserialize)]
struct AudioBufferF32Repr {
    format: AudioFormat,
    samples: Vec<f32>,
}

impl<'de> Deserialize<'de> for AudioBufferF32 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let repr = AudioBufferF32Repr::deserialize(deserializer)?;
        Self::new(repr.format, repr.samples).map_err(serde::de::Error::custom)
    }
}

impl AudioBufferF32 {
    pub fn new(format: AudioFormat, samples: Vec<f32>) -> Result<Self, DomainError> {
        if !samples.len().is_multiple_of(usize::from(format.channels)) {
            return Err(DomainError::IncompleteInterleavedFrame);
        }
        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(DomainError::NonFiniteSample);
        }
        Ok(Self { format, samples })
    }

    pub fn format(&self) -> AudioFormat {
        self.format
    }
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.format.channels)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stem {
    pub kind: StemType,
    pub audio: AudioBufferF32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeparationResult {
    stems: BTreeMap<StemType, Stem>,
}

impl SeparationResult {
    /// Creates a result only when all five stems align and their sample sum
    /// reconstructs the original mix within the domain's null-test tolerance.
    pub fn try_new(
        original: &AudioBufferF32,
        stems: impl IntoIterator<Item = Stem>,
    ) -> Result<Self, DomainError> {
        let mut by_kind = BTreeMap::new();
        for stem in stems {
            if stem.audio.format() != original.format() || stem.audio.frames() != original.frames()
            {
                return Err(DomainError::StemFormatMismatch(stem.kind));
            }
            let kind = stem.kind;
            if by_kind.insert(kind, stem).is_some() {
                return Err(DomainError::DuplicateStem(kind));
            }
        }
        if by_kind.len() != StemType::ALL.len() {
            return Err(DomainError::UnexpectedStemCount(by_kind.len()));
        }
        for kind in StemType::ALL {
            if !by_kind.contains_key(&kind) {
                return Err(DomainError::MissingStem(kind));
            }
        }
        for (index, original_sample) in original.samples().iter().enumerate() {
            let reconstructed: f32 = StemType::ALL
                .iter()
                .map(|kind| by_kind[kind].audio.samples()[index])
                .sum();
            let deviation = (reconstructed - original_sample).abs();
            if deviation > quality::RECONSTRUCTION_TOLERANCE {
                return Err(DomainError::ReconstructionMismatch {
                    frame: index,
                    deviation,
                });
            }
        }
        Ok(Self { stems: by_kind })
    }

    pub fn stem(&self, kind: StemType) -> &Stem {
        &self.stems[&kind]
    }
    pub fn stems(&self) -> impl ExactSizeIterator<Item = &Stem> {
        self.stems.values()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioSession {
    pub id: String,
    pub name: String,
    pub source: AudioAsset,
    pub stem_assets: Vec<AudioAsset>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeparationJobStatus {
    Queued,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeparationJob {
    pub id: String,
    pub source: AudioAssetId,
    pub status: SeparationJobStatus,
}
