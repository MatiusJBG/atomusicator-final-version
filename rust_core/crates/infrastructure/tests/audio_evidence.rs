use atomusicator_domain::{AudioBufferF32, AudioFormat};
use atomusicator_infrastructure::audio_evidence::signal_statistics;
use atomusicator_infrastructure::audio_evidence::{Score, compare_audio};

fn audio(values: &[f32]) -> AudioBufferF32 {
    AudioBufferF32::new(AudioFormat::new(44100, 1).unwrap(), values.to_vec()).unwrap()
}

#[test]
fn scale_invariance_does_not_hide_gain_error_in_the_report() {
    let report = compare_audio(
        &audio(&[1.0, -1.0, 0.5, -0.5]),
        &audio(&[0.5, -0.5, 0.25, -0.25]),
    )
    .unwrap();
    assert_eq!(report.channels[0].si_sdr, Score::Perfect);
    assert_eq!(report.channels[0].max_abs_error, 0.5);
    assert!(report.channels[0].rms_error > 0.0);
    match report.channels[0].sdr {
        Score::Decibels(db) => assert!((db - 6.020599913).abs() < 1e-8),
        ref other => panic!("unexpected SDR: {other:?}"),
    }
}
#[test]
fn silent_reference_has_no_sdr_and_reports_leakage() {
    let report = compare_audio(&audio(&[0.0; 4]), &audio(&[0.1, -0.1, 0.1, -0.1])).unwrap();
    assert_eq!(report.channels[0].si_sdr, Score::UndefinedReference);
    assert!(report.channels[0].estimate_rms > 0.09);
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("NaN"));
    assert!(!json.contains("Infinity"));
}
#[test]
fn a_silent_or_dc_estimate_is_never_a_perfect_separation() {
    for estimate in [[0.0; 4], [0.5; 4]] {
        let report = compare_audio(&audio(&[1.0, -1.0, 0.5, -0.5]), &audio(&estimate)).unwrap();
        assert_eq!(report.channels[0].si_sdr, Score::UndefinedEstimate);
        assert!(report.channels[0].rms_error > 0.0);
    }
}

#[test]
fn an_orthogonal_estimate_has_an_explicit_zero_projection_score() {
    let report = compare_audio(
        &audio(&[1.0, -1.0, 1.0, -1.0]),
        &audio(&[1.0, 1.0, -1.0, -1.0]),
    )
    .unwrap();
    assert_eq!(report.channels[0].si_sdr, Score::NoTargetProjection);
    assert!(!serde_json::to_string(&report).unwrap().contains("null"));
}
#[test]
fn comparisons_reject_misalignment_and_measure_channels_independently() {
    assert!(compare_audio(&audio(&[0.0; 2]), &audio(&[0.0; 3])).is_err());
    let format = AudioFormat::new(44100, 2).unwrap();
    let left = AudioBufferF32::new(format, vec![1.0, 0.0, -1.0, 0.0]).unwrap();
    let right = AudioBufferF32::new(format, vec![1.0, 0.2, -1.0, -0.2]).unwrap();
    let report = compare_audio(&left, &right).unwrap();
    assert_eq!(report.channels[0].max_abs_error, 0.0);
    assert_eq!(report.channels[1].si_sdr, Score::UndefinedReference);
}

#[test]
fn signal_statistics_separate_channel_levels_from_float_headroom() {
    let buffer = AudioBufferF32::new(
        AudioFormat::new(44100, 2).unwrap(),
        vec![1.5, 0.0, -1.5, 0.0],
    )
    .unwrap();
    let stats = signal_statistics(&buffer).unwrap();
    assert_eq!(stats[0].peak, 1.5);
    assert_eq!(stats[0].above_full_scale_samples, 2);
    assert_eq!(stats[0].rms, 1.5);
    assert_eq!(stats[0].max_adjacent_step, 3.0);
    assert_eq!(stats[1].rms, 0.0);
    assert_eq!(stats[1].above_full_scale_samples, 0);
}

#[test]
fn streaming_metrics_match_independent_memory_metrics_across_block_boundaries() {
    use atomusicator_application::{CancellationToken, ports::AudioBlockStore};
    use atomusicator_domain::AudioAssetMetadata;
    use atomusicator_infrastructure::{
        audio_evidence::compare_sources, audio_tools::TemporaryDirectory,
        storage::float_spool::FloatSpoolStore,
    };
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let store = FloatSpoolStore::new(root.0.clone());
    for silent in [false, true] {
        let format = AudioFormat::new(44100, 2).unwrap();
        let a: Vec<f32> = (0..65539 * 2)
            .map(|i| {
                if silent {
                    0.0
                } else {
                    (i as f32 * 0.037).sin() * 0.2
                }
            })
            .collect();
        let b: Vec<f32> = a
            .iter()
            .enumerate()
            .map(|(i, v)| v * 0.5 + (i as f32 * 0.061).sin() * 0.01)
            .collect();
        let mut sources = Vec::new();
        for samples in [&a, &b] {
            let mut w = store
                .create(
                    "metrics",
                    AudioAssetMetadata {
                        format,
                        frame_count: 65539,
                    },
                )
                .unwrap();
            w.write(samples).unwrap();
            sources.push(w.finish().unwrap());
        }
        let memory = compare_audio(
            &AudioBufferF32::new(format, a).unwrap(),
            &AudioBufferF32::new(format, b).unwrap(),
        )
        .unwrap();
        let streamed = compare_sources(
            sources[0].as_ref(),
            sources[1].as_ref(),
            &CancellationToken::default(),
        )
        .unwrap();
        for (a, b) in memory.channels.iter().zip(&streamed.channels) {
            assert!((a.rms_error - b.rms_error).abs() < 1e-12);
            for (a, b) in [(&a.si_sdr, &b.si_sdr), (&a.sdr, &b.sdr)] {
                match (a, b) {
                    (Score::Decibels(a), Score::Decibels(b)) => assert!((a - b).abs() < 1e-10),
                    _ => assert_eq!(a, b),
                }
            }
        }
    }
}
