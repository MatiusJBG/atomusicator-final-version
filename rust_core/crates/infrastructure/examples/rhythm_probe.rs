//! Isolate upstream subtraction from the rhythm models on a reference-aligned excerpt.
use atomusicator_application::{
    CancellationToken,
    ports::{AudioBlockSource, AudioBlockStore, BlockStemInferencePort, InferenceStage},
};
use atomusicator_domain::AudioFormat;
use atomusicator_infrastructure::{
    audio_evidence::compare_sources,
    inference_diagnostics::InferenceDiagnostics,
    mdx::MdxInference,
    models::manifest_manager::ManifestModelManager,
    onnx::OrtSessionPool,
    storage::{
        float_spool::{FloatSpool, FloatSpoolStore},
        paths::WorkspacePathPolicy,
    },
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
type Source = Arc<dyn AudioBlockSource>;
fn stage(
    root: &Path,
    key: &str,
    cancel: &CancellationToken,
) -> Result<Source, Box<dyn std::error::Error>> {
    let candidates: Vec<_> = std::fs::read_dir(root)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "json")
                && p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(&format!("{key}-"))
        })
        .collect();
    if candidates.len() != 1 {
        return Err(format!("expected one saved {key} stage").into());
    }
    Ok(Arc::new(FloatSpool::open(&candidates[0], cancel)?))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err(
            "usage: rhythm_probe <completed-benchmark-case-directory> <new-probe-directory>".into(),
        );
    }
    let work = PathBuf::from(&args[0]);
    let output = PathBuf::from(&args[1]);
    std::fs::create_dir(&output)?;
    let cancel = CancellationToken::default();
    let format = AudioFormat::new(44100, 2)?;
    let open = |name: &str| -> Result<Source, Box<dyn std::error::Error>> {
        Ok(Arc::new(FloatSpool::inspect_raw(
            work.join(format!("{name}.f32")),
            format,
            &cancel,
        )?))
    };
    let mix = open("decoded")?;
    let drums = open("reference-drums")?;
    let bass = open("reference-bass")?;
    let master = stage(&work.join("stages"), "instrumental-master", &cancel)?;
    let drum_free = stage(&work.join("stages"), "drum-free", &cancel)?;
    let frames = mix.metadata().frame_count.min(15 * 44100);
    let mut buffer = vec![0.0; frames as usize * 2];
    let mut chosen = 0;
    let mut energy = -1.0;
    for start in (0..=mix.metadata().frame_count - frames).step_by(frames as usize) {
        drums.read_frames(start, &mut buffer)?;
        let observed = buffer.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
        if observed > energy {
            energy = observed;
            chosen = start;
        }
    }
    let store = FloatSpoolStore::new(output.join("spool"));
    let segment = |name: &str, source: &Source| -> Result<Source, Box<dyn std::error::Error>> {
        let mut metadata = source.metadata();
        metadata.frame_count = frames;
        let mut writer = store.create(name, metadata)?;
        let mut block = vec![0.0; 65536 * 2];
        for offset in (0..frames).step_by(65536) {
            let count = (frames - offset).min(65536) as usize;
            if source.read_frames(chosen + offset, &mut block[..count * 2])? != count {
                return Err("truncated probe source".into());
            }
            writer.write(&block[..count * 2])?;
        }
        Ok(writer.finish()?)
    };
    let mix = segment("mix", &mix)?;
    let drums = segment("reference-drums", &drums)?;
    let bass = segment("reference-bass", &bass)?;
    let master = segment("master", &master)?;
    let drum_free = segment("drum-free", &drum_free)?;
    let manager =
        ManifestModelManager::from_embedded_manifest(WorkspacePathPolicy::system_default()?)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let mut models = BTreeMap::new();
    for profile in &manager.manifest().models {
        let model = runtime
            .block_on(manager.cached_profile(&profile.profile_id))?
            .ok_or("verified models required")?;
        models.insert(
            profile.profile_id.clone(),
            (model.local_path, model.sha256_hash),
        );
    }
    let pool = Arc::new(OrtSessionPool::with_cpu_threads(1, 2)?);
    let mut probes = BTreeMap::new();
    for (name, kind, reference, upstream) in [
        ("drums", InferenceStage::Drums, &drums, &master),
        ("bass", InferenceStage::Bass, &bass, &drum_free),
    ] {
        let mut results = BTreeMap::new();
        for (label, input) in [
            ("original_mix", &mix),
            ("pipeline_residual", upstream),
            ("isolated_reference", reference),
        ] {
            let levels = Arc::new(InferenceDiagnostics::default());
            let inference = MdxInference::with_session_pool(models.clone(), None, pool.clone())?
                .with_diagnostics(levels.clone());
            let mut writer = store.create(
                &format!("{name}-{}", label.replace('_', "-")),
                input.metadata(),
            )?;
            inference.infer_blocks(kind, input.as_ref(), writer.as_mut(), &cancel)?;
            let inferred = writer.finish()?;
            results.insert(label,serde_json::json!({"metrics":compare_sources(reference.as_ref(),inferred.as_ref(),&cancel)?,"input_vs_reference":compare_sources(reference.as_ref(),input.as_ref(),&cancel)?,"diagnostics":levels.report()}));
        }
        probes.insert(name, results);
    }
    let report = serde_json::json!({"schema_version":1,"selection":"15-second grid excerpt with maximum reference drum energy; all probes share the same offset; no temporal fitting","start_frame":chosen,"frames":frames,"start_seconds":chosen as f64/44100.0,"probes":probes,"session_diagnostics":pool.diagnostics()});
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", output.join("report.json").display());
    Ok(())
}
