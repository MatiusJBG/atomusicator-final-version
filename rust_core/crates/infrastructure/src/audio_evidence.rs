//! Per-channel audio evidence. Undefined/perfect SDR are explicit, never NaN in JSON.
use atomusicator_domain::{AudioBufferF32, DomainError};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", content = "db", rename_all = "snake_case")]
pub enum Score {
    UndefinedReference,
    UndefinedEstimate,
    Perfect,
    Decibels(f64),
}

#[derive(Debug, Serialize)]
pub struct ChannelMetrics {
    pub reference_rms: f64,
    pub estimate_rms: f64,
    pub rms_error: f64,
    pub max_abs_error: f64,
    pub estimate_peak: f64,
    pub si_sdr: Score,
}
#[derive(Debug, Serialize)]
pub struct AudioComparison {
    pub frames: usize,
    pub sample_rate_hz: u32,
    pub channels: Vec<ChannelMetrics>,
}

pub fn compare_audio(
    reference: &AudioBufferF32,
    estimate: &AudioBufferF32,
) -> Result<AudioComparison, DomainError> {
    if reference.format() != estimate.format()
        || reference.frames() != estimate.frames()
        || reference.frames() == 0
    {
        return Err(DomainError::AudioAlignmentMismatch);
    }
    let channels = usize::from(reference.format().channels);
    let count = reference.frames() as f64;
    let mut metrics = Vec::with_capacity(channels);
    for channel in 0..channels {
        let pairs = || {
            reference
                .samples()
                .iter()
                .skip(channel)
                .step_by(channels)
                .zip(estimate.samples().iter().skip(channel).step_by(channels))
                .map(|(a, b)| (f64::from(*a), f64::from(*b)))
        };
        let (sum_a, sum_b) = pairs().fold((0.0, 0.0), |(a, b), (x, y)| (a + x, b + y));
        let mean_a = sum_a / count;
        let mean_b = sum_b / count;
        let mut ref_energy = 0.0;
        let mut est_energy = 0.0;
        let mut error_energy = 0.0;
        let mut max_error = 0.0f64;
        let mut peak = 0.0f64;
        let mut dot = 0.0;
        let mut centered_ref_energy = 0.0;
        for (a, b) in pairs() {
            ref_energy += a * a;
            est_energy += b * b;
            error_energy += (a - b) * (a - b);
            max_error = max_error.max((a - b).abs());
            peak = peak.max(b.abs());
            dot += (a - mean_a) * (b - mean_b);
            centered_ref_energy += (a - mean_a) * (a - mean_a);
        }
        let si_sdr = if centered_ref_energy <= f64::MIN_POSITIVE {
            Score::UndefinedReference
        } else {
            let scale = dot / centered_ref_energy;
            let target_energy = scale * scale * centered_ref_energy;
            let residual = pairs()
                .map(|(a, b)| {
                    let e = (b - mean_b) - scale * (a - mean_a);
                    e * e
                })
                .sum::<f64>();
            if residual <= f64::MIN_POSITIVE && target_energy <= f64::MIN_POSITIVE {
                Score::UndefinedEstimate
            } else if residual <= f64::EPSILON * target_energy.max(f64::MIN_POSITIVE) {
                Score::Perfect
            } else {
                Score::Decibels(
                    10.0 * (target_energy.max(f64::MIN_POSITIVE).log10() - residual.log10()),
                )
            }
        };
        metrics.push(ChannelMetrics {
            reference_rms: (ref_energy / count).sqrt(),
            estimate_rms: (est_energy / count).sqrt(),
            rms_error: (error_energy / count).sqrt(),
            max_abs_error: max_error,
            estimate_peak: peak,
            si_sdr,
        });
    }
    Ok(AudioComparison {
        frames: reference.frames(),
        sample_rate_hz: reference.format().sample_rate_hz,
        channels: metrics,
    })
}
