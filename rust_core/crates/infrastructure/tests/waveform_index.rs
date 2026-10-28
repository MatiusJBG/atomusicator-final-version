use atomusicator_domain::{AudioBufferF32, AudioFormat};
use atomusicator_infrastructure::{
    audio_io::wav_codec::write_wav_f32, audio_tools::TemporaryDirectory,
    waveform_index::WaveformIndex,
};

#[test]
fn every_level_and_range_retains_real_channel_extrema_and_the_partial_tail() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let frames = 70001;
    let samples: Vec<f32> = (0..frames)
        .flat_map(|i| {
            let value = if i == frames - 1 {
                1.5
            } else {
                (i as f32 * 0.031).sin() * 0.5
            };
            [value, -value]
        })
        .collect();
    let source = root.0.join("source.wav");
    write_wav_f32(
        &source,
        &AudioBufferF32::new(AudioFormat::new(44100, 2).unwrap(), samples.clone()).unwrap(),
    )
    .unwrap();
    let index = WaveformIndex::prepare(&source, &root.0).unwrap();
    for (start, end, limit) in [
        (0, frames, 1),
        (0, frames, 33),
        (543, 40000, 29),
        (frames - 1, frames, 8192),
    ] {
        let range = index.range(start as u64, end as u64, limit).unwrap();
        assert!(range.peaks.len() / 2 <= limit);
        assert!(range.start_frame <= start as u64);
        for (i, pair) in range.peaks.as_chunks::<2>().0.iter().enumerate() {
            let low = range.start_frame as usize + i * range.frames_per_bucket as usize;
            let high = (low + range.frames_per_bucket as usize).min(frames);
            let actual = &samples[low * 2..high * 2];
            assert_eq!(
                pair[0],
                actual.iter().copied().fold(f32::INFINITY, f32::min)
            );
            assert_eq!(
                pair[1],
                actual.iter().copied().fold(f32::NEG_INFINITY, f32::max)
            );
        }
    }
    assert!(index.range(1, 1, 100).is_err());
    assert!(index.range(0, frames as u64, 8193).is_err());
    drop(index);
    assert_eq!(
        std::fs::read_dir(&root.0).unwrap().count(),
        1,
        "all index levels are retired with their last owner"
    );
}

#[test]
fn a_truncated_level_fails_instead_of_returning_fabricated_peaks() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let source = root.0.join("source.wav");
    write_wav_f32(
        &source,
        &AudioBufferF32::new(AudioFormat::new(48000, 1).unwrap(), vec![0.2; 1000]).unwrap(),
    )
    .unwrap();
    let index = WaveformIndex::prepare(&source, &root.0).unwrap();
    let directory = std::fs::read_dir(&root.0)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.is_dir())
        .unwrap();
    std::fs::write(directory.join("0.peaks"), []).unwrap();
    assert!(index.range(0, 1000, 8192).is_err());
}
