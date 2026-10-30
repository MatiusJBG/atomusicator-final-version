//! Experimental three-stage bounded MDX scheduling.
//!
//! Reconstruction stays ordered on the caller. This API is not selected by the
//! application: real-model parity, memory admission and end-to-end timing must
//! qualify the additional window buffers and two workers before activation.
use crate::{
    job_diagnostics::StageTimer,
    mdx::{MdxConfig, MdxFftWorkspace, MdxTransform},
    mdx_bounded::{MdxObserver, run_bounded_mdx_reusing},
    mdx_overlap::{MdxOverlap, OverlapLayout, WindowPosition},
};
use atomusicator_application::{
    CancellationToken,
    ports::{AudioBlockSource, AudioBlockWriter},
};
use atomusicator_domain::DomainError;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{RecvTimeoutError, sync_channel},
};
use std::time::Duration;

pub type ParallelMdxObserver<'a> = dyn Fn(&str, &[f32]) + Sync + 'a;
const WINDOW_SLOTS: usize = 3;
const CHANNEL_CAPACITY: usize = 1;

struct Window {
    position: WindowPosition,
    raw: Vec<f32>,
    spectrum: Vec<f32>,
    prediction: Vec<f32>,
}
impl Window {
    fn new(c: MdxConfig) -> Self {
        let tensor = 4 * c.dim_f * c.dim_t;
        Self {
            position: WindowPosition {
                start: 0,
                actual: 0,
            },
            raw: vec![0.0; c.chunk_frames() * 2],
            spectrum: vec![0.0; tensor],
            prediction: Vec::with_capacity(tensor),
        }
    }
}
struct StopOnDrop(Arc<AtomicBool>);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
fn check(cancel: &CancellationToken) -> Result<(), DomainError> {
    if cancel.is_cancelled() {
        Err(DomainError::SeparationCancelled)
    } else {
        Ok(())
    }
}
fn fail(message: impl Into<String>) -> DomainError {
    DomainError::DspFailure(message.into())
}

