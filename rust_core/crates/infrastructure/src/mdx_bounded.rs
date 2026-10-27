//! Duration-independent MDX overlap accumulator; window grid and f32 summation order are preserved.
use crate::mdx::{MdxConfig, MdxTransform};
use atomusicator_application::{
    CancellationToken,
    ports::{AudioBlockSource, AudioBlockWriter},
};
use atomusicator_domain::DomainError;

pub fn run_bounded_mdx(
    c: MdxConfig,
    source: &dyn AudioBlockSource,
    target: &mut dyn AudioBlockWriter,
    cancel: &CancellationToken,
    mut predict: impl FnMut(&[f32]) -> Result<Vec<f32>, DomainError>,
) -> Result<(), DomainError> {
    let metadata = source.metadata();
    if metadata.format.sample_rate_hz != 44100
        || metadata.format.channels != 2
        || metadata.frame_count == 0
        || !source.peak().is_finite()
        || source.peak() < 0.0
    {
        return Err(fail("MDX requires nonempty finite 44100 Hz stereo audio"));
    }
    let transform = MdxTransform::new(c)?;
    let chunk = c.chunk_frames();
    let trim = c.n_fft / 2;
    let generation = chunk - 2 * trim;
    let frames = metadata.frame_count;
    let pad = (generation + trim) as u64 - frames % generation as u64;
    let padded = (trim as u64)
        .checked_add(frames)
        .and_then(|v| v.checked_add(pad))
        .ok_or_else(|| fail("MDX frame count overflow"))?;
    let step = (0.65 * chunk as f64) as usize;
    if step == 0 {
        return Err(fail("invalid MDX step"));
    }
    let peak = source.peak();
    if peak == 0.0 {
        let zeros = vec![0.0; 65536 * 2];
        let mut remaining = frames;
        while remaining > 0 {
            check(cancel)?;
            let count = remaining.min(65536) as usize;
            target.write(&zeros[..count * 2])?;
            remaining -= count as u64;
        }
        return Ok(());
    }
    let mut accumulator = vec![0.0f32; chunk * 2];
    let mut weights = vec![0.0f32; chunk];
    for start in (0..padded).step_by(step) {
        check(cancel)?;
        let actual = (padded - start).min(chunk as u64) as usize;
        let mut raw = vec![0.0f32; chunk * 2];
        let first = start.max(trim as u64);
        let end = (start + actual as u64).min(trim as u64 + frames);
        if first < end {
            let offset = (first - start) as usize;
            let count = (end - first) as usize;
            if source.read_frames(
                first - trim as u64,
                &mut raw[offset * 2..(offset + count) * 2],
            )? != count
            {
                return Err(fail("MDX source ended before its declared frame count"));
            }
        }
        let normalized: Vec<f32> = raw.iter().map(|v| v / peak).collect();
        let mut spectrum = transform.forward(&normalized)?;
        for component in 0..4 {
            spectrum
                [component * c.dim_f * c.dim_t..(component * c.dim_f + 3.min(c.dim_f)) * c.dim_t]
                .fill(0.0);
        }
        let prediction = predict(&spectrum)?;
        check(cancel)?;
        let decoded = transform.inverse(&prediction)?;
        for frame in 0..actual {
            let weight = if actual > 1 {
                0.5 - 0.5 * (2.0 * std::f32::consts::PI * frame as f32 / (actual - 1) as f32).cos()
            } else {
                1.0
            };
            weights[frame] += weight;
            for channel in 0..2 {
                accumulator[frame * 2 + channel] += decoded[frame * 2 + channel] * weight;
            }
        }
        // No later window can contribute before the next grid position.
        let flush = step.min((padded - start) as usize);
        let mut output = Vec::with_capacity(flush * 2);
        for frame in 0..flush {
            let position = start + frame as u64;
            if position < trim as u64 || position - trim as u64 >= frames {
                continue;
            }
            if weights[frame] <= 0.0 {
                return Err(fail("MDX overlap left an uncovered frame"));
            }
            for channel in 0..2 {
                let value = accumulator[frame * 2 + channel] / weights[frame] * peak * c.compensate;
                output.push(if c.invert {
                    raw[frame * 2 + channel] - value
                } else {
                    value
                });
            }
        }
        target.write(&output)?;
        accumulator.copy_within(flush * 2.., 0);
        accumulator[(chunk - flush) * 2..].fill(0.0);
        weights.copy_within(flush.., 0);
        weights[chunk - flush..].fill(0.0);
    }
    check(cancel)
}
fn check(cancel: &CancellationToken) -> Result<(), DomainError> {
    if cancel.is_cancelled() {
        Err(DomainError::SeparationCancelled)
    } else {
        Ok(())
    }
}
fn fail(message: &str) -> DomainError {
    DomainError::DspFailure(message.into())
}
