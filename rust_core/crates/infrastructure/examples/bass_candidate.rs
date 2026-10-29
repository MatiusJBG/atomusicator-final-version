//! Full-track counterfactual: replay validated unchanged baseline stages and
//! infer bass from the original mix. This is not an end-to-end timing benchmark.
use atomusicator_application::{
    CancellationToken,
    ports::{AudioBlockSource, AudioBlockStore, BlockStemInferencePort, InferenceStage},
};
use atomusicator_domain::AudioFormat;
use atomusicator_infrastructure::{
    audio_evidence::compare_sources,
    audio_io::wav_codec::write_block_wav_hashed,
    mdx::MdxInference,
    models::manifest_manager::ManifestModelManager,
    onnx::OrtSessionPool,
    storage::{
        float_spool::{FloatSpool, FloatSpoolStore},
        paths::WorkspacePathPolicy,
    },
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
type Source = Arc<dyn AudioBlockSource>;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err(
            "usage: bass_candidate <completed-baseline-case-directory> <new-candidate-directory>"
                .into(),
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
    let keys = [
        "lead_vocal",
        "backing_vocals",
        "drums",
        "bass",
        "instrumental",
    ];
    let mut baseline = BTreeMap::new();
    let mut references = BTreeMap::new();
    for key in keys {
        baseline.insert(key, open(&format!("persisted-{key}"))?);
        references.insert(key, open(&format!("reference-{key}"))?);
    }
    let manager =
        ManifestModelManager::from_embedded_manifest(WorkspacePathPolicy::system_default()?)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let model = runtime
        .block_on(manager.cached_profile("kuielab-mdxnet-bass-v1"))?
        .ok_or("verified bass model required")?;
    let models = BTreeMap::from([(
        "kuielab-mdxnet-bass-v1".into(),
        (model.local_path, model.sha256_hash),
    )]);
    let inference = MdxInference::with_session_pool(
        models,
        None,
        Arc::new(OrtSessionPool::with_cpu_threads(1, 2)?),
    )?;
    let store = FloatSpoolStore::new(output.join("spool"));
    let mut writer = store.create("bass", mix.metadata())?;
    inference.infer_blocks(InferenceStage::Bass, mix.as_ref(), writer.as_mut(), &cancel)?;
    let bass = writer.finish()?;
    let paths: Vec<_> = std::fs::read_dir(work.join("stages"))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "json")
                && p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("instrumental-master-")
        })
        .collect();
    if paths.len() != 1 {
        return Err("one validated baseline master required".into());
    }
    let master = FloatSpool::open(&paths[0], &cancel)?;
    let metadata = mix.metadata();
    let channels = metadata.format.channels as usize;
    let mut writer = store.create("instrumental", metadata)?;
    let mut a = vec![0.0; 65536 * channels];
    let mut d = vec![0.0; a.len()];
    let mut b = vec![0.0; a.len()];
    for start in (0..metadata.frame_count).step_by(65536) {
        let n = (metadata.frame_count - start).min(65536) as usize * channels;
        master.read_frames(start, &mut a[..n])?;
        baseline["drums"].read_frames(start, &mut d[..n])?;
        bass.read_frames(start, &mut b[..n])?;
        for i in 0..n {
            a[i] -= d[i] + b[i];
        }
        writer.write(&a[..n])?;
    }
    let other = writer.finish()?;
    let mut candidate = baseline.clone();
    candidate.insert("bass", bass);
    candidate.insert("instrumental", other);
    let mut metrics = BTreeMap::new();
    for key in keys {
        let reference = references[key].as_ref();
        metrics.insert(key,serde_json::json!({"baseline":compare_sources(reference,baseline[key].as_ref(),&cancel)?,"candidate":compare_sources(reference,candidate[key].as_ref(),&cancel)?}));
    }
    let mut hashes = BTreeMap::new();
    for key in ["bass", "instrumental"] {
        hashes.insert(
            key,
            write_block_wav_hashed(
                &output.join(format!("{key}.wav")),
                candidate[key].as_ref(),
                &cancel,
            )?,
        );
    }
    let mut writer = store.create("sum", metadata)?;
    for start in (0..metadata.frame_count).step_by(65536) {
        let n = (metadata.frame_count - start).min(65536) as usize * channels;
        a[..n].fill(0.0);
        for key in keys {
            candidate[key].read_frames(start, &mut b[..n])?;
            for i in 0..n {
                a[i] += b[i];
            }
        }
        writer.write(&a[..n])?;
    }
    let sum = writer.finish()?;
    let reconstruction = compare_sources(mix.as_ref(), sum.as_ref(), &cancel)?;
    let report = serde_json::json!({"schema_version":1,"profile":"bass-original-mix-candidate-v1","scope":"full-track quality counterfactual; unchanged validated baseline stems replayed; not fresh end-to-end timing or release qualification","frames":metadata.frame_count,"metrics":metrics,"modified_wav_hashes":hashes,"reconstruction":reconstruction});
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", output.join("report.json").display());
    Ok(())
}
