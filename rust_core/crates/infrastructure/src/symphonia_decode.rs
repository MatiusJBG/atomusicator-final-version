//! File decode used when the pinned FFmpeg executable is not on this device.
//! WAV, FLAC, MP3 and M4A go through Symphonia. The separation spool stays
//! stereo 44.1 kHz float, matching the FFmpeg command.

use std::{
    fs::File,
    io::Write,
    path::Path,
};

use atomusicator_application::CancellationToken;
use atomusicator_domain::{AudioBufferF32, AudioFormat, DomainError};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::DecoderOptions,
    errors::Error as SymphoniaError,
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};

const SPOOL_RATE: u32 = 44100;
const SPOOL_CHANNELS: u16 = 2;

pub fn decode_buffer(
    path: &Path,
    cancel: &CancellationToken,
) -> Result<AudioBufferF32, DomainError> {
    let (rate, channels, samples) = read_interleaved(path, cancel)?;
    AudioBufferF32::new(
        AudioFormat::new(rate, u16::try_from(channels).unwrap_or(SPOOL_CHANNELS))?,
        samples,
    )
}

pub fn convert(
    audio: &AudioBufferF32,
    sample_rate: u32,
    channels: u16,
) -> Result<AudioBufferF32, DomainError> {
    let stereo = to_stereo_rate(
        audio.samples(),
        usize::from(audio.format().channels),
        audio.format().sample_rate_hz,
        sample_rate,
    );
    let samples = if channels == 1 {
        stereo.chunks(2).map(|frame| (frame[0] + frame[1]) * 0.5).collect()
    } else {
        stereo
    };
    AudioBufferF32::new(AudioFormat::new(sample_rate, channels)?, samples)
}

pub fn write_spool(
    source: &Path,
    target: &Path,
    cancel: &CancellationToken,
) -> Result<(), DomainError> {
    let audio = convert(&decode_buffer(source, cancel)?, SPOOL_RATE, SPOOL_CHANNELS)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(decode_error)?;
    }
    let mut file = File::create(target).map_err(decode_error)?;
    for sample in audio.samples() {
        if cancel.is_cancelled() {
            return Err(DomainError::SeparationCancelled);
        }
        file.write_all(&sample.to_le_bytes()).map_err(decode_error)?;
    }
    file.sync_all().map_err(decode_error)?;
    Ok(())
}

fn read_interleaved(
    path: &Path,
    cancel: &CancellationToken,
) -> Result<(u32, usize, Vec<f32>), DomainError> {
    let file = File::open(path).map_err(decode_error)?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let probed = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(decode_error)?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| DomainError::AudioDecodeFailure("audio has no playable track".into()))?;
    let track_id = track.id;
    let rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| DomainError::AudioDecodeFailure("audio sample rate is missing".into()))?;
    let channels = track
        .codec_params
        .channels
        .map(|channels| channels.count())
        .filter(|count| *count > 0)
        .ok_or_else(|| DomainError::AudioDecodeFailure("audio channel count is missing".into()))?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(decode_error)?;
    let mut samples = Vec::new();
    loop {
        if cancel.is_cancelled() {
            return Err(DomainError::SeparationCancelled);
        }
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(error) => return Err(decode_error(error)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                buffer.copy_interleaved_ref(decoded);
                samples.extend_from_slice(buffer.samples());
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(error) => return Err(decode_error(error)),
        }
    }
    if samples.is_empty() || samples.len() < channels {
        return Err(DomainError::AudioDecodeFailure("audio is empty".into()));
    }
    Ok((rate, channels, samples))
}

