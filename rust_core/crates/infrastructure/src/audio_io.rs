pub mod aligned;
mod pitch_stream;
pub mod playback;
pub mod streamer;
pub mod wav_codec;

use std::{
    fs,
    fs::File,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use atomusicator_application::ports::{AudioDecoderPort, ExportAudioPort};
use atomusicator_domain::{AudioAssetMetadata, AudioBufferF32, AudioFormat, DomainError};
use hound::{SampleFormat, WavSpec, WavWriter};
use symphonia::{
    core::{
        audio::SampleBuffer,
        codecs::{CODEC_TYPE_NULL, DecoderOptions},
        errors::Error as SymphoniaError,
        formats::FormatOptions,
        io::{MediaSourceStream, MediaSourceStreamOptions},
        meta::MetadataOptions,
        probe::Hint,
    },
    default,
};

#[derive(Default)]
pub struct SymphoniaAudioDecoder;

impl AudioDecoderPort for SymphoniaAudioDecoder {
    fn probe_metadata(&self, path: &Path) -> Result<AudioAssetMetadata, DomainError> {
        let (sample_rate, channels, frames) = probe(path)?;
        Ok(AudioAssetMetadata {
            format: AudioFormat::new(sample_rate, channels)?,
            frame_count: frames.unwrap_or(0),
        })
    }

    fn decode_to_f32(&self, path: &Path) -> Result<AudioBufferF32, DomainError> {
        let file = File::open(path).map_err(|error| {
            DomainError::AudioDecodeFailure(format!("{}: {error}", path.display()))
        })?;
        let source = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
            hint.with_extension(extension);
        }
        let probed = default::get_probe()
            .format(
                &hint,
                source,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|error| DomainError::AudioDecodeFailure(error.to_string()))?;
        let mut format = probed.format;
        let track = format.default_track().ok_or_else(|| {
            DomainError::AudioDecodeFailure("file has no default audio track".into())
        })?;
        if track.codec_params.codec == CODEC_TYPE_NULL {
            return Err(DomainError::AudioDecodeFailure(
                "default track has no audio codec".into(),
            ));
        }
        let track_id = track.id;
        let sample_rate = track.codec_params.sample_rate.ok_or_else(|| {
            DomainError::AudioDecodeFailure("audio track does not declare a sample rate".into())
        })?;
        let channels = track
            .codec_params
            .channels
            .ok_or_else(|| {
                DomainError::AudioDecodeFailure(
                    "audio track does not declare a channel layout".into(),
                )
            })?
            .count();
        let codec_params = track.codec_params.clone();
        let format_info = AudioFormat::new(
            sample_rate,
            u16::try_from(channels)
                .map_err(|_| DomainError::AudioDecodeFailure("channel count exceeds u16".into()))?,
        )?;
        let mut decoder = default::get_codecs()
            .make(&codec_params, &DecoderOptions::default())
            .map_err(|error| DomainError::AudioDecodeFailure(error.to_string()))?;
        let mut samples = Vec::new();

        loop {
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(SymphoniaError::IoError(error))
                    if error.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(SymphoniaError::ResetRequired) => {
                    return Err(DomainError::AudioDecodeFailure(
                        "format reset is unsupported".into(),
                    ));
                }
                Err(error) => return Err(DomainError::AudioDecodeFailure(error.to_string())),
            };
            if packet.track_id() != track_id {
                continue;
            }
            let decoded = match decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(error) => return Err(DomainError::AudioDecodeFailure(error.to_string())),
            };
            let mut interleaved =
                SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
            interleaved.copy_interleaved_ref(decoded);
            samples.extend_from_slice(interleaved.samples());
        }
        AudioBufferF32::new(format_info, samples)
    }
}

