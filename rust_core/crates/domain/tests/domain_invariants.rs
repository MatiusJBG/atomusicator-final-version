use atomusicator_domain::{
    AudioBufferF32, AudioFormat, DomainError, PitchShiftSpec, SeparationResult, Stem, StemType,
};

fn buffer(samples: &[f32]) -> AudioBufferF32 {
    AudioBufferF32::new(AudioFormat::new(48_000, 1).unwrap(), samples.to_vec()).unwrap()
}

#[test]
fn buffer_rejects_partial_interleaved_frames() {
    let format = AudioFormat::new(48_000, 2).unwrap();
    assert_eq!(
        AudioBufferF32::new(format, vec![0.0]),
        Err(DomainError::IncompleteInterleavedFrame)
    );
}

#[test]
fn buffer_rejects_non_finite_samples() {
    let format = AudioFormat::new(48_000, 1).unwrap();
    assert_eq!(
        AudioBufferF32::new(format, vec![f32::NAN]),
        Err(DomainError::NonFiniteSample)
    );
}

#[test]
fn audio_format_rejects_unsupported_sample_rates_and_channel_counts() {
    assert_eq!(
        AudioFormat::new(0, 1),
        Err(DomainError::InvalidSampleRate(0))
    );
    assert_eq!(
        AudioFormat::new(48_000, 3),
        Err(DomainError::InvalidChannelCount(3))
    );
}

#[test]
fn pitch_shift_spec_accepts_inclusive_bounds_and_rejects_invalid_values() {
    assert_eq!(
        PitchShiftSpec::try_new(-12.0, false).unwrap().semitones(),
        -12.0
    );
    assert_eq!(
        PitchShiftSpec::try_new(12.0, true).unwrap().semitones(),
        12.0
    );
    assert_eq!(
        PitchShiftSpec::try_new(12.01, false),
        Err(DomainError::PitchOutOfBounds(12.01))
    );
    assert!(PitchShiftSpec::try_new(f32::NAN, false).is_err());
}

#[test]
fn deserialization_preserves_audio_and_pitch_invariants() {
    assert!(
        serde_json::from_str::<AudioFormat>(r#"{"sample_rate_hz":48000,"channels":3}"#).is_err()
    );
    assert!(
        serde_json::from_str::<PitchShiftSpec>(r#"{"semitones":13.0,"preserve_formants":false}"#)
            .is_err()
    );
    assert!(
        serde_json::from_str::<AudioBufferF32>(
            r#"{"format":{"sample_rate_hz":48000,"channels":2},"samples":[0.1]}"#
        )
        .is_err()
    );

    let audio = AudioBufferF32::new(AudioFormat::new(48_000, 1).unwrap(), vec![0.1, -0.1]).unwrap();
    let encoded = serde_json::to_string(&audio).unwrap();
    assert_eq!(
        serde_json::from_str::<AudioBufferF32>(&encoded).unwrap(),
        audio
    );
}

#[test]
fn separation_result_requires_five_aligned_reconstructing_stems() {
    let original = buffer(&[1.0, -0.5]);
    let stems = StemType::ALL.map(|kind| Stem {
        kind,
        audio: buffer(match kind {
            StemType::LeadVocal => &[0.1, 0.0],
            StemType::BackingVocals => &[0.2, -0.1],
            StemType::Drums => &[0.3, -0.1],
            StemType::Bass => &[0.1, -0.1],
            StemType::Instrumental => &[0.3, -0.2],
        }),
    });

    let result = SeparationResult::try_new(&original, stems).unwrap();
    assert_eq!(result.stems().len(), 5);
}

#[test]
fn separation_result_rejects_a_failed_null_test() {
    let original = buffer(&[1.0]);
    let stems = StemType::ALL.map(|kind| Stem {
        kind,
        audio: buffer(&[0.0]),
    });
    assert!(matches!(
        SeparationResult::try_new(&original, stems),
        Err(DomainError::ReconstructionMismatch { .. })
    ));
}
