use atomusicator_domain::{
    AudioBufferF32, AudioFormat, DomainError, SeparationResult, Stem, StemType,
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
