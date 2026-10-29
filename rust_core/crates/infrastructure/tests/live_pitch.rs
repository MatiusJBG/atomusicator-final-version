use atomusicator_domain::PitchShiftSpec;
use atomusicator_infrastructure::live_pitch::LivePitchProcessor;

fn frequency(samples: &[f32], rate: u32) -> f64 {
    // Formant compensation can add low-level ripples and extra zero crossings.
    // Measure the dominant sustained spectral component rather than count them.
    let length = samples.len().next_power_of_two() * 4;
    let fft = realfft::RealFftPlanner::<f32>::new().plan_fft_forward(length);
    let mut input = vec![0.0; length];
    for (i, value) in samples.iter().enumerate() {
        input[i] = value
            * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (samples.len() - 1) as f32).cos());
    }
    let mut spectrum = fft.make_output_vec();
    fft.process(&mut input, &mut spectrum).unwrap();
    let start = (20.0 * length as f64 / f64::from(rate)).ceil() as usize;
    let peak = (start..spectrum.len() - 1)
        .max_by(|a, b| spectrum[*a].norm_sqr().total_cmp(&spectrum[*b].norm_sqr()))
        .unwrap();
    let power = |i: usize| f64::from(spectrum[i].norm_sqr().max(1e-30)).ln();
    let correction = 0.5 * (power(peak - 1) - power(peak + 1))
        / (power(peak - 1) - 2.0 * power(peak) + power(peak + 1));
    (peak as f64 + correction) * f64::from(rate) / length as f64
}

#[test]
fn full_pitch_range_keeps_frequency_and_duration_with_both_formant_options() {
    for rate in [44100, 48000] {
        for semitones in [-12, -7, -1, 1, 7, 12] {
            for formants in [false, true] {
                let mut processor = LivePitchProcessor::new(rate).unwrap();
                processor.set_spec(PitchShiftSpec::try_new(semitones as f32, formants).unwrap());
                let mut samples = Vec::new();
                for block in 0..220 {
                    let input: Vec<_> = (0..256)
                        .flat_map(|i| {
                            let x = (std::f32::consts::TAU * 440.0 * (block * 256 + i) as f32
                                / rate as f32)
                                .sin()
                                * 0.2;
                            [x, x]
                        })
                        .collect();
                    let mut output = [0.0; 512];
                    processor.process(&input, &mut output).unwrap();
                    if block > 100 {
                        samples.extend(output.as_chunks::<2>().0.iter().map(|s| s[0]));
                    }
                }
                let hz = frequency(&samples, rate);
                let expected = 440.0 * 2.0f64.powf(semitones as f64 / 12.0);
                let cents = (1200.0 * (hz / expected).log2()).abs();
                assert!(
                    cents < 5.0,
                    "rate={rate}, semitones={semitones}, formants={formants}, cents={cents}"
                );
            }
        }
    }
}

#[test]
fn processor_preserves_tempo_stereo_and_pitch_without_whole_track_rendering() {
    let rate = 48000;
    let mut processor = LivePitchProcessor::new(rate).unwrap();
    processor.set_spec(PitchShiftSpec::try_new(12.0, false).unwrap());
    let mut samples = Vec::new();
    for block in 0..300 {
        let input: Vec<_> = (0..256)
            .flat_map(|i| {
                let x = (std::f32::consts::TAU * 440.0 * (block * 256 + i) as f32 / rate as f32)
                    .sin()
                    * 0.2;
                [x, x * 0.5]
            })
            .collect();
        let mut output = vec![0.0; input.len()];
        processor.process(&input, &mut output).unwrap();
        assert!(output.iter().all(|s| s.is_finite()));
        samples.extend_from_slice(&output);
    }
    let stable = &samples[48000..];
    let left: Vec<_> = stable.as_chunks::<2>().0.iter().map(|s| s[0]).collect();
    let crossings = left
        .windows(2)
        .filter(|s| s[0] <= 0.0 && s[1] > 0.0)
        .count();
    let frequency = crossings as f64 * rate as f64 / left.len() as f64;
    assert!(
        (1200.0 * (frequency / 880.0).log2()).abs() < 5.0,
        "frequency: {frequency}"
    );
    let stereo_error = stable
        .as_chunks::<2>()
        .0
        .iter()
        .map(|s| (s[1] - s[0] * 0.5).abs())
        .fold(0.0, f32::max);
    assert!(stereo_error < 0.01, "stereo error: {stereo_error}");
    assert_eq!(samples.len(), 300 * 256 * 2);
}

#[test]
fn invalid_buffers_are_rejected_without_native_processing() {
    let mut processor = LivePitchProcessor::new(44100).unwrap();
    assert!(processor.process(&[0.0; 3], &mut [0.0; 3]).is_err());
    assert!(
        processor
            .process(&[f32::NAN; 512], &mut [0.0; 512])
            .is_err()
    );
    assert!(LivePitchProcessor::new(0).is_err());
}

