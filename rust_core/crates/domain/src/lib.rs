//! Pure audio and session domain types. This crate intentionally has no
//! third-party dependencies so application policy cannot inherit I/O details.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum DomainError {
    EmptyIdentifier,
    InvalidAudioFormat,
    IncompleteInterleavedFrame,
    NonFiniteSample,
    MissingStem(StemType),
    DuplicateStem(StemType),
    UnexpectedStemCount(usize),
    StemFormatMismatch(StemType),
    ReconstructionMismatch { frame: usize, deviation: f32 },
    SeparationCancelled,
    AudioAlignmentMismatch,
    CacheFailure(String),
    DspFailure(String),
    AudioDecodeFailure(String),
    AudioExportFailure(String),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyIdentifier => write!(f, "identifier must not be empty"),
            Self::InvalidAudioFormat => write!(f, "sample rate and channel count must be positive"),
            Self::IncompleteInterleavedFrame => {
                write!(f, "sample count must contain complete interleaved frames")
            }
            Self::NonFiniteSample => write!(f, "audio samples must be finite"),
            Self::MissingStem(stem) => write!(f, "required stem is missing: {stem:?}"),
            Self::DuplicateStem(stem) => write!(f, "stem was supplied more than once: {stem:?}"),
            Self::UnexpectedStemCount(count) => write!(f, "expected five stems, received {count}"),
            Self::StemFormatMismatch(stem) => {
                write!(f, "stem format does not match the original mix: {stem:?}")
            }
            Self::ReconstructionMismatch { frame, deviation } => write!(
                f,
                "stem reconstruction differs at sample {frame} by {deviation}"
            ),
            Self::SeparationCancelled => write!(f, "separation was cancelled"),
            Self::AudioAlignmentMismatch => write!(
                f,
                "audio buffers must have matching formats and frame counts"
            ),
            Self::CacheFailure(message) => write!(f, "checkpoint cache failed: {message}"),
            Self::DspFailure(message) => write!(f, "DSP operation failed: {message}"),
            Self::AudioDecodeFailure(message) => write!(f, "audio decode failed: {message}"),
            Self::AudioExportFailure(message) => write!(f, "audio export failed: {message}"),
        }
    }
}

impl std::error::Error for DomainError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioAsset {
    pub id: AudioAssetId,
    pub path: PathBuf,
    pub display_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormat {
    pub sample_rate_hz: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioAssetMetadata {
    pub format: AudioFormat,
    pub frame_count: u64,
}

impl AudioFormat {
    pub fn new(sample_rate_hz: u32, channels: u16) -> Result<Self, DomainError> {
        if sample_rate_hz == 0 || channels == 0 {
            return Err(DomainError::InvalidAudioFormat);
        }
        Ok(Self {
            sample_rate_hz,
            channels,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioBufferF32 {
    format: AudioFormat,
    samples: Vec<f32>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct Stem {
    pub kind: StemType,
    pub audio: AudioBufferF32,
}

#[derive(Debug, Clone, PartialEq)]
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
            if deviation > 1.0e-5 {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSession {
    pub id: String,
    pub name: String,
    pub source: AudioAsset,
    pub stem_assets: Vec<AudioAsset>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeparationJobStatus {
    Queued,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeparationJob {
    pub id: String,
    pub source: AudioAssetId,
    pub status: SeparationJobStatus,
}
