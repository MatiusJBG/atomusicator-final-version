use std::path::Path;

use atomusicator_domain::{AudioAssetMetadata, AudioBufferF32, AudioFormat, DomainError};
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};

/// Bounded random access for the WAV formats accepted by the session reader.
pub struct WavBlockReader {
    reader: WavReader<std::io::BufReader<std::fs::File>>,
    metadata: AudioAssetMetadata,
}
impl WavBlockReader {
    pub fn open(path: &Path) -> Result<Self, DomainError> {
        let reader = WavReader::open(path).map_err(decode_error)?;
        let spec = reader.spec();
        if !matches!(
            (spec.sample_format, spec.bits_per_sample),
            (SampleFormat::Float, 32) | (SampleFormat::Int, 16..=32)
        ) {
            return Err(decode_error("unsupported session WAV format"));
        }
        let metadata = AudioAssetMetadata {
            format: AudioFormat::new(spec.sample_rate, spec.channels)?,
            frame_count: u64::from(reader.duration()),
        };
        Ok(Self { reader, metadata })
    }
    pub fn metadata(&self) -> AudioAssetMetadata {
        self.metadata
    }
    pub fn read_frames(
        &mut self,
        start: u64,
        destination: &mut [f32],
    ) -> Result<usize, DomainError> {
        let channels = usize::from(self.metadata.format.channels);
        if !destination.len().is_multiple_of(channels) {
            return Err(DomainError::IncompleteInterleavedFrame);
        }
        destination.fill(0.0);
        if start >= self.metadata.frame_count {
            return Ok(0);
        }
        let count =
            (self.metadata.frame_count - start).min((destination.len() / channels) as u64) as usize;
        self.reader
            .seek(u32::try_from(start).map_err(decode_error)?)
            .map_err(decode_error)?;
        let spec = self.reader.spec();
        match spec.sample_format {
            SampleFormat::Float => {
                let mut samples = self.reader.samples::<f32>();
                for target in &mut destination[..count * channels] {
                    *target = samples
                        .next()
                        .ok_or_else(|| decode_error("truncated stem"))?
                        .map_err(decode_error)?;
                }
            }
            SampleFormat::Int => {
                let scale = (1u64 << (spec.bits_per_sample - 1)) as f32;
                let mut samples = self.reader.samples::<i32>();
                for target in &mut destination[..count * channels] {
                    *target = samples
                        .next()
                        .ok_or_else(|| decode_error("truncated stem"))?
                        .map_err(decode_error)? as f32
                        / scale;
                }
            }
        }
        if destination[..count * channels]
            .iter()
            .any(|v| !v.is_finite())
        {
            return Err(DomainError::NonFiniteSample);
        }
        Ok(count)
    }
}
fn decode_error(e: impl std::fmt::Display) -> DomainError {
    DomainError::AudioDecodeFailure(e.to_string())
}

/// Header-only inspection; duration checks must not allocate full PCM buffers.
pub fn wav_metadata(path: &Path) -> Result<AudioAssetMetadata, DomainError> {
    let reader =
        WavReader::open(path).map_err(|e| DomainError::AudioDecodeFailure(e.to_string()))?;
    let spec = reader.spec();
    Ok(AudioAssetMetadata {
        format: AudioFormat::new(spec.sample_rate, spec.channels)?,
        frame_count: u64::from(reader.duration()),
    })
}

/// Writes IEEE 754 float32 samples without clipping or quantization.
pub fn write_wav_f32(path: &Path, audio: &AudioBufferF32) -> Result<(), DomainError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|error| DomainError::AudioExportFailure(error.to_string()))?;
    let spec = WavSpec {
        channels: audio.format().channels,
        sample_rate: audio.format().sample_rate_hz,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = WavWriter::create(path, spec)
        .map_err(|error| DomainError::AudioExportFailure(error.to_string()))?;
    for sample in audio.samples() {
        writer
            .write_sample(*sample)
            .map_err(|error| DomainError::AudioExportFailure(error.to_string()))?;
    }
    writer
        .finalize()
        .map_err(|error| DomainError::AudioExportFailure(error.to_string()))?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| DomainError::AudioExportFailure(error.to_string()))
}

pub fn validate_block_wav_size(metadata: AudioAssetMetadata) -> Result<(), DomainError> {
    let bytes = metadata
        .frame_count
        .checked_mul(u64::from(metadata.format.channels) * 4)
        .ok_or_else(|| DomainError::AudioExportFailure("WAV size overflow".into()))?;
    if bytes > u64::from(u32::MAX) - 128 {
        return Err(DomainError::AudioExportFailure("Float WAV exceeds the RIFF limit; split this source into sessions shorter than 3 hours".into()));
    }
    Ok(())
}
pub fn write_block_wav(
    path: &Path,
    source: &dyn atomusicator_application::ports::AudioBlockSource,
    cancel: &atomusicator_application::CancellationToken,
) -> Result<(), DomainError> {
    write_block_wav_hashed(path, source, cancel).map(|_| ())
}

