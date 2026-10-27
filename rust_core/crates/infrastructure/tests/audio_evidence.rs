use atomusicator_domain::{AudioBufferF32, AudioFormat};
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
fn comparisons_reject_misalignment_and_measure_channels_independently() {
    assert!(compare_audio(&audio(&[0.0; 2]), &audio(&[0.0; 3])).is_err());
    let format = AudioFormat::new(44100, 2).unwrap();
    let left = AudioBufferF32::new(format, vec![1.0, 0.0, -1.0, 0.0]).unwrap();
    let right = AudioBufferF32::new(format, vec![1.0, 0.2, -1.0, -0.2]).unwrap();
    let report = compare_audio(&left, &right).unwrap();
    assert_eq!(report.channels[0].max_abs_error, 0.0);
    assert_eq!(report.channels[1].si_sdr, Score::UndefinedReference);
}
