use atomusicator_domain::{AudioBufferF32, AudioFormat};
use atomusicator_infrastructure::{
    audio_io::{aligned::AlignedStream, wav_codec::write_wav_f32},
    audio_tools::TemporaryDirectory,
};
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
fn files(root: &std::path::Path, rate: u32, frames: usize) -> Vec<std::path::PathBuf> {
    (0..5)
        .map(|index| {
            let path = root.join(format!("{index}.wav"));
            write_wav_f32(
                &path,
                &AudioBufferF32::new(
                    AudioFormat::new(rate, 2).unwrap(),
                    vec![(index + 1) as f32 * 0.1; frames * 2],
                )
                .unwrap(),
            )
            .unwrap();
            path
        })
        .collect()
}
#[test]
fn one_clock_preserves_alignment_and_seek_generation_at_both_device_rates() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let paths = files(&root.0, 44100, 4410);
    for rate in [44100, 48000] {
        let mut stream = AlignedStream::open(&paths, 0.05, rate, 17, 256).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut count = 0;
        loop {
            match stream.consumer.pop() {
                Ok(frame) => {
                    count += 1;
                    assert_eq!(frame.generation, 17);
                    assert!(
                        (frame.position_seconds - (0.05 + count as f64 / f64::from(rate)).min(0.1))
                            .abs()
                            < 1e-9
                    );
                    for (index, sample) in frame.samples.iter().enumerate() {
                        assert!((sample[0] - (index + 1) as f32 * 0.1).abs() < 1e-5);
                        assert_eq!(sample[0], sample[1]);
                    }
                }
                Err(_) => {
                    if stream.producer.finished.load(Ordering::Acquire) {
                        break;
                    }
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
        assert_eq!(count, (f64::from(rate) * 0.05).ceil() as usize);
        assert!(!stream.producer.failed.load(Ordering::Acquire));
    }
}
#[test]
fn mismatched_stems_are_rejected_before_replacing_a_session() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let paths = files(&root.0, 44100, 100);
    write_wav_f32(
        &paths[4],
        &AudioBufferF32::new(AudioFormat::new(48000, 2).unwrap(), vec![0.0; 200]).unwrap(),
    )
    .unwrap();
    assert!(AlignedStream::open(&paths, 0.0, 48000, 0, 256).is_err());
}

#[test]
fn resampling_preserves_passband_and_rejects_out_of_band_energy() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    for (input_rate, output_rate, frequency, expected_rms) in [
        (44100, 48000, 1000.0, Some(0.5f64 / 2.0f64.sqrt())),
        (48000, 44100, 1000.0, Some(0.5f64 / 2.0f64.sqrt())),
        (48000, 22050, 18000.0, None),
    ] {
        let audio = AudioBufferF32::new(
            AudioFormat::new(input_rate, 1).unwrap(),
            (0..input_rate / 5)
                .map(|i| {
                    (f64::from(i) * frequency * std::f64::consts::TAU / f64::from(input_rate)).sin()
                        as f32
                        * 0.5
                })
                .collect(),
        )
        .unwrap();
        let paths: Vec<_> = (0..5)
            .map(|i| {
                let path = root.0.join(format!("tone-{i}.wav"));
                write_wav_f32(&path, &audio).unwrap();
                path
            })
            .collect();
        let mut stream = AlignedStream::open(&paths, 0.0, output_rate, 1, 512).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut values = Vec::new();
        loop {
            if let Ok(frame) = stream.consumer.pop() {
                assert!(frame.samples.iter().all(|s| *s == frame.samples[0]));
                assert_eq!(frame.samples[0][0], frame.samples[0][1]);
                values.push(f64::from(frame.samples[0][0]));
            } else if stream.producer.finished.load(Ordering::Acquire) {
                // finished may become visible before the queue has been drained.
                if stream.consumer.is_empty() {
                    break;
                }
            } else {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        let middle = &values[128..values.len() - 128];
        let rms = (middle.iter().map(|v| v * v).sum::<f64>() / middle.len() as f64).sqrt();
        if let Some(expected) = expected_rms {
            assert!((rms - expected).abs() < 0.005, "passband RMS {rms}");
        } else {
            assert!(rms < 0.0005, "alias rejection RMS {rms}");
        }
    }
}