fn probe(path: &Path) -> Result<(u32, u16, Option<u64>), DomainError> {
    let file = File::open(path)
        .map_err(|error| DomainError::AudioDecodeFailure(format!("{}: {error}", path.display())))?;
    let source = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
        hint.with_extension(extension);
    }
    let probed = default::get_probe()
        .format(
            &hint,
            source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|error| DomainError::AudioDecodeFailure(error.to_string()))?;
    let track = probed
        .format
        .default_track()
        .ok_or_else(|| DomainError::AudioDecodeFailure("file has no default audio track".into()))?;
    let sample_rate = track.codec_params.sample_rate.ok_or_else(|| {
        DomainError::AudioDecodeFailure("audio track does not declare a sample rate".into())
    })?;
    let channels = track
        .codec_params
        .channels
        .ok_or_else(|| {
            DomainError::AudioDecodeFailure("audio track does not declare a channel layout".into())
        })?
        .count();
    let channels = u16::try_from(channels)
        .map_err(|_| DomainError::AudioDecodeFailure("channel count exceeds u16".into()))?;
    AudioFormat::new(sample_rate, channels)?;
    Ok((sample_rate, channels, track.codec_params.n_frames))
}

#[derive(Default)]
pub struct HoundWavExporter;

impl ExportAudioPort for HoundWavExporter {
    fn write_wav_atomic(
        &self,
        path: &Path,
        audio: &AudioBufferF32,
        bits_per_sample: u16,
    ) -> Result<(), DomainError> {
        let sample_format = if bits_per_sample == 32 {
            SampleFormat::Float
        } else {
            SampleFormat::Int
        };
        if !matches!(bits_per_sample, 16 | 24 | 32) {
            return Err(DomainError::AudioExportFailure(
                "supported WAV depths are 16-bit PCM, 24-bit PCM, and 32-bit float".into(),
            ));
        }
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| {
            DomainError::AudioExportFailure(format!("{}: {error}", parent.display()))
        })?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary_path = path.with_file_name(format!(
            ".{}.{}.tmp.wav",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("export"),
            nonce
        ));
        let spec = WavSpec {
            channels: audio.format().channels,
            sample_rate: audio.format().sample_rate_hz,
            bits_per_sample,
            sample_format,
        };
        let write_result = (|| {
            let mut writer = WavWriter::create(&temporary_path, spec)
                .map_err(|error| DomainError::AudioExportFailure(error.to_string()))?;
            for sample in audio.samples() {
                let sample = sample.clamp(-1.0, 1.0);
                match sample_format {
                    SampleFormat::Float => writer.write_sample(sample),
                    SampleFormat::Int if bits_per_sample == 16 => {
                        writer.write_sample((sample * i16::MAX as f32).round() as i16)
                    }
                    SampleFormat::Int => writer.write_sample((sample * 8_388_607.0).round() as i32),
                }
                .map_err(|error| DomainError::AudioExportFailure(error.to_string()))?;
            }
            writer
                .finalize()
                .map_err(|error| DomainError::AudioExportFailure(error.to_string()))?;
            File::open(&temporary_path)
                .and_then(|file| file.sync_all())
                .map_err(|error| {
                    DomainError::AudioExportFailure(format!(
                        "{}: {error}",
                        temporary_path.display()
                    ))
                })?;
            publish_file(&temporary_path, path)
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        write_result
    }
}

fn publish_file(temporary: &Path, destination: &Path) -> Result<(), DomainError> {
    if !destination.exists() {
        return fs::rename(temporary, destination).map_err(|error| {
            DomainError::AudioExportFailure(format!("{}: {error}", destination.display()))
        });
    }
    let backup = destination.with_extension(format!("{}.previous", std::process::id()));
    fs::rename(destination, &backup).map_err(|error| {
        DomainError::AudioExportFailure(format!("{}: {error}", destination.display()))
    })?;
    match fs::rename(temporary, destination) {
        Ok(()) => {
            let _ = fs::remove_file(backup);
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(&backup, destination);
            Err(DomainError::AudioExportFailure(format!(
                "{}: {error}",
                destination.display()
            )))
        }
    }
}
