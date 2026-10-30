//! Explicit local qualification. Registration alone never establishes GPU use.
use atomusicator_infrastructure::{
    mdx::{MdxConfig, MdxTransform},
    models::manifest_manager::ManifestModelManager,
    onnx::{ExecutionProviderKind, OnnxSessionRunner, OrtSessionPool},
    provider_trace::ProviderAssignment,
    storage::paths::WorkspacePathPolicy,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args_os().nth(1).map(PathBuf::from).ok_or(
        "usage: provider_qualify <empty-report-directory> [device-id] [--batch-scaling=1|2|4]",
    )?;
    if directory.exists() && std::fs::read_dir(&directory)?.next().is_some() {
        return Err(
            "qualification requires an empty report directory to prevent stale evidence".into(),
        );
    }
    std::fs::create_dir_all(&directory)?;
    let device_id = std::env::args()
        .nth(2)
        .map(|v| v.parse::<u32>())
        .transpose()?
        .unwrap_or(0);
    let batches = batch_sizes(std::env::args().nth(3).as_deref())?;
    let batch_scaling = !batches.is_empty();
    if std::env::args_os().len() > 4 {
        return Err("too many qualification arguments".into());
    }
    let manager =
        ManifestModelManager::from_embedded_manifest(WorkspacePathPolicy::system_default()?)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let cpu = Arc::new(OrtSessionPool::with_cpu_threads(
        1,
        6.min(std::thread::available_parallelism()?.get()),
    )?);
    let dml = Arc::new(
        OrtSessionPool::with_provider_order(
            1,
            vec![ExecutionProviderKind::DirectMl, ExecutionProviderKind::Cpu],
        )?
        .with_device_id(device_id)?
        .requiring_accelerator()
        .with_profiling_directory(directory.join("traces"))?,
    );
    let mut results = Vec::new();
    for profile in &manager.manifest().models {
        let model = runtime
            .block_on(manager.cached_profile(&profile.profile_id))?
            .ok_or("verified local models required; qualification never downloads")?;
        let config = MdxConfig::for_profile(&profile.profile_id)?;
        let transform = MdxTransform::new(config)?;
        let pcm: Vec<f32> = (0..config.chunk_frames())
            .flat_map(|i| {
                let sample = (i as f32 * 0.031).sin() * 0.25;
                [sample, sample * 0.7]
            })
            .collect();
        let spectrum = transform.forward(&pcm)?;
        let shape = [1, 4, config.dim_f as i64, config.dim_t as i64];
        let started = Instant::now();
        let reference = OnnxSessionRunner::new(cpu.clone()).run_f32(
            &model.local_path,
            "input",
            &shape,
            &spectrum,
        )?;
        let cpu_seconds = started.elapsed().as_secs_f64();
        let started = Instant::now();
        let candidate = OnnxSessionRunner::new(dml.clone()).run_f32(
            &model.local_path,
            "input",
            &shape,
            &spectrum,
        )?;
        let dml_seconds = started.elapsed().as_secs_f64();
        // Retain the profiled session so failed batch probes can still flush
        // their trace and save an error report without reloading quarantine.
        let session = dml.load_session(&model.local_path)?;
        let batch_results = if batch_scaling {
            measure_batches(
                &OnnxSessionRunner::new(dml.clone()),
                &model.local_path,
                BatchWindow {
                    spectrum: &spectrum,
                    shape,
                    transform: &transform,
                    reference_pcm: &transform.inverse(&reference)?,
                    device_id,
                    directory: &directory,
                    batches: &batches,
                },
            )?
        } else {
            Vec::new()
        };
        let trace = session
            .lock()
            .map_err(|_| "session poisoned")?
            .end_profiling()?;
        let assignment = ProviderAssignment::from_file(std::path::Path::new(&trace))?;
        let expected = transform.inverse(&reference)?;
        let actual = transform.inverse(&candidate)?;
        if expected.len() != actual.len() {
            return Err("PCM length mismatch".into());
        }
        let max_pcm_error = expected
            .iter()
            .zip(&actual)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        results.push(serde_json::json!({"profile_id":profile.profile_id,"model_sha256":profile.sha256,
            "cold_cpu_seconds":cpu_seconds,"cold_dml_seconds":dml_seconds,
            "assignment":assignment,"max_normalized_window_pcm_error":max_pcm_error,
            "batch_scaling":batch_results,
            "node_and_window_parity_pass":assignment.directml_executed() && max_pcm_error <= 1e-4,
            "input_shape":shape,"declared_input_type":format!("{:?}", session.lock().map_err(|_| "session poisoned")?.inputs),
            "scope":"single synthetic MDX window; full-track SI-SDR and VRAM qualification still required"}));
        drop(session);
        dml.trim_idle_and_resize(1)?;
        cpu.trim_idle_and_resize(1)?;
    }
    let report = serde_json::json!({"schema_version":1,"onnx_runtime":ort::info(),"models":results,
        "session_diagnostics":dml.diagnostics(),"production_accelerator_enabled":false,
        "production_batching_enabled":false,
        "batch_sizes_requested":batches,
        "batch_measurement_scope":"Optional ONNX runner throughput for synthetic windows, including tensor copies/session lookup/profiling; two warmup rounds then five timed rounds of four windows. Excludes FFT, decode, queueing, publication and full-pipeline RTF. CPU fallback is refused."});
    std::fs::write(
        directory.join("provider-qualification.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", report);
    if !results
        .iter()
        .all(|model| model["node_and_window_parity_pass"] == true)
    {
        return Err(
            "DirectML node execution or window parity failed; inspect the saved report".into(),
        );
    }
    if batch_scaling
        && results.iter().any(|model| {
            model["batch_scaling"].as_array().is_none_or(|rows| {
                rows.len() != batches.len()
                    || rows.iter().any(|row| row["window_parity_pass"] != true)
            })
        })
    {
        return Err(
            "one or more batch probes failed; results saved without enabling production batching"
                .into(),
        );
    }
    Ok(())
}

fn timing_median(values: &[f64]) -> Result<f64, &'static str> {
    if values.is_empty() || values.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return Err("finite nonnegative timing observations required");
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    Ok(if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    })
}

