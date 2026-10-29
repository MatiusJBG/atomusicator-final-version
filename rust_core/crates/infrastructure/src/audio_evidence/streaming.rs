//! Fixed-memory evidence over disk sources. No temporal alignment or gain fitting
//! is applied to SDR; SI-SDR centers each channel and fits its scalar projection.
use super::{AudioComparison, ChannelMetrics, Score, energy_ratio};
use atomusicator_application::{CancellationToken, ports::AudioBlockSource};
use atomusicator_domain::DomainError;

fn pairs(
    reference: &dyn AudioBlockSource,
    estimate: &dyn AudioBlockSource,
    cancel: &CancellationToken,
    mut visit: impl FnMut(usize, f64, f64),
) -> Result<(), DomainError> {
    let metadata = reference.metadata();
    let channels = metadata.format.channels as usize;
    let mut a = vec![0.0; 65536 * channels];
    let mut b = vec![0.0; a.len()];
    for start in (0..metadata.frame_count).step_by(65536) {
        if cancel.is_cancelled() {
            return Err(DomainError::SeparationCancelled);
        }
        let count = (metadata.frame_count - start).min(65536) as usize;
        if reference.read_frames(start, &mut a[..count * channels])? != count
            || estimate.read_frames(start, &mut b[..count * channels])? != count
        {
            return Err(DomainError::AudioAlignmentMismatch);
        }
        for (i, (&a, &b)) in a[..count * channels].iter().zip(&b).enumerate() {
            if !a.is_finite() || !b.is_finite() {
                return Err(DomainError::NonFiniteSample);
            }
            visit(i % channels, f64::from(a), f64::from(b));
        }
    }
    Ok(())
}

pub fn compare_sources(
    reference: &dyn AudioBlockSource,
    estimate: &dyn AudioBlockSource,
    cancel: &CancellationToken,
) -> Result<AudioComparison, DomainError> {
    let metadata = reference.metadata();
    if metadata != estimate.metadata() || metadata.frame_count == 0 {
        return Err(DomainError::AudioAlignmentMismatch);
    }
    let channels = metadata.format.channels as usize;
    let n = metadata.frame_count as f64;
    let mut sums = vec![[0.0f64; 7]; channels];
    pairs(reference, estimate, cancel, |c, a, b| {
        let v = &mut sums[c];
        v[0] += a;
        v[1] += b;
        v[2] += a * a;
        v[3] += b * b;
        v[4] += (a - b) * (a - b);
        v[5] = v[5].max((a - b).abs());
        v[6] = v[6].max(b.abs());
    })?;
    let mut centered = vec![[0.0; 2]; channels];
    pairs(reference, estimate, cancel, |c, a, b| {
        let a = a - sums[c][0] / n;
        let b = b - sums[c][1] / n;
        centered[c][0] += a * a;
        centered[c][1] += a * b;
    })?;
    let scales: Vec<_> = centered
        .iter()
        .map(|v| {
            if v[0] > f64::MIN_POSITIVE {
                v[1] / v[0]
            } else {
                0.0
            }
        })
        .collect();
    let mut residuals = vec![0.0; channels];
    pairs(reference, estimate, cancel, |c, a, b| {
        let residual = (b - sums[c][1] / n) - scales[c] * (a - sums[c][0] / n);
        residuals[c] += residual * residual;
    })?;
    Ok(AudioComparison {
        frames: usize::try_from(metadata.frame_count)
            .map_err(|_| DomainError::AudioAlignmentMismatch)?,
        sample_rate_hz: metadata.format.sample_rate_hz,
        channels: (0..channels)
            .map(|c| {
                let v = sums[c];
                let target = scales[c] * scales[c] * centered[c][0];
                let si_sdr = if centered[c][0] <= f64::MIN_POSITIVE {
                    Score::UndefinedReference
                } else if target <= f64::MIN_POSITIVE && residuals[c] <= f64::MIN_POSITIVE {
                    Score::UndefinedEstimate
                } else if target <= f64::MIN_POSITIVE {
                    Score::NoTargetProjection
                } else {
                    energy_ratio(target, residuals[c])
                };
                ChannelMetrics {
                    reference_rms: (v[2] / n).sqrt(),
                    estimate_rms: (v[3] / n).sqrt(),
                    rms_error: (v[4] / n).sqrt(),
                    max_abs_error: v[5],
                    estimate_peak: v[6],
                    si_sdr,
                    sdr: energy_ratio(v[2], v[4]),
                }
            })
            .collect(),
    })
}
