//! Per-model CPU measurements: explicit cold initialization, then resident warm runs.
//! No stage cache, downloads, quality claims, or full-song allocation.
use atomusicator_infrastructure::{
    mdx::{MdxConfig, MdxTransform},
    models::manifest_manager::ManifestModelManager,
    onnx::{OnnxSessionRunner, OrtSessionPool},
    storage::paths::WorkspacePathPolicy,
};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 1 {
        return Err("usage: onnx_bench <intra-threads>".into());
    }
    let pool = Arc::new(OrtSessionPool::with_cpu_threads(1, args[0].parse()?)?);
    let runner = OnnxSessionRunner::new(pool.clone());
    let manager =
        ManifestModelManager::from_embedded_manifest(WorkspacePathPolicy::system_default()?)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let mut models = Vec::new();
    for profile in &manager.manifest().models {
        let verified = runtime
            .block_on(manager.cached_profile(&profile.profile_id))?
            .ok_or("verified local model required; benchmark never downloads")?;
        let config = MdxConfig::for_profile(&profile.profile_id)?;
        let transform = MdxTransform::new(config)?;
        let pcm: Vec<f32> = (0..config.chunk_frames())
            .flat_map(|i| {
                let sample = (i as f32 * 0.031).sin() * 0.25;
                [sample, sample * 0.7]
            })
            .collect();
        let input = transform.forward(&pcm)?;
        pool.trim_idle_and_resize(1)?;
        let started = Instant::now();
        let session = pool.load_session(&verified.local_path)?;
        let initialization_seconds = started.elapsed().as_secs_f64();
        // Hold the lease throughout; every timed call uses this resident session.
        let mut times = Vec::new();
        let mut expected = None;
        for _ in 0..5 {
            let started = Instant::now();
            let output = runner.run_f32(
                &verified.local_path,
                "input",
                &[1, 4, config.dim_f as i64, config.dim_t as i64],
                &input,
            )?;
            times.push(started.elapsed().as_secs_f64());
            let mut hash = Sha256::new();
            for value in output {
                hash.update(value.to_le_bytes());
            }
            let hash = format!("{:x}", hash.finalize());
            if expected.as_ref().is_some_and(|value| value != &hash) {
                return Err("resident CPU outputs are not bit identical".into());
            }
            expected = Some(hash);
        }
        let mut sorted = times.clone();
        sorted.sort_by(f64::total_cmp);
        models.push(
            serde_json::json!({"profile_id":profile.profile_id,"filename":profile.filename,
            "model_bytes":std::fs::metadata(&verified.local_path)?.len(),"sha256":profile.sha256,
            "cold_initialization_seconds":initialization_seconds,"warm_runs_seconds":times,
            "warm_median_seconds":sorted[2],"warm_p95_seconds":sorted[4],
            "warm_range_seconds":sorted[4]-sorted[0],"output_sha256":expected}),
        );
        drop(session);
    }
    println!(
        "{}",
        serde_json::json!({"schema_version":1,"scope":"per-model synthetic spectral tensor; not musical or full-pipeline qualification",
        "app_version":env!("CARGO_PKG_VERSION"),"onnx_runtime":ort::info(),"intra_threads":pool.intra_threads(),"inter_threads":1,
        "session_diagnostics":pool.diagnostics(),"models":models})
    );
    Ok(())
}
