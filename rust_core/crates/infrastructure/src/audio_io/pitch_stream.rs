//! Bounded DSP worker between file/resampling producer and the device callback.
use super::aligned::{AlignedFrame, AlignedProducer, AlignedStream};
use crate::{
    live_pitch::{LivePitchBank, PITCH_BLOCK},
    live_pitch_control::LivePitchControl,
};
use atomusicator_domain::DomainError;
use rtrb::{PopError, PushError, RingBuffer};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
pub(super) const OUTPUT_CAPACITY: usize = 16384;

pub(super) fn open(
    paths: &[PathBuf],
    seek: f64,
    rate: u32,
    generation: u64,
    control: Arc<LivePitchControl>,
) -> Result<AlignedStream, DomainError> {
    if !seek.is_finite() || seek < 0.0 {
        return Err(fail("invalid pitch seek"));
    }
    let mut processors = LivePitchBank::new(rate)?;
    let delay = processors.latency_frames();
    let metadata = super::wav_codec::WavBlockReader::open(
        paths.first().ok_or_else(|| fail("five stems required"))?,
    )?
    .metadata();
    let duration = metadata.frame_count as f64 / f64::from(metadata.format.sample_rate_hz);
    let wanted = ((duration - seek).max(0.0) * f64::from(rate)).ceil() as u64;
    // Real preceding audio warms spectral history after a seek. The processor
    // and common timeline both discard this preroll before publishing frames.
    let preroll = if wanted == 0 {
        0
    } else {
        (seek * f64::from(rate)).floor().min(delay as f64) as u64
    };
    let raw = AlignedStream::open(
        paths,
        seek - preroll as f64 / f64::from(rate),
        rate,
        generation,
        4096,
    )?;
    let total = wanted + preroll;
    let (mut queue, consumer) = RingBuffer::new(OUTPUT_CAPACITY);
    let cancel = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    let worker_finished = finished.clone();
    let worker_failed = failed.clone();
    let (ready, buffered) = mpsc::sync_channel(1);
    let worker = thread::Builder::new()
        .name("atomusicator-live-pitch".into())
        .spawn(move || {
            let mut raw = raw;
            let source_failed = raw.producer.failed.clone();
            let _raw_guard = raw.producer;
            let empty = AlignedFrame {
                generation,
                position_seconds: seek,
                samples: [[0.0; 2]; 5],
                pitch_revision: 0,
            };
            let mut history = vec![empty; delay];
            let mut cursor = 0;
            let mut input = [[0.0f32; PITCH_BLOCK * 2]; 5];
            let mut output = [[0.0f32; PITCH_BLOCK * 2]; 5];
            let mut frames = [empty; PITCH_BLOCK];
            let mut seen = 0u64;
            let mut emitted = 0u64;
            let mut wet = 0.0f32;
            let mut ready = Some(ready);
            let result = (|| -> Result<(), DomainError> {
                while seen < total + delay as u64 {
                    let count = (total + delay as u64 - seen).min(PITCH_BLOCK as u64) as usize;
                    for offset in 0..PITCH_BLOCK {
                        let frame = if seen + (offset as u64) < total && offset < count {
                            loop {
                                if worker_cancel.load(Ordering::Acquire) {
                                    return Ok(());
                                }
                                match raw.consumer.pop() {
                                    Ok(frame) => break frame,
                                    Err(PopError::Empty) if raw.consumer.is_abandoned() => {
                                        // Resampling endpoints computed using
                                        // floating-point ceil may differ by one.
                                        if seen + offset as u64 + 1 == total
                                            && !source_failed.load(Ordering::Acquire)
                                        {
                                            break AlignedFrame {
                                                position_seconds: duration,
                                                ..empty
                                            };
                                        }
                                        return Err(fail(
                                            "source ended before its declared frame count",
                                        ));
                                    }
                                    Err(PopError::Empty) => {
                                        thread::sleep(Duration::from_micros(200))
                                    }
                                }
                            }
                        } else {
                            empty
                        };
                        frames[offset] = frame;
                        for (track, buffer) in input.iter_mut().enumerate() {
                            buffer[offset * 2..offset * 2 + 2]
                                .copy_from_slice(&frame.samples[track]);
                        }
                    }
                    let snapshot = control.snapshot();
                    processors.process(&input, &mut output, snapshot.spec)?;
                    let target_wet = if snapshot.spec.semitones() == 0.0 {
                        0.0
                    } else {
                        1.0
                    };
                    for offset in 0..count {
                        let mut frame = history[cursor];
                        history[cursor] = frames[offset];
                        cursor = (cursor + 1) % delay;
                        wet += (target_wet - wet).clamp(-1.0 / 256.0, 1.0 / 256.0);
                        for (track, output) in frame.samples.iter_mut().zip(&output) {
                            for channel in 0..2 {
                                track[channel] +=
                                    wet * (output[offset * 2 + channel] - track[channel]);
                            }
                        }
                        frame.pitch_revision = snapshot.revision;
                        if seen + (offset as u64) < delay as u64 + preroll {
                            continue;
                        }
                        loop {
                            if worker_cancel.load(Ordering::Acquire) {
                                return Ok(());
                            }
                            let buffered = OUTPUT_CAPACITY - queue.slots();
                            control.update_buffered(buffered);
                            if buffered >= control.queue_target().min(OUTPUT_CAPACITY) {
                                thread::sleep(Duration::from_micros(200));
                                continue;
                            }
                            match queue.push(frame) {
                                Ok(()) => {
                                    control.update_buffered(buffered + 1);
                                    break;
                                }
                                Err(PushError::Full(pending)) => {
                                    frame = pending;
                                    thread::sleep(Duration::from_micros(200));
                                }
                            }
                        }
                        emitted += 1;
                        if (emitted >= 256 || emitted == wanted)
                            && let Some(ready) = ready.take()
                        {
                            let _ = ready.send(Ok(()));
                        }
                    }
                    seen += count as u64;
                }
                if let Some(ready) = ready.take() {
                    let _ = ready.send(Ok(()));
                }
                Ok(())
            })();
            if let Err(error) = result {
                worker_failed.store(true, Ordering::Release);
                if let Some(ready) = ready.take() {
                    let _ = ready.send(Err(error.to_string()));
                }
            }
            worker_finished.store(true, Ordering::Release);
        })
        .map_err(|e| fail(e.to_string()))?;
    let producer = AlignedProducer {
        cancel,
        finished,
        failed,
        worker: Some(worker),
    };
    match buffered.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(AlignedStream {
            consumer,
            producer,
            metadata,
        }),
        Ok(Err(error)) => Err(fail(error)),
        Err(error) => Err(fail(error.to_string())),
    }
}
fn fail(s: impl std::fmt::Display) -> DomainError {
    DomainError::DspFailure(s.to_string())
}