fn to_stereo_rate(interleaved: &[f32], channels: usize, from_rate: u32, to_rate: u32) -> Vec<f32> {
    let frames = interleaved.len() / channels;
    let stereo: Vec<(f32, f32)> = (0..frames)
        .map(|frame| {
            let base = frame * channels;
            let left = interleaved[base];
            let right = if channels == 1 {
                left
            } else {
                interleaved[base + 1]
            };
            (left, right)
        })
        .collect();
    if frames == 0 {
        return Vec::new();
    }
    if from_rate == to_rate {
        return stereo.into_iter().flat_map(|(left, right)| [left, right]).collect();
    }
    let out_frames = ((frames as u64).saturating_mul(u64::from(to_rate)) / u64::from(from_rate)).max(1) as usize;
    let mut output = Vec::with_capacity(out_frames * 2);
    for index in 0..out_frames {
        let position = index as f64 * f64::from(from_rate) / f64::from(to_rate);
        let left_index = (position.floor() as usize).min(frames - 1);
        let right_index = (left_index + 1).min(frames - 1);
        let fraction = (position - left_index as f64) as f32;
        let (left_a, right_a) = stereo[left_index];
        let (left_b, right_b) = stereo[right_index];
        output.push(left_a + (left_b - left_a) * fraction);
        output.push(right_a + (right_b - right_a) * fraction);
    }
    output
}

fn decode_error(error: impl std::fmt::Display) -> DomainError {
    DomainError::AudioDecodeFailure(error.to_string())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioPreview {
    pub duration: f64,
    pub sample_rate: u32,
    pub channels: u16,
    pub bit_depth: Option<u32>,
    pub peaks: Vec<f32>,
}

pub fn preview_audio(
    path: &Path,
    buckets: usize,
    cancel: &CancellationToken,
) -> Result<AudioPreview, DomainError> {
    let buckets = buckets.clamp(16, 4096);
    let file = File::open(path).map_err(decode_error)?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(decode_error)?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| DomainError::AudioDecodeFailure("audio has no playable track".into()))?;
    let track_id = track.id;
    let rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| DomainError::AudioDecodeFailure("audio sample rate is missing".into()))?;
    let channels = track
        .codec_params
        .channels
        .map(|c| c.count())
        .filter(|count| *count > 0)
        .ok_or_else(|| DomainError::AudioDecodeFailure("audio channel count is missing".into()))?;
    let bit_depth = track.codec_params.bits_per_sample;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(decode_error)?;

    const WINDOW_FRAMES: usize = 64;
    let mut windows: Vec<(f32, f32)> = Vec::new();
    let mut win_min = 0.0_f32;
    let mut win_max = 0.0_f32;
    let mut win_count = 0_usize;
    let mut total_frames = 0_u64;

    loop {
        if cancel.is_cancelled() {
            return Err(DomainError::SeparationCancelled);
        }
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(error) => return Err(decode_error(error)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let mut buffer =
                    SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                buffer.copy_interleaved_ref(decoded);
                let inv_channels = 1.0 / (channels as f32);
                for frame in buffer.samples().chunks_exact(channels) {
                    let mono = frame.iter().copied().sum::<f32>() * inv_channels;
                    if win_count == 0 {
                        win_min = mono;
                        win_max = mono;
                    } else {
                        if mono < win_min {
                            win_min = mono;
                        }
                        if mono > win_max {
                            win_max = mono;
                        }
                    }
                    win_count += 1;
                    total_frames += 1;
                    if win_count == WINDOW_FRAMES {
                        windows.push((win_min, win_max));
                        win_count = 0;
                    }
                }
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(error) => return Err(decode_error(error)),
        }
    }
    if win_count > 0 {
        windows.push((win_min, win_max));
    }
    if total_frames == 0 || windows.is_empty() {
        return Err(DomainError::AudioDecodeFailure("audio is empty".into()));
    }

    let mut peaks = vec![0.0_f32; buckets * 2];
    let total_windows = windows.len();
    for (idx, &(w_min, w_max)) in windows.iter().enumerate() {
        let bucket = (idx * buckets / total_windows).min(buckets - 1);
        let base = bucket * 2;
        if w_min < peaks[base] {
            peaks[base] = w_min;
        }
        if w_max > peaks[base + 1] {
            peaks[base + 1] = w_max;
        }
    }

    Ok(AudioPreview {
        duration: total_frames as f64 / f64::from(rate),
        sample_rate: rate,
        channels: u16::try_from(channels).unwrap_or(SPOOL_CHANNELS),
        bit_depth,
        peaks,
    })
}
