//! Background audio decoding into a bounded SPSC queue for realtime consumers.
//!
//! Disk reads and codec work run on a dedicated producer thread. When the
//! queue is full, only that producer sleeps briefly; the audio callback should
//! call `try_read_sample` and treat an empty queue as silence.

use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use atomusicator_domain::{AudioAssetMetadata, AudioFormat, DomainError};
use rtrb::{Consumer, PushError, RingBuffer};
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

pub const STREAM_THRESHOLD_BYTES: u64 = 256 * 1024 * 1024;
const DEFAULT_RING_CAPACITY_SAMPLES: usize = 48_000;

/// A disk-backed source with a bounded decoded-sample queue. The consumer is
/// intentionally mutable so it can be moved directly into a single callback.
pub struct BufferedStreamTrackSource {
    format: AudioFormat,
    consumer: Consumer<f32>,
    producer: StreamProducerGuard,
}

/// Own this on a control thread, separate from the realtime callback's consumer.
pub struct StreamProducerGuard {
    cancel: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    failure: Arc<Mutex<Option<String>>>,
    worker: Option<JoinHandle<()>>,
}

impl BufferedStreamTrackSource {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DomainError> {
        Self::open_with_capacity(path, DEFAULT_RING_CAPACITY_SAMPLES)
    }

    pub fn open_with_capacity(
        path: impl AsRef<Path>,
        capacity_samples: usize,
    ) -> Result<Self, DomainError> {
        let path = path.as_ref().to_path_buf();
        if capacity_samples == 0 {
            return Err(DomainError::AudioDecodeFailure(
                "stream ring capacity must be greater than zero".into(),
            ));
        }
        let metadata = probe_stream_metadata(&path)?;
        let (mut producer, consumer) = RingBuffer::<f32>::new(capacity_samples);
        let cancel = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let failure = Arc::new(Mutex::new(None));
        let worker_cancel = Arc::clone(&cancel);
        let worker_finished = Arc::clone(&finished);
        let worker_failure = Arc::clone(&failure);
        let worker = thread::Builder::new()
            .name("atomusicator-audio-reader".into())
            .spawn(move || {
                let result = decode_to_ring(&path, metadata.format, &mut producer, &worker_cancel);
                if let Err(error) = result
                    && let Ok(mut slot) = worker_failure.lock()
                {
                    *slot = Some(error);
                }
                worker_finished.store(true, Ordering::Release);
            })
            .map_err(|error| DomainError::AudioDecodeFailure(error.to_string()))?;

        Ok(Self {
            format: metadata.format,
            consumer,
            producer: StreamProducerGuard {
                cancel,
                finished,
                failure,
                worker: Some(worker),
            },
        })
    }

    pub fn format(&self) -> AudioFormat {
        self.format
    }

    /// Nonblocking read intended for the audio callback.
    pub fn try_read_sample(&mut self) -> Option<f32> {
        self.consumer.pop().ok()
    }

    /// Separates the callback-owned queue consumer from the background worker
    /// lifecycle. Keep the guard on a control thread; moving it into an audio
    /// callback would make callback teardown wait for the decoder thread.
    pub fn into_parts(self) -> (Consumer<f32>, StreamProducerGuard) {
        (self.consumer, self.producer)
    }

    pub fn is_finished(&self) -> bool {
        self.producer.is_finished()
    }

    /// Reads a background decoder failure from a non-realtime thread.
    pub fn take_failure(&self) -> Result<Option<String>, DomainError> {
        self.producer.take_failure()
    }
}

impl StreamProducerGuard {
    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    pub fn take_failure(&self) -> Result<Option<String>, DomainError> {
        self.failure
            .lock()
            .map(|mut failure| failure.take())
            .map_err(|_| DomainError::AudioDecodeFailure("stream error state was poisoned".into()))
    }
}

impl Drop for StreamProducerGuard {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn should_stream(metadata: &AudioAssetMetadata) -> bool {
    let estimated_bytes = metadata
        .frame_count
        .saturating_mul(u64::from(metadata.format.channels))
        .saturating_mul(std::mem::size_of::<f32>() as u64);
    estimated_bytes > STREAM_THRESHOLD_BYTES
}

fn probe_stream_metadata(path: &Path) -> Result<AudioAssetMetadata, DomainError> {
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
    let format = AudioFormat::new(sample_rate, channels)?;
    Ok(AudioAssetMetadata {
        format,
        frame_count: track.codec_params.n_frames.unwrap_or(0),
    })
}

fn decode_to_ring(
    path: &PathBuf,
    expected_format: AudioFormat,
    producer: &mut rtrb::Producer<f32>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let file = File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
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
        .map_err(|error| error.to_string())?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| "file has no default audio track".to_owned())?;
    if track.codec_params.codec == CODEC_TYPE_NULL {
        return Err("default track has no audio codec".into());
    }
    let track_id = track.id;
    let codec_params = track.codec_params.clone();
    let mut decoder = default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .map_err(|error| error.to_string())?;

    while !cancel.load(Ordering::Acquire) {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::ResetRequired) => return Err("format reset is unsupported".into()),
            Err(error) => return Err(error.to_string()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(error) => return Err(error.to_string()),
        };
        if decoded.spec().rate != expected_format.sample_rate_hz
            || decoded.spec().channels.count() != usize::from(expected_format.channels)
        {
            return Err("audio format changed during stream decoding".into());
        }
        let mut interleaved = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        interleaved.copy_interleaved_ref(decoded);
        for &sample in interleaved.samples() {
            let mut pending = sample;
            loop {
                if cancel.load(Ordering::Acquire) {
                    return Ok(());
                }
                match producer.push(pending) {
                    Ok(()) => break,
                    Err(PushError::Full(sample)) => {
                        pending = sample;
                        thread::park_timeout(Duration::from_millis(2));
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_io::wav_codec::write_wav_f32;
    use atomusicator_domain::AudioBufferF32;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    #[test]
    fn background_decoder_feeds_a_bounded_queue_without_losing_samples() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "atomusicator-stream-{}-{nonce}.wav",
            std::process::id()
        ));
        let format = AudioFormat::new(48_000, 1).unwrap();
        let samples = vec![0.0, 0.25, -0.5, 0.75, -1.0, 0.5, 0.125, -0.125];
        let buffer = AudioBufferF32::new(format, samples.clone()).unwrap();
        write_wav_f32(&path, &buffer).unwrap();

        let mut source = BufferedStreamTrackSource::open_with_capacity(&path, 3).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut decoded = Vec::new();
        while Instant::now() < deadline {
            if let Some(sample) = source.try_read_sample() {
                decoded.push(sample);
            } else if source.is_finished() {
                break;
            } else {
                thread::sleep(Duration::from_millis(1));
            }
        }
        let failure = source.take_failure().unwrap();
        drop(source);
        let _ = std::fs::remove_file(&path);

        assert_eq!(failure, None);
        assert_eq!(decoded.len(), samples.len());
        for (actual, expected) in decoded.iter().zip(samples) {
            assert!((actual - expected).abs() < 1.0e-6);
        }
    }
}
