//! Opt-in muted device probe. Captures only our own synthetic samples before
//! output gain; no microphone or system-loopback recording is performed.
//! Driver timestamps estimate presentation, not acoustic latency.
use atomusicator_domain::{AudioBufferF32, AudioFormat, PitchShiftSpec};
use atomusicator_infrastructure::{
    audio_io::{aligned::AlignedStream, wav_codec::write_wav_f32},
    audio_tools::TemporaryDirectory,
    live_pitch_control::LivePitchControl,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

fn energy(samples: &VecDeque<f32>, frequency: f64, rate: u32) -> f64 {
    let mut real = 0.0;
    let mut imaginary = 0.0;
    for (i, sample) in samples.iter().enumerate() {
        let angle = std::f64::consts::TAU * frequency * i as f64 / f64::from(rate);
        let window = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / 511.0).cos();
        real += f64::from(*sample) * window * angle.cos();
        imaginary += f64::from(*sample) * window * angle.sin();
    }
    real * real + imaginary * imaginary
}

fn main() {
    let device = cpal::default_host()
        .default_output_device()
        .expect("output device");
    let supported = device.default_output_config().unwrap();
    assert_eq!(supported.sample_format(), cpal::SampleFormat::F32);
    let rate = supported.sample_rate().0;
    let channels = usize::from(supported.channels());
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let source = root.0.join("own-synthetic-tone.wav");
    let samples = (0..rate * 12)
        .flat_map(|i| {
            let value = (std::f32::consts::TAU * 440.0 * i as f32 / rate as f32).sin() * 0.2;
            [value, value]
        })
        .collect();
    write_wav_f32(
        &source,
        &AudioBufferF32::new(AudioFormat::new(rate, 2).unwrap(), samples).unwrap(),
    )
    .unwrap();
    let control = Arc::new(LivePitchControl::default());
    control.begin_output_probe();
    let prepared =
        AlignedStream::open_pitched(&vec![source; 5], 0.0, rate, 1, control.clone()).unwrap();
    let mut consumer = prepared.consumer;
    let _producer = prepared.producer;
    let playing = Arc::new(AtomicBool::new(false));
    let underruns = Arc::new(AtomicUsize::new(0));
    let overflow = Arc::new(AtomicUsize::new(0));
    let driver_errors = Arc::new(AtomicUsize::new(0));
    let (mut capture, mut captured) = rtrb::RingBuffer::new(rate as usize);
    let callback_control = control.clone();
    let callback_playing = playing.clone();
    let callback_underruns = underruns.clone();
    let callback_overflow = overflow.clone();
    let callback_errors = driver_errors.clone();
    let started = Instant::now();
    let stream = device
        .build_output_stream(
            &supported.config(),
            move |output: &mut [f32], info| {
                output.fill(0.0);
                callback_control.observe_output_buffer(output.len() / channels);
                if !callback_playing.load(Ordering::Acquire) {
                    return;
                }
                let timestamp = info.timestamp();
                let delay = timestamp
                    .playback
                    .duration_since(&timestamp.callback)
                    .unwrap_or_default();
                let presentation = started.elapsed() + delay;
                for i in 0..output.len() / channels {
                    if let Ok(frame) = consumer.pop() {
                        let time = presentation.as_secs_f64() + i as f64 / f64::from(rate);
                        if capture.push((time, frame.samples[0][0])).is_err() {
                            callback_overflow.fetch_add(1, Ordering::Relaxed);
                        }
                    } else {
                        callback_underruns.fetch_add(1, Ordering::Relaxed);
                    }
                }
            },
            move |_| {
                callback_errors.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )
        .unwrap();
    stream.play().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !control.output_primed() {
        assert!(Instant::now() < deadline, "output did not prime");
        std::thread::sleep(Duration::from_millis(1));
    }
    playing.store(true, Ordering::Release);
    let mut recent = VecDeque::with_capacity(512);
    let mut changes = Vec::new();
    let mut next_change = Instant::now() + Duration::from_millis(400);
    let mut waiting = None;
    let mut revision = 0;
    let mut analyzed_frames = 0usize;
    while changes.len() < 16 {
        assert!(
            started.elapsed() < Duration::from_secs(11),
            "pitch response timed out"
        );
        if waiting.is_none() && Instant::now() >= next_change {
            revision += 1;
            let semitones = if revision % 2 == 1 { 12.0 } else { 0.0 };
            control
                .set(
                    0,
                    revision,
                    PitchShiftSpec::try_new(semitones, true).unwrap(),
                )
                .unwrap();
            waiting = Some((started.elapsed().as_secs_f64(), semitones));
        }
        while let Ok((presentation, sample)) = captured.pop() {
            analyzed_frames += 1;
            if recent.len() == 512 {
                recent.pop_front();
            }
            recent.push_back(sample);
            // Analysis occurs only here, never on the device callback.
            if analyzed_frames.is_multiple_of(32)
                && recent.len() == 512
                && let Some((changed, semitones)) = waiting
            {
                let ratio = energy(&recent, 880.0, rate) / energy(&recent, 440.0, rate).max(1e-20);
                if presentation > changed
                    && ((semitones > 0.0 && ratio > 2.0) || (semitones == 0.0 && ratio < 0.5))
                {
                    changes.push((presentation - changed) * 1000.0);
                    waiting = None;
                    next_change = Instant::now() + Duration::from_millis(310 + revision * 7);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    playing.store(false, Ordering::Release);
    drop(stream);
    changes.sort_by(f64::total_cmp);
    let p95 = changes[changes.len() * 95 / 100];
    println!(
        "{{\"scope\":\"muted native output with driver-estimated presentation\",\"acoustic_latency_measured\":false,\"sample_rate\":{rate},\"callback_frames\":{},\"queue_target_frames\":{},\"changes\":{},\"p50_ms\":{},\"p95_ms\":{p95},\"max_ms\":{},\"underrun_frames\":{},\"capture_overflows\":{},\"driver_errors\":{}}}",
        control.output_buffer_frames(),
        control.queue_target(),
        changes.len(),
        changes[changes.len() / 2],
        changes[changes.len() - 1],
        underruns.load(Ordering::Acquire),
        overflow.load(Ordering::Acquire),
        driver_errors.load(Ordering::Acquire)
    );
    assert_eq!(underruns.load(Ordering::Acquire), 0);
    assert_eq!(overflow.load(Ordering::Acquire), 0);
    assert_eq!(driver_errors.load(Ordering::Acquire), 0);
    assert!(
        p95 <= 100.0,
        "device presentation estimate misses the 100ms target"
    );
    assert!(changes[changes.len() - 1] <= 150.0);
}