#[test]
fn zero_pitch_stream_is_sample_exact_and_keeps_short_stems_and_eof_aligned() {
    use atomusicator_domain::{AudioBufferF32, AudioFormat};
    use atomusicator_infrastructure::{
        audio_io::{aligned::AlignedStream, wav_codec::write_wav_f32},
        audio_tools::TemporaryDirectory,
        live_pitch_control::LivePitchControl,
    };
    let root = TemporaryDirectory::new(&std::env::temp_dir())
        .unwrap_or_else(|error| panic!("temp root {:?}: {error}", std::env::temp_dir()));
    let samples: Vec<f32> = (0..1000)
        .flat_map(|i| [i as f32 / 1000.0, -(i as f32) / 1000.0])
        .collect();
    let paths: Vec<_> = (0..5)
        .map(|i| {
            let path = root.0.join(format!("{i}.wav"));
            write_wav_f32(
                &path,
                &AudioBufferF32::new(AudioFormat::new(48000, 2).unwrap(), samples.clone()).unwrap(),
            )
            .unwrap();
            path
        })
        .collect();
    let mut stream = AlignedStream::open_pitched(
        &paths,
        0.0,
        48000,
        7,
        std::sync::Arc::new(LivePitchControl::default()),
    )
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    for frame in 0..1000 {
        let output = loop {
            match stream.consumer.pop() {
                Ok(v) => break v,
                Err(_) => {
                    assert!(std::time::Instant::now() < deadline);
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
        };
        assert_eq!(output.generation, 7);
        for track in output.samples {
            assert_eq!(track, [samples[frame * 2], samples[frame * 2 + 1]]);
        }
        assert!((output.position_seconds - (frame + 1) as f64 / 48000.0).abs() < 1e-9);
    }
    assert!(stream.consumer.pop().is_err());
    drop(stream);
    let mut seeked = AlignedStream::open_pitched(
        &paths,
        37.0 / 48000.0,
        48000,
        8,
        std::sync::Arc::new(LivePitchControl::default()),
    )
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    for index in 37..1000 {
        let frame = loop {
            match seeked.consumer.pop() {
                Ok(v) => break v,
                Err(_) => {
                    assert!(std::time::Instant::now() < deadline);
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
        };
        assert_eq!(frame.generation, 8);
        assert_eq!(
            frame.samples[0],
            [samples[index * 2], samples[index * 2 + 1]]
        );
    }
}

#[test]
fn pitched_export_matches_the_streaming_engine_and_preserves_originals() {
    use atomusicator_domain::{AudioBufferF32, AudioFormat};
    use atomusicator_infrastructure::{
        audio_io::{
            aligned::AlignedStream,
            wav_codec::{read_wav_f32, write_wav_f32},
        },
        audio_tools::TemporaryDirectory,
        live_pitch::{PITCH_BLOCK, render_file},
        live_pitch_control::LivePitchControl,
    };
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let source = root.0.join("original.wav");
    let target = root.0.join("rendered.wav");
    let samples: Vec<f32> = (0..48000)
        .flat_map(|i| {
            let v = (i as f32 * std::f32::consts::TAU * 440.0 / 48000.0).sin() * 0.2;
            [v, v * 0.5]
        })
        .collect();
    write_wav_f32(
        &source,
        &AudioBufferF32::new(AudioFormat::new(48000, 2).unwrap(), samples.clone()).unwrap(),
    )
    .unwrap();
    let bytes = std::fs::read(&source).unwrap();
    let spec = PitchShiftSpec::try_new(7.0, true).unwrap();
    render_file(&source, &target, spec, &Default::default()).unwrap();
    let exported = read_wav_f32(&target).unwrap();
    assert_eq!(exported.frames(), 48000);
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    let control = std::sync::Arc::new(LivePitchControl::default());
    control.set(0, 1, spec).unwrap();
    let mut stream = AlignedStream::open_pitched(&vec![source; 5], 0.0, 48000, 1, control).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    for frame in 0..48000 {
        let live = loop {
            match stream.consumer.pop() {
                Ok(v) => break v,
                Err(_) => {
                    assert!(std::time::Instant::now() < deadline);
                    std::thread::yield_now();
                }
            }
        };
        if frame > PITCH_BLOCK {
            for ch in 0..2 {
                assert!((live.samples[0][ch] - exported.samples()[frame * 2 + ch]).abs() < 1e-6);
            }
        }
    }
}

#[test]
#[ignore = "opt-in release-build five-stem DSP timing, no physical output"]
fn five_stem_dsp_timing() {
    let rate = 48000;
    let mut processors = atomusicator_infrastructure::live_pitch::LivePitchBank::new(rate).unwrap();
    let mut timings = Vec::new();
    let input: Vec<f32> = (0..256)
        .flat_map(|i| {
            let v = (i as f32 * 0.0576).sin() * 0.2;
            [v, v * 0.5]
        })
        .collect();
    let mut output = [[0.0; 512]; 5];
    let input: [f32; 512] = input.try_into().unwrap();
    let inputs = [input; 5];
    for block in 0..2000 {
        let spec =
            PitchShiftSpec::try_new(if block % 200 < 100 { 7.0 } else { -7.0 }, true).unwrap();
        let start = std::time::Instant::now();
        processors.process(&inputs, &mut output, spec).unwrap();
        if block >= 100 {
            timings.push(start.elapsed().as_secs_f64() * 1000.0);
        }
    }
    timings.sort_by(f64::total_cmp);
    let p95 = timings[timings.len() * 95 / 100];
    let max = timings[timings.len() - 1];
    let mean = timings.iter().sum::<f64>() / timings.len() as f64;
    println!(
        "{{\"five_stereo_stems\":true,\"formants\":true,\"block_frames\":256,\"sample_rate\":48000,\"process_p95_ms\":{p95},\"process_max_ms\":{max},\"realtime_factor\":{},\"algorithm_delay_frames\":{},\"control_output_delay_frames\":{}}}",
        mean / (256.0 / 48.0),
        processors.latency_frames(),
        processors.output_delay_frames()
    );
    assert!(
        mean < 256.0 / 48.0 * 0.6,
        "five-stem processor lacks realtime CPU headroom"
    );
    assert!((processors.output_delay_frames() as f64 / 48.0) < 75.0);
}

#[test]
#[ignore = "opt-in paced software-output latency measurement; release build; no physical output"]
fn software_stream_control_response() {
    use atomusicator_domain::{AudioBufferF32, AudioFormat};
    use atomusicator_infrastructure::{
        audio_io::{aligned::AlignedStream, wav_codec::write_wav_f32},
        audio_tools::TemporaryDirectory,
        live_pitch_control::LivePitchControl,
    };
    use std::time::{Duration, Instant};
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let source = root.0.join("tone.wav");
    let samples: Vec<_> = (0..480000)
        .flat_map(|i| {
            let v = (std::f32::consts::TAU * 440.0 * i as f32 / 48000.0).sin() * 0.2;
            [v, v]
        })
        .collect();
    write_wav_f32(
        &source,
        &AudioBufferF32::new(AudioFormat::new(48000, 2).unwrap(), samples).unwrap(),
    )
    .unwrap();
    let control = std::sync::Arc::new(LivePitchControl::default());
    let mut stream =
        AlignedStream::open_pitched(&vec![source; 5], 0.0, 48000, 1, control.clone()).unwrap();
    let started = Instant::now();
    let mut changes = Vec::new();
    let mut waiting = None;
    let mut recent = std::collections::VecDeque::with_capacity(512);
    for block in 0..1500 {
        if block >= 100 && block % 100 == 0 {
            assert!(
                waiting.is_none(),
                "previous control update never became audible in software output"
            );
            let semitones = if block % 200 == 100 { 12 } else { 0 };
            control
                .set(
                    0,
                    (block / 100) as u64,
                    PitchShiftSpec::try_new(semitones as f32, true).unwrap(),
                )
                .unwrap();
            waiting = Some((Instant::now(), semitones));
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        for _ in 0..256 {
            let frame = loop {
                match stream.consumer.pop() {
                    Ok(v) => break v,
                    Err(_) => {
                        assert!(Instant::now() < deadline, "software sink underrun");
                        std::thread::yield_now();
                    }
                }
            };
            assert!(
                frame
                    .samples
                    .windows(2)
                    .all(|tracks| tracks[0] == tracks[1])
            );
            if recent.len() == 512 {
                recent.pop_front();
            }
            recent.push_back(frame.samples[0][0]);
        }
        if let Some((time, semitones)) = waiting
            && recent.len() == 512
        {
            let hz = frequency(recent.make_contiguous(), 48000);
            if (semitones == 12 && hz > 660.0 && hz < 1100.0)
                || (semitones == 0 && hz > 330.0 && hz < 660.0)
            {
                changes.push(time.elapsed().as_secs_f64() * 1000.0);
                waiting = None;
            }
        }
        let next = started + Duration::from_secs_f64((block + 1) as f64 * 256.0 / 48000.0);
        if let Some(wait) = next.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
    }
    changes.sort_by(f64::total_cmp);
    assert_eq!(changes.len(), 14);
    let p95 = changes[changes.len() * 95 / 100];
    let max = changes[changes.len() - 1];
    println!(
        "{{\"scope\":\"paced software output, includes detection window\",\"physical_output\":false,\"driver_latency_included\":false,\"changes\":{},\"p50_ms\":{},\"p95_ms\":{p95},\"max_ms\":{max}}}",
        changes.len(),
        changes[changes.len() / 2]
    );
    assert!(p95 <= 100.0, "software response misses the 100ms target");
    assert!(max <= 150.0);
}
