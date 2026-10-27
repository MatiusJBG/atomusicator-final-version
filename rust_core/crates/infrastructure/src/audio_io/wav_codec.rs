use std::path::Path;

use atomusicator_domain::{AudioBufferF32, AudioFormat, DomainError};
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};

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