/// At most three windows exist, including windows currently being processed.
/// Read/FFT and inference workers never hold the output writer. All workers join
/// before returning, including when a source, predictor, writer or observer fails.
/// Observers run concurrently across stages and must synchronize shared state.
/// Cancellation is cooperative between source reads, transforms and predictions;
/// a blocking external predictor must supply its own deadline.
pub fn run_overlapped_mdx(
    c: MdxConfig,
    source: &dyn AudioBlockSource,
    target: &mut dyn AudioBlockWriter,
    cancel: &CancellationToken,
    mut predict: impl FnMut(&mut [f32], &mut Vec<f32>) -> Result<(), DomainError> + Send,
    observe: Option<&ParallelMdxObserver<'_>>,
) -> Result<(), DomainError> {
    let _total = StageTimer::new("mdx_overlapped_total");
    let metadata = source.metadata();
    let peak = source.peak();
    if metadata.format.sample_rate_hz != 44100
        || metadata.format.channels != 2
        || metadata.frame_count == 0
        || !peak.is_finite()
        || peak < 0.0
    {
        return Err(fail("MDX requires nonempty finite 44100 Hz stereo audio"));
    }
    let transform = Arc::new(MdxTransform::new(c)?);
    check(cancel)?;
    if peak == 0.0 {
        return run_bounded_mdx_reusing(
            c,
            source,
            target,
            cancel,
            predict,
            observe.map(|observer| observer as &MdxObserver<'_>),
        );
    }
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
    let layout = OverlapLayout {
        frames,
        padded,
        step,
        peak,
    };
    let (recycle, available) = sync_channel(WINDOW_SLOTS);
    for _ in 0..WINDOW_SLOTS {
        recycle
            .send(Window::new(c))
            .map_err(|_| fail("MDX window pool initialization failed"))?;
    }
    let (prepared, input) = sync_channel::<Window>(CHANNEL_CAPACITY);
    let (predicted, output) = sync_channel::<Window>(CHANNEL_CAPACITY);
    let stop = Arc::new(AtomicBool::new(false));
    let dispatcher = tracing::dispatcher::get_default(Clone::clone);
    let parent_span = tracing::Span::current();
    tracing::info!(
        window_slots = WINDOW_SLOTS,
        channel_capacity = CHANNEL_CAPACITY,
        "bounded MDX pipeline started"
    );

    std::thread::scope(move |scope| {
        // The closure owns output/recycle. Unwinding drops them before scope
        // joins workers, so a panicking consumer cannot leave senders blocked.
        let _shutdown = StopOnDrop(stop.clone());
        let read_stop = stop.clone();
        let read_transform = transform.clone();
        let read_dispatcher = dispatcher.clone();
        let read_span = parent_span.clone();
        let producing = std::thread::Builder::new()
            .name("atomusicator-mdx-read-fft".into())
            .spawn_scoped(scope, move || -> Result<(), DomainError> {
                let _dispatch = tracing::dispatcher::set_default(&read_dispatcher);
                let _span = read_span.enter();
                let mut normalized = vec![0.0; chunk * 2];
                let mut workspace = MdxFftWorkspace::new(&read_transform);
                for start in (0..padded).step_by(step) {
                    check(cancel)?;
                    let mut window = loop {
                        if read_stop.load(Ordering::Acquire) {
                            return Ok(());
                        }
                        check(cancel)?;
                        match available.recv_timeout(Duration::from_millis(20)) {
                            Ok(window) => break window,
                            Err(RecvTimeoutError::Timeout) => continue,
                            Err(RecvTimeoutError::Disconnected) => return Ok(()),
                        }
                    };
                    let actual = (padded - start).min(chunk as u64) as usize;
                    window.position = WindowPosition { start, actual };
                    {
                        let _timer = StageTimer::new("read_and_normalize");
                        window.raw.fill(0.0);
                        let first = start.max(trim as u64);
                        let end = (start + actual as u64).min(trim as u64 + frames);
                        if first < end {
                            let offset = (first - start) as usize;
                            let count = (end - first) as usize;
                            if source.read_frames(
                                first - trim as u64,
                                &mut window.raw[offset * 2..(offset + count) * 2],
                            )? != count
                            {
                                return Err(fail(
                                    "MDX source ended before its declared frame count",
                                ));
                            }
                        }
                        for (normalized, raw) in normalized.iter_mut().zip(&window.raw) {
                            *normalized = raw / peak;
                        }
                    }
                    if let Some(observe) = observe {
                        observe("input_pcm_windows", &window.raw);
                    }
                    {
                        let _timer = StageTimer::new("fft_forward");
                        read_transform.forward_into(
                            &normalized,
                            &mut window.spectrum,
                            &mut workspace,
                        )?;
                    }
                    for component in 0..4 {
                        window.spectrum[component * c.dim_f * c.dim_t
                            ..(component * c.dim_f + 3.min(c.dim_f)) * c.dim_t]
                            .fill(0.0);
                    }
                    if let Some(observe) = observe {
                        observe("tensor_input_spectral_units", &window.spectrum);
                    }
                    if prepared.send(window).is_err() {
                        return Ok(());
                    }
                }
                Ok(())
            })
            .map_err(|error| fail(format!("could not start MDX preparation worker: {error}")))?;

        let inference_stop = stop.clone();
        let inferring = std::thread::Builder::new()
            .name("atomusicator-mdx-inference".into())
            .spawn_scoped(scope, move || -> Result<(), DomainError> {
                let _dispatch = tracing::dispatcher::set_default(&dispatcher);
                let _span = parent_span.enter();
                while let Ok(mut window) = input.recv() {
                    if inference_stop.load(Ordering::Acquire) {
                        return Ok(());
                    }
                    check(cancel)?;
                    {
                        let _timer = StageTimer::new("onnx_run_including_session_lookup");
                        predict(&mut window.spectrum, &mut window.prediction)?;
                    }
                    if let Some(observe) = observe {
                        observe("tensor_output_spectral_units", &window.prediction);
                    }
                    if predicted.send(window).is_err() {
                        return Ok(());
                    }
                }
                Ok(())
            })
            .map_err(|error| fail(format!("could not start MDX inference worker: {error}")))?;

        let consumed = (|| -> Result<(), DomainError> {
            let mut overlap = MdxOverlap::new(&transform, c, layout, observe.is_some());
            let mut workspace = MdxFftWorkspace::new(&transform);
            while let Ok(window) = output.recv() {
                check(cancel)?;
                overlap.emit(
                    window.position,
                    &window.raw,
                    &window.prediction,
                    target,
                    observe.map(|observer| observer as &MdxObserver<'_>),
                    &mut workspace,
                )?;
                // The reader may already have finished its final grid window.
                let _ = recycle.send(window);
            }
            check(cancel)
        })();
        stop.store(true, Ordering::Release);
        drop(output);
        drop(recycle);
        // Collect both joins before propagating any error.
        let produced = producing
            .join()
            .map_err(|_| fail("MDX preparation worker panicked"));
        let inferred = inferring
            .join()
            .map_err(|_| fail("MDX inference worker panicked"));
        consumed?;
        produced??;
        inferred??;
        Ok(())
    })
}