/// Hashes the definitive RIFF header and PCM bytes in write order, without
/// rereading the file or seeking back to patch a placeholder header.
pub fn write_block_wav_hashed(
    path: &Path,
    source: &dyn atomusicator_application::ports::AudioBlockSource,
    cancel: &atomusicator_application::CancellationToken,
) -> Result<String, DomainError> {
    use sha2::{Digest, Sha256};
    use std::io::{BufWriter, Write};
    let metadata = source.metadata();
    validate_block_wav_size(metadata)?;
    let channels = usize::from(metadata.format.channels);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| DomainError::AudioExportFailure(e.to_string()))?;
    }
    let failure = |e: std::io::Error| DomainError::AudioExportFailure(e.to_string());
    let block_align = metadata
        .format
        .channels
        .checked_mul(4)
        .ok_or_else(|| failure(std::io::Error::other("WAV block alignment overflow")))?;
    let byte_rate = metadata
        .format
        .sample_rate_hz
        .checked_mul(u32::from(block_align))
        .ok_or_else(|| failure(std::io::Error::other("WAV byte rate overflow")))?;
    let data_bytes = (metadata.frame_count * u64::from(block_align)) as u32;
    let mut header = Vec::with_capacity(58);
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&(data_bytes + 50).to_le_bytes());
    header.extend_from_slice(b"WAVEfmt ");
    header.extend_from_slice(&18u32.to_le_bytes());
    header.extend_from_slice(&3u16.to_le_bytes()); // WAVE_FORMAT_IEEE_FLOAT
    header.extend_from_slice(&metadata.format.channels.to_le_bytes());
    header.extend_from_slice(&metadata.format.sample_rate_hz.to_le_bytes());
    header.extend_from_slice(&byte_rate.to_le_bytes());
    header.extend_from_slice(&block_align.to_le_bytes());
    header.extend_from_slice(&32u16.to_le_bytes());
    header.extend_from_slice(&0u16.to_le_bytes()); // cbSize
    header.extend_from_slice(b"fact");
    header.extend_from_slice(&4u32.to_le_bytes());
    header.extend_from_slice(&(metadata.frame_count as u32).to_le_bytes());
    header.extend_from_slice(b"data");
    header.extend_from_slice(&data_bytes.to_le_bytes());
    let mut writer = BufWriter::new(std::fs::File::create(path).map_err(failure)?);
    let mut hash = Sha256::new();
    writer.write_all(&header).map_err(failure)?;
    hash.update(&header);
    let mut buffer = vec![0.0; 65536 * channels];
    let mut bytes = Vec::with_capacity(buffer.len() * 4);
    for start in (0..metadata.frame_count).step_by(65536) {
        if cancel.is_cancelled() {
            return Err(DomainError::SeparationCancelled);
        }
        let frames = (metadata.frame_count - start).min(65536) as usize;
        if source.read_frames(start, &mut buffer[..frames * channels])? != frames {
            return Err(DomainError::AudioAlignmentMismatch);
        }
        bytes.clear();
        for sample in &buffer[..frames * channels] {
            if !sample.is_finite() {
                return Err(DomainError::NonFiniteSample);
            }
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        writer.write_all(&bytes).map_err(failure)?;
        hash.update(&bytes);
    }
    writer.flush().map_err(failure)?;
    writer.get_ref().sync_all().map_err(failure)?;
    Ok(format!("{:x}", hash.finalize()))
}

/// Reads float32 WAVs exactly and normalizes supported signed PCM WAVs to `f32`.
pub fn read_wav_f32(path: &Path) -> Result<AudioBufferF32, DomainError> {
    let mut reader = WavReader::open(path)
        .map_err(|error| DomainError::AudioDecodeFailure(error.to_string()))?;
    let spec = reader.spec();
    let samples = match spec.sample_format {
        SampleFormat::Float if spec.bits_per_sample == 32 => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DomainError::AudioDecodeFailure(error.to_string()))?,
        SampleFormat::Float => {
            return Err(DomainError::AudioDecodeFailure(
                "only 32-bit float WAV files are supported".into(),
            ));
        }
        SampleFormat::Int if spec.bits_per_sample == 16 => reader
            .samples::<i16>()
            .map(|sample| sample.map(|value| f32::from(value) / 32768.0))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DomainError::AudioDecodeFailure(error.to_string()))?,
        SampleFormat::Int if (17..=32).contains(&spec.bits_per_sample) => {
            let scale = (1u64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 / scale))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| DomainError::AudioDecodeFailure(error.to_string()))?
        }
        SampleFormat::Int => {
            return Err(DomainError::AudioDecodeFailure(format!(
                "unsupported PCM WAV bit depth: {}",
                spec.bits_per_sample
            )));
        }
    };
    let format = AudioFormat::new(spec.sample_rate, spec.channels)?;
    AudioBufferF32::new(format, samples)
}
