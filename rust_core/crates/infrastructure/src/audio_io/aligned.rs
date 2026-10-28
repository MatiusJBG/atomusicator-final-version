//! One bounded producer and clock for five stems. Resampling and WAV access stay off the callback.
use super::wav_codec::WavBlockReader;
use atomusicator_domain::{AudioAssetMetadata, DomainError};
use rtrb::{Consumer, PushError, RingBuffer};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
const TAPS: usize = 64;
const PHASES: usize = 1024;
const BLOCK: usize = 4096;
#[derive(Clone, Copy, Debug)]
pub struct AlignedFrame {
    pub generation: u64,
    pub position_seconds: f64,
    pub samples: [[f32; 2]; 5],
}
pub struct AlignedProducer {
    cancel: Arc<AtomicBool>,
    pub finished: Arc<AtomicBool>,
    pub failed: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
pub struct AlignedStream {
    pub consumer: Consumer<AlignedFrame>,
    pub producer: AlignedProducer,
    pub metadata: AudioAssetMetadata,
}
impl Drop for AlignedProducer {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl AlignedStream {
    pub fn open(
        paths: &[PathBuf],
        seek_seconds: f64,
        output_rate: u32,
        generation: u64,
        capacity: usize,
    ) -> Result<Self, DomainError> {
        if paths.len() != 5
            || !seek_seconds.is_finite()
            || seek_seconds < 0.0
            || output_rate == 0
            || capacity == 0
        {
            return Err(error("invalid aligned stream request"));
        }
        let mut readers: Vec<_> = paths
            .iter()
            .map(|p| WavBlockReader::open(p))
            .collect::<Result<_, _>>()?;
        let metadata = readers[0].metadata();
        if metadata.frame_count == 0 || readers.iter().any(|r| r.metadata() != metadata) {
            return Err(DomainError::AudioAlignmentMismatch);
        }
        let (mut queue, consumer) = RingBuffer::new(capacity);
        let cancel = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let worker_finished = Arc::clone(&finished);
        let worker_failed = Arc::clone(&failed);
        let (ready, buffered) = mpsc::sync_channel::<Result<(), String>>(1);
        let worker = thread::Builder::new()
            .name("atomusicator-aligned-reader".into())
            .spawn(move || {
                let result = (|| -> Result<(), DomainError> {
                    let input_rate = f64::from(metadata.format.sample_rate_hz);
                    let rate = f64::from(output_rate);
                    let duration = metadata.frame_count as f64 / input_rate;
                    let seek = seek_seconds.min(duration);
                    let output_frames = ((duration - seek) * rate).ceil() as u64;
                    let channels = usize::from(metadata.format.channels);
                    let table = sinc_table((rate / input_rate).min(1.0) * 0.95);
                    let mut cache = vec![vec![0.0; (BLOCK + TAPS * 2) * channels]; 5];
                    let mut cache_start = u64::MAX;
                    let mut cache_frames = 0;
                    let mut ready = Some(ready);
                    let mut sent = 0usize;
                    for index in 0..output_frames {
                        if worker_cancel.load(Ordering::Acquire) {
                            return Ok(());
                        }
                        let position = seek * input_rate + index as f64 * input_rate / rate;
                        let center = (position.floor() as u64).min(metadata.frame_count - 1);
                        let low = center.saturating_sub(TAPS as u64 / 2);
                        let high = (center + TAPS as u64 / 2 + 1).min(metadata.frame_count);
                        if cache_start == u64::MAX
                            || low < cache_start
                            || high > cache_start + cache_frames as u64
                        {
                            cache_start = low;
                            for (reader, buffer) in readers.iter_mut().zip(&mut cache) {
                                let count = reader.read_frames(cache_start, buffer)?;
                                if count == 0 {
                                    return Err(error("stem ended early"));
                                }
                                cache_frames = count;
                            }
                        }
                        let mut samples = [[0.0; 2]; 5];
                        let fraction = position - position.floor();
                        if output_rate == metadata.format.sample_rate_hz && fraction < 1e-9 {
                            for (track, buffer) in samples.iter_mut().zip(&cache) {
                                let offset = (center - cache_start) as usize * channels;
                                track[0] = buffer[offset];
                                track[1] = buffer[offset + channels - 1];
                            }
                        } else {
                            let phase = ((fraction * PHASES as f64).round() as usize).min(PHASES);
                            for (tap, weight) in table[phase].iter().enumerate() {
                                let frame = (center as i128 + tap as i128 - (TAPS / 2 - 1) as i128)
                                    .clamp(0, metadata.frame_count as i128 - 1)
                                    as u64;
                                let offset = (frame - cache_start) as usize * channels;
                                for (track, buffer) in samples.iter_mut().zip(&cache) {
                                    track[0] += buffer[offset] * weight;
                                    track[1] += buffer[offset + channels - 1] * weight;
                                }
                            }
                        }
                        let mut frame = AlignedFrame {
                            generation,
                            position_seconds: (seek + (index + 1) as f64 / rate).min(duration),
                            samples,
                        };
                        loop {
                            match queue.push(frame) {
                                Ok(()) => break,
                                Err(PushError::Full(value)) => {
                                    frame = value;
                                    if worker_cancel.load(Ordering::Acquire) {
                                        return Ok(());
                                    }
                                    thread::sleep(Duration::from_millis(1));
                                }
                            }
                        }
                        sent += 1;
                        if sent >= capacity.min(1024)
                            && let Some(ready) = ready.take()
                        {
                            let _ = ready.send(Ok(()));
                        }
                    }
                    if let Some(ready) = ready.take() {
                        let _ = ready.send(Ok(()));
                    }
                    Ok(())
                })();
                if result.is_err() {
                    worker_failed.store(true, Ordering::Release);
                }
                worker_finished.store(true, Ordering::Release);
            })
            .map_err(error)?;
        let producer = AlignedProducer {
            cancel,
            finished,
            failed,
            worker: Some(worker),
        };
        match buffered.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(Self {
                consumer,
                producer,
                metadata,
            }),
            _ => {
                drop(producer);
                Err(error("aligned playback could not prebuffer the stems"))
            }
        }
    }
}
fn sinc_table(cutoff: f64) -> Vec<[f32; TAPS]> {
    (0..=PHASES)
        .map(|phase| {
            let fraction = phase as f64 / PHASES as f64;
            let mut values = [0.0; TAPS];
            let mut sum = 0.0;
            for (tap, value) in values.iter_mut().enumerate() {
                let x = tap as f64 - (TAPS / 2 - 1) as f64 - fraction;
                let sinc = if x.abs() < 1e-12 {
                    cutoff
                } else {
                    (std::f64::consts::PI * x * cutoff).sin() / (std::f64::consts::PI * x)
                };
                let window = 0.42
                    - 0.5 * (2.0 * std::f64::consts::PI * tap as f64 / (TAPS - 1) as f64).cos()
                    + 0.08 * (4.0 * std::f64::consts::PI * tap as f64 / (TAPS - 1) as f64).cos();
                *value = (sinc * window) as f32;
                sum += f64::from(*value);
            }
            for value in &mut values {
                *value /= sum as f32;
            }
            values
        })
        .collect()
}
fn error(value: impl std::fmt::Display) -> DomainError {
    DomainError::AudioDecodeFailure(value.to_string())
}