fn batch_sizes(value: Option<&str>) -> Result<Vec<usize>, &'static str> {
    match value {
        None => Ok(Vec::new()),
        Some("--batch-scaling=1") => Ok(vec![1]),
        Some("--batch-scaling=2") => Ok(vec![1, 2]),
        Some("--batch-scaling=4") => Ok(vec![1, 2, 4]),
        _ => Err("batch probes require an explicit maximum: --batch-scaling=1, =2 or =4"),
    }
}

struct BatchWindow<'a> {
    spectrum: &'a [f32],
    shape: [i64; 4],
    transform: &'a MdxTransform,
    reference_pcm: &'a [f32],
    device_id: u32,
    directory: &'a Path,
    batches: &'a [usize],
}
fn measure_batches(
    runner: &OnnxSessionRunner,
    model: &Path,
    window: BatchWindow<'_>,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let BatchWindow {
        spectrum,
        shape: single_shape,
        transform,
        reference_pcm,
        device_id,
        directory,
        batches,
    } = window;
    let identity = model
        .file_stem()
        .ok_or("model filename missing")?
        .to_string_lossy();
    let checkpoint = directory.join(format!("batch-observations-{identity}.json"));
    let progress = directory.join(format!("batch-progress-{identity}.json"));
    let mut rows = Vec::new();
    let mut baseline = None;
    for &batch in batches {
        let observation = (|| -> Result<serde_json::Value, Box<dyn std::error::Error>> {
            let mut shape = single_shape;
            shape[0] = batch as i64;
            let input = spectrum.repeat(batch);
            let mut last_output = Vec::new();
            let mut timings = Vec::with_capacity(5);
            for trial in 0..7 {
                std::fs::write(
                    &progress,
                    serde_json::to_vec_pretty(&serde_json::json!({
                        "model":model,"batch":batch,"trial":trial,"phase":"before_four_window_round",
                        "completed_batch_observations":rows.len(),"input_shape":shape
                    }))?,
                )?;
                let started = Instant::now();
                for _ in 0..4 / batch {
                    last_output = runner.run_f32(model, "input", &shape, &input)?;
                }
                if trial >= 2 {
                    timings.push(started.elapsed().as_secs_f64());
                }
            }
            if last_output.len() != input.len() {
                return Err("batch output shape mismatch".into());
            }
            let mut max_pcm_error = 0.0f32;
            for output in last_output.chunks_exact(spectrum.len()) {
                let actual = transform.inverse(output)?;
                if actual.len() != reference_pcm.len() {
                    return Err("batch PCM length mismatch".into());
                }
                for (actual, expected) in actual.iter().zip(reference_pcm) {
                    if !actual.is_finite() {
                        return Err("nonfinite batch output".into());
                    }
                    max_pcm_error = max_pcm_error.max((actual - expected).abs());
                }
            }
            let median = timing_median(&timings)?;
            if batch == 1 {
                baseline = Some(median);
            }
            let gain = baseline.filter(|v| *v > 0.0).map(|v| 1.0 - median / v);
            Ok(
                serde_json::json!({"batch":batch,"windows_per_round":4,"warmup_rounds":2,
                "timed_round_seconds":timings,"median_seconds_for_four_windows":median,
                "fractional_runner_time_reduction_vs_batch1":gain,
                "max_normalized_window_pcm_error":max_pcm_error,"window_parity_pass":max_pcm_error<=1e-4,
                "gpu_memory_after_trials":atomusicator_infrastructure::gpu_probe::adapter(device_id)?,
                "full_pipeline_rtf_qualification_required":true}),
            )
        })();
        rows.push(match observation {
            Ok(value) => value,
            Err(error) => serde_json::json!({"batch":batch,"window_parity_pass":false,"error":error.to_string(),"full_pipeline_rtf_qualification_required":true}),
        });
        std::fs::write(&checkpoint, serde_json::to_vec_pretty(&rows)?)?;
    }
    Ok(rows)
}

#[cfg(test)]
mod batch_tests {
    #[test]
    fn batch_probe_limits_are_explicit_and_always_include_a_baseline() {
        assert_eq!(super::batch_sizes(None).unwrap(), Vec::<usize>::new());
        assert_eq!(super::batch_sizes(Some("--batch-scaling=1")).unwrap(), [1]);
        assert_eq!(
            super::batch_sizes(Some("--batch-scaling=2")).unwrap(),
            [1, 2]
        );
        assert_eq!(
            super::batch_sizes(Some("--batch-scaling=4")).unwrap(),
            [1, 2, 4]
        );
        assert!(super::batch_sizes(Some("--batch-scaling=3")).is_err());
        assert!(super::batch_sizes(Some("typo")).is_err());
    }
    #[test]
    fn timing_median_rejects_incomplete_or_nonfinite_observations() {
        assert_eq!(super::timing_median(&[3.0, 1.0, 2.0]).unwrap(), 2.0);
        assert!(super::timing_median(&[]).is_err());
        assert!(super::timing_median(&[f64::NAN]).is_err());
        assert!(super::timing_median(&[-1.0]).is_err());
    }
}
