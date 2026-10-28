//! Controlled CPU-only bounded pipeline measurements. Never downloads models or promotes a profile.
use atomusicator_application::{
    CancellationToken, ports::SeparationProgressPort,
    use_cases::bounded_separation::BoundedSeparation,
};
use atomusicator_domain::{AudioAsset, AudioAssetId};
use atomusicator_infrastructure::{
    audio_tools::AudioTools,
    block_workspace::{PublicationHooks, publish_blocks},
    mdx::MdxInference,
    models::manifest_manager::ManifestModelManager,
    onnx::OrtSessionPool,
    storage::{float_spool::FloatSpoolStore, paths::WorkspacePathPolicy},
    use_case_adapters::StftSpectralBleedGate,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
struct Progress;
impl SeparationProgressPort for Progress {
    fn report(&self, done: u8, total: u8, message: &str) {
        eprintln!("{done}/{total}: {message}");
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err(
            "usage: cpu_thread_bench <local-audio> <new-output-directory> <1..6-threads>".into(),
        );
    }
    let source = Path::new(&args[0]).canonicalize()?;
    let output = PathBuf::from(&args[1]);
    fs::create_dir(&output)?;
    let pool = Arc::new(OrtSessionPool::with_cpu_threads(2, args[2].parse()?)?);
    let policy = WorkspacePathPolicy::system_default()?;
    let manager = ManifestModelManager::from_embedded_manifest(policy.clone())?;
    let runtime = tokio::runtime::Runtime::new()?;
    let cancel = CancellationToken::default();
    let mut models = BTreeMap::new();
    for profile in &manager.manifest().models {
        let path = policy
            .model_candidates(&profile.filename)?
            .into_iter()
            .find(|p| p.is_file())
            .ok_or("verified models required; this tool never downloads")?;
        let hash = runtime.block_on(ManifestModelManager::verify_model_file(&path))?;
        if hash != profile.sha256 {
            return Err("model hash mismatch".into());
        }
        models.insert(profile.profile_id.clone(), (path, hash));
    }
    let model_hashes: BTreeMap<_, _> = models.iter().map(|(id, (_, hash))| (id, hash)).collect();
    let source_hash =
        atomusicator_infrastructure::job_journal::JobJournal::hash_source(&source, &cancel)?;
    let decode = Instant::now();
    let decoded = Arc::new(AudioTools::discover()?.decode_to_spool(
        &source,
        &output.join("decoded.f32"),
        &cancel,
    )?);
    let decode_seconds = decode.elapsed().as_secs_f64();
    let asset = AudioAsset {
        id: AudioAssetId::new("thread-benchmark")?,
        path: source,
        display_name: "local CPU benchmark".into(),
    };
    let mut passes = Vec::new();
    for pass in 0..2 {
        // Each pass has fresh stage storage, so this never times a checkpoint cache hit.
        let pipeline = BoundedSeparation::new(
            MdxInference::with_session_pool(models.clone(), None, Arc::clone(&pool))?,
            Progress,
            StftSpectralBleedGate::new(2048, 0.05, 0.25)?,
            FloatSpoolStore::new(output.join(format!("cache-{pass}"))),
        );
        let started = Instant::now();
        let stems = pipeline.execute(decoded.clone(), &cancel)?;
        let inference_seconds = started.elapsed().as_secs_f64();
        let published = publish_blocks(
            &output,
            &format!("pass-{pass}"),
            &asset,
            &stems,
            serde_json::json!({"source_sha256":source_hash,"model_hashes":model_hashes,"profile":atomusicator_domain::quality::SeparationProfile::compatibility(),"intra_threads":pool.intra_threads(),"provider":"CPUExecutionProvider"}),
            &cancel,
            PublicationHooks {
                staged: |_: &Path| Ok(()),
                commit: || Ok(()),
            },
        )?;
        passes.push(serde_json::json!({"pass":pass,"inference_seconds":inference_seconds,"inference_and_publication_seconds":started.elapsed().as_secs_f64(),"session_directory":published.directory}));
    }
    println!(
        "{}",
        serde_json::json!({"schema_version":1,"intra_threads":pool.intra_threads(),"inter_threads":1,"execution":"serial","stage_cache":"fresh each pass","onnx_runtime":ort::info(),"source_sha256":source_hash,"model_hashes":model_hashes,"decode_seconds":decode_seconds,"passes":passes})
    );
    Ok(())
}
