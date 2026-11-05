use atomusicator_application::{
    CancellationToken,
    ports::{AudioBlockSource, AudioBlockWriter},
};
use atomusicator_domain::{AudioAssetMetadata, AudioFormat, DomainError};
use atomusicator_infrastructure::{
    mdx::MdxConfig, mdx_bounded::run_bounded_mdx_reusing, mdx_pipeline::run_overlapped_mdx,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

struct Source {
    samples: Vec<f32>,
    short: bool,
}
impl AudioBlockSource for Source {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn metadata(&self) -> AudioAssetMetadata {
        AudioAssetMetadata {
            format: AudioFormat::new(44100, 2).unwrap(),
            frame_count: self.samples.len() as u64 / 2,
        }
    }
    fn peak(&self) -> f32 {
        self.samples.iter().map(|v| v.abs()).fold(0.0, f32::max)
    }
    fn content_hash(&self) -> &str {
        "test-only-finite-source"
    }
    fn read_frames(&self, start: u64, out: &mut [f32]) -> Result<usize, DomainError> {
        let start = start as usize * 2;
        let count = out.len().min(self.samples.len().saturating_sub(start));
        out.fill(0.0);
        out[..count].copy_from_slice(&self.samples[start..start + count]);
        Ok((count / 2).saturating_sub(usize::from(self.short)))
    }
}
#[derive(Default)]
struct Sink {
    samples: Vec<f32>,
    fail: bool,
}
impl AudioBlockWriter for Sink {
    fn write(&mut self, values: &[f32]) -> Result<(), DomainError> {
        if self.fail {
            return Err(DomainError::DspFailure("expected writer failure".into()));
        }
        self.samples.extend_from_slice(values);
        Ok(())
    }
    fn finish(self: Box<Self>) -> Result<Arc<dyn AudioBlockSource>, DomainError> {
        Err(DomainError::DspFailure("test sink does not publish".into()))
    }
}
fn config(invert: bool) -> MdxConfig {
    MdxConfig {
        n_fft: 60,
        hop: 12,
        dim_f: 31,
        dim_t: 16,
        compensate: 1.043,
        invert,
    }
}
fn source(frames: usize) -> Source {
    Source {
        samples: (0..frames * 2)
            .map(|i| (i as f32 * 0.173).sin() * 0.8)
            .collect(),
        short: false,
    }
}
fn prediction(input: &mut [f32], output: &mut Vec<f32>) -> Result<(), DomainError> {
    output.clear();
    output.extend(input.iter().map(|v| v * 0.6));
    Ok(())
}

#[test]
fn ordered_pipeline_is_bit_identical_at_window_and_padding_boundaries() {
    for frames in [1, 31, 59, 60, 61, 117, 120, 121, 178, 4097] {
        for invert in [false, true] {
            let source = source(frames);
            let c = config(invert);
            let cancel = CancellationToken::default();
            let mut serial = Sink::default();
            let mut parallel = Sink::default();
            run_bounded_mdx_reusing(c, &source, &mut serial, &cancel, prediction, None).unwrap();
            run_overlapped_mdx(c, &source, &mut parallel, &cancel, prediction, None).unwrap();
            assert_eq!(
                parallel.samples, serial.samples,
                "frames={frames}, invert={invert}"
            );
        }
    }
}

// A timeout makes blocked channel shutdown fail the test instead of hanging CI.
fn completes(
    operation: impl FnOnce() -> Result<(), DomainError> + Send + 'static,
) -> Result<(), DomainError> {
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(operation());
    });
    receive
        .recv_timeout(Duration::from_secs(3))
        .expect("pipeline workers did not terminate")
}
#[test]
fn errors_and_cancellation_terminate_backpressured_workers() {
    for failure in [
        "source",
        "prediction",
        "writer",
        "nonfinite",
        "cancel",
        "panic",
    ] {
        let result = completes(move || {
            let mut source = source(4097);
            source.short = failure == "source";
            let mut sink = Sink {
                fail: failure == "writer",
                ..Sink::default()
            };
            let cancel = CancellationToken::default();
            let worker_cancel = cancel.clone();
            run_overlapped_mdx(
                config(false),
                &source,
                &mut sink,
                &cancel,
                move |input, out| {
                    if failure == "panic" {
                        panic!("expected worker panic")
                    }
                    if failure == "prediction" {
                        return Err(DomainError::DspFailure(
                            "expected prediction failure".into(),
                        ));
                    }
                    if failure == "cancel" {
                        worker_cancel.cancel();
                    }
                    prediction(input, out)?;
                    if failure == "nonfinite" {
                        out[0] = f32::NAN;
                    }
                    Ok(())
                },
                None,
            )
        });
        assert!(result.is_err(), "failure={failure}");
        if failure == "cancel" {
            assert!(matches!(result, Err(DomainError::SeparationCancelled)));
        }
    }
}

#[test]
fn silent_input_never_dispatches_an_inference_window() {
    let source = Source {
        samples: vec![0.0; 8194],
        short: false,
    };
    let mut sink = Sink::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let worker_calls = calls.clone();
    run_overlapped_mdx(
        config(false),
        &source,
        &mut sink,
        &CancellationToken::default(),
        move |input, out| {
            worker_calls.fetch_add(1, Ordering::Relaxed);
            prediction(input, out)
        },
        None,
    )
    .unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert_eq!(sink.samples, source.samples);
}

#[test]
fn panicking_observers_release_workers_on_every_stage() {
    for stage in [
        "input_pcm_windows",
        "tensor_input_spectral_units",
        "tensor_output_spectral_units",
        "post_istft_normalized_windows",
        "post_ola_restore_pre_comp_pcm",
        "restored_compensated_pcm",
    ] {
        let result = completes(move || {
            let source = source(4097);
            let mut sink = Sink::default();
            let observer = |name: &str, _: &[f32]| {
                assert_ne!(name, stage, "expected observer panic");
            };
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_overlapped_mdx(
                    config(false),
                    &source,
                    &mut sink,
                    &CancellationToken::default(),
                    prediction,
                    Some(&observer),
                )
            }));
            match outcome {
                Err(_) | Ok(Err(_)) => Ok(()),
                Ok(Ok(())) => panic!("observer stage was never reached: {stage}"),
            }
        });
        result.unwrap();
    }
}
