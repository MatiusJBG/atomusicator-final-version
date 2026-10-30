//! Explicit local qualification. Registration alone never establishes GPU use.
use atomusicator_infrastructure::{
    mdx::{MdxConfig, MdxTransform},
    models::manifest_manager::ManifestModelManager,
    onnx::{ExecutionProviderKind, OnnxSessionRunner, OrtSessionPool},
    provider_trace::ProviderAssignment,
    storage::paths::WorkspacePathPolicy,
};
use std::{path::PathBuf, sync::Arc, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: provider_qualify <local-report-directory>")?;
    std::fs::create_dir_all(&directory)?;
    let device_id = std::env::args()
        .nth(2)
        .map(|v| v.parse::<u32>())
        .transpose()?
        .unwrap_or(0);
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
        let session = dml.load_session(&model.local_path)?;
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
            "node_and_window_parity_pass":assignment.directml_executed() && max_pcm_error <= 1e-4,
            "input_shape":shape,"declared_input_type":format!("{:?}", session.lock().map_err(|_| "session poisoned")?.inputs),
            "scope":"single synthetic MDX window; full-track SI-SDR and VRAM qualification still required"}));
        drop(session);
        dml.trim_idle_and_resize(1)?;
        cpu.trim_idle_and_resize(1)?;
    }
    let report = serde_json::json!({"schema_version":1,"onnx_runtime":ort::info(),"models":results,
        "session_diagnostics":dml.diagnostics(),"production_accelerator_enabled":false});
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
    Ok(())
}
