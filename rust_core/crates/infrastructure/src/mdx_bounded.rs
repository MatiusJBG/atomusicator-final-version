//! Duration-independent MDX overlap accumulator; window grid and f32 summation order are preserved.
use crate::job_diagnostics::StageTimer;
use crate::mdx::{MdxConfig, MdxFftWorkspace, MdxTransform};
use crate::mdx_overlap::{MdxOverlap, OverlapLayout, WindowPosition};
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
    predict: impl FnMut(&[f32]) -> Result<Vec<f32>, DomainError>,
) -> Result<(), DomainError> {
    run_bounded_mdx_observed(c, source, target, cancel, predict, None)
}

pub use crate::mdx_overlap::MdxObserver;
pub fn run_bounded_mdx_observed(
    c: MdxConfig,
    source: &dyn AudioBlockSource,
    target: &mut dyn AudioBlockWriter,
    cancel: &CancellationToken,
    mut predict: impl FnMut(&[f32]) -> Result<Vec<f32>, DomainError>,
    observe: Option<&MdxObserver<'_>>,
) -> Result<(), DomainError> {
    run_bounded_mdx_reusing(
        c,
        source,
        target,
        cancel,
        |input, output| {
            *output = predict(input)?;
            Ok(())
        },
        observe,
    )
}

/// Production path: caller and transform reuse all large per-window buffers.
pub fn run_bounded_mdx_reusing(
    c: MdxConfig,
    source: &dyn AudioBlockSource,
    target: &mut dyn AudioBlockWriter,
    cancel: &CancellationToken,
    predict: impl FnMut(&mut [f32], &mut Vec<f32>) -> Result<(), DomainError>,
    observe: Option<&MdxObserver<'_>>,
) -> Result<(), DomainError> {
    run_bounded_mdx_reusing_with_progress(c, source, target, cancel, predict, observe, None)
}

/// Production path with per-window progress reporting.
pub fn run_bounded_mdx_reusing_with_progress(
    c: MdxConfig,
    source: &dyn AudioBlockSource,
    target: &mut dyn AudioBlockWriter,
    cancel: &CancellationToken,
    mut predict: impl FnMut(&mut [f32], &mut Vec<f32>) -> Result<(), DomainError>,
    observe: Option<&MdxObserver<'_>>,
    on_window: Option<&dyn Fn(usize, usize)>,
) -> Result<(), DomainError> {
    let _total = StageTimer::new("mdx_total");
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
    let step = if cfg!(target_os = "android") {
        (0.80 * chunk as f64) as usize
    } else {
        (0.65 * chunk as f64) as usize
    };
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
        if let Some(cb) = on_window {
            cb(1, 1);
        }
        return Ok(());
    }
    let mut overlap = MdxOverlap::new(
        &transform,
        c,
        OverlapLayout {
            frames,
            padded,
            step,
            peak,
        },
        observe.is_some(),
    );
    let mut raw = vec![0.0f32; chunk * 2];
    let mut normalized = vec![0.0f32; chunk * 2];
    let mut spectrum = vec![0.0f32; 4 * c.dim_f * c.dim_t];
    let mut prediction = Vec::with_capacity(spectrum.len());
    let mut fft_workspace = MdxFftWorkspace::new(&transform);
    let total_windows = (0..padded).step_by(step).count();
    for (window_index, start) in (0..padded).step_by(step).enumerate() {
        check(cancel)?;
        let read_timer = StageTimer::new("read_and_normalize");
        let actual = (padded - start).min(chunk as u64) as usize;
        raw.fill(0.0);
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
        for (normalized, raw) in normalized.iter_mut().zip(&raw) {
            *normalized = raw / peak;
        }
        drop(read_timer);
        if let Some(observe) = observe {
            observe("input_pcm_windows", &raw);
        }
        {
            let _timer = StageTimer::new("fft_forward");
            transform.forward_into(&normalized, &mut spectrum, &mut fft_workspace)?;
        }
        for component in 0..4 {
            spectrum
                [component * c.dim_f * c.dim_t..(component * c.dim_f + 3.min(c.dim_f)) * c.dim_t]
                .fill(0.0);
        }
        if let Some(observe) = observe {
            observe("tensor_input_spectral_units", &spectrum);
        }
        let silent_window = if cfg!(target_os = "android") {
            let energy: f64 = raw.iter().map(|&s| (s as f64) * (s as f64)).sum();
            let rms = (energy / raw.len().max(1) as f64).sqrt();
            rms <= 0.0005623413
        } else {
            false
        };
        if silent_window {
            prediction.resize(spectrum.len(), 0.0);
            prediction.fill(0.0);
        } else {
            let _timer = StageTimer::new("onnx_run_including_session_lookup");
            predict(&mut spectrum, &mut prediction)?;
        }
        if let Some(observe) = observe {
            observe("tensor_output_spectral_units", &prediction);
        }
        check(cancel)?;
        overlap.emit(
            WindowPosition { start, actual },
            &raw,
            &prediction,
            target,
            observe,
            &mut fft_workspace,
        )?;
        if let Some(cb) = on_window {
            cb(window_index + 1, total_windows);
        }
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
