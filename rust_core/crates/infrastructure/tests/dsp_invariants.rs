use atomusicator_application::ports::PhaseDspPort;
use atomusicator_application::ports::WaveformCachePort;
use atomusicator_domain::{AudioBufferF32, AudioFormat};
use atomusicator_infrastructure::dsp::{PhaseSubtractor, SpectralBleedGate, StftProcessor};
use atomusicator_infrastructure::use_case_adapters::NativePhaseDsp;
use atomusicator_infrastructure::waveform::WaveformPeakCache;

#[test]
fn stft_round_trip_reconstructs_interleaved_audio() {
    let format = AudioFormat::new(48_000, 2).unwrap();
    let samples: Vec<f32> = (0..512)
        .map(|index| ((index as f32) * 0.13).sin() * 0.7)
        .collect();
    let original = AudioBufferF32::new(format, samples).unwrap();
    let processor = StftProcessor::new(64).unwrap();
    let reconstructed = processor
        .inverse(&processor.forward(&original).unwrap())
        .unwrap();
    for (expected, actual) in original.samples().iter().zip(reconstructed.samples()) {
        assert!((expected - actual).abs() < 1.0e-4, "{expected} != {actual}");
    }
}

#[test]
fn phase_subtractor_preserves_exact_sample_residuals() {
    let format = AudioFormat::new(48_000, 1).unwrap();
    let mix = AudioBufferF32::new(format, vec![0.9, -0.7]).unwrap();
    let stem = AudioBufferF32::new(format, vec![0.2, 0.3]).unwrap();
    let residual = PhaseSubtractor::subtract(&mix, &stem).unwrap();
    assert_eq!(residual.samples(), &[0.7, -1.0]);
}

#[test]
fn bleed_gate_rebalances_without_changing_the_complex_sum() {
    let processor = StftProcessor::new(64).unwrap();
    let format = AudioFormat::new(48_000, 1).unwrap();
    let lead = AudioBufferF32::new(format, vec![0.25; 256]).unwrap();
    let backing = AudioBufferF32::new(format, vec![0.01; 256]).unwrap();
    let mut lead_spectrum = processor.forward(&lead).unwrap();
    let mut backing_spectrum = processor.forward(&backing).unwrap();
    let before: Vec<_> = lead_spectrum
        .bins
        .iter()
        .zip(&backing_spectrum.bins)
        .map(|(lead, backing)| {
            lead.iter()
                .zip(backing)
                .map(|(lead, backing)| *lead + *backing)
                .collect::<Vec<_>>()
        })
        .collect();
    SpectralBleedGate::rebalance(&mut lead_spectrum, &mut backing_spectrum, 0.1, 1.0).unwrap();
    for ((lead, backing), before) in lead_spectrum
        .bins
        .iter()
        .zip(&backing_spectrum.bins)
        .zip(before)
    {
        for ((lead, backing), before) in lead.iter().zip(backing).zip(before) {
            assert!(((*lead + *backing) - before).norm() < 1.0e-5);
        }
    }
}

#[test]
fn waveform_cache_returns_channel_merged_minimum_and_maximum_peaks() {
    let format = AudioFormat::new(48_000, 2).unwrap();
    let audio = AudioBufferF32::new(format, vec![0.1, -0.2, 0.4, -0.5, -0.3, 0.2]).unwrap();
    let cache = WaveformPeakCache::new(4).unwrap();

    assert_eq!(cache.peaks(&audio, 2), vec![(-0.5, 0.4), (-0.3, 0.2)]);
    assert!(cache.peaks(&audio, 0).is_empty());
}

#[test]
fn native_phase_dsp_adapter_subtracts_and_rebalances_compatible_audio() {
    let dsp = NativePhaseDsp::new(64, 0.1, 1.0).unwrap();
    let format = AudioFormat::new(48_000, 1).unwrap();
    let mix = AudioBufferF32::new(format, vec![0.7; 256]).unwrap();
    let stem = AudioBufferF32::new(format, vec![0.2; 256]).unwrap();

    let residual = dsp.subtract_phase(&mix, &stem).unwrap();
    assert!(
        residual
            .samples()
            .iter()
            .all(|sample| (*sample - 0.5).abs() < 1.0e-6)
    );

    let (lead, backing) = dsp.apply_spectral_bleed_gate(&stem, &residual).unwrap();
    for ((lead, backing), original) in lead
        .samples()
        .iter()
        .zip(backing.samples())
        .zip(mix.samples())
    {
        assert!((lead + backing - original).abs() < 1.0e-4);
    }
}
