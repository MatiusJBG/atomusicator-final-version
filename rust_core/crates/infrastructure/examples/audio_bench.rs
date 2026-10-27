//! Local licensed-corpus runner. Invoke with a JSON manifest and an output directory.
use atomusicator_application::{
    CancellationToken, ports::SeparationProgressPort,
    use_cases::separate_five_stems::SeparateFiveStemsUseCase,
};
use atomusicator_domain::StemType;
use atomusicator_infrastructure::{
    audio_evidence::compare_audio, audio_io::wav_codec::write_wav_f32, audio_tools::AudioTools,
    mdx::MdxInference, models::manifest_manager::ManifestModelManager, onnx::OrtSessionPool,
    storage::paths::WorkspacePathPolicy, use_case_adapters::StftSpectralBleedGate,
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    source: PathBuf,
    sha256: String,
    license: String,
    attribution: String,
    #[serde(default)]
    references: BTreeMap<String, PathBuf>,
}
struct Progress;
impl SeparationProgressPort for Progress {
    fn report(&self, done: u8, total: u8, message: &str) {
        eprintln!("{done}/{total}: {message}");
    }
}
fn key(kind: StemType) -> &'static str {
    match kind {
        StemType::LeadVocal => "lead_vocal",
        StemType::BackingVocals => "backing_vocals",
        StemType::Drums => "drums",
        StemType::Bass => "bass",
        StemType::Instrumental => "instrumental",
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args_os().collect::<Vec<_>>();
    if args.len() != 3 {
        return Err("usage: audio_bench <corpus.json> <output-directory>".into());
    }
    let manifest_path = Path::new(&args[1]);
    let corpus: Corpus = serde_json::from_slice(&fs::read(manifest_path)?)?;
    if corpus.cases.is_empty() {
        return Err("corpus must contain cases".into());
    }
    let parent = manifest_path.parent().unwrap_or(Path::new("."));
    let output = PathBuf::from(&args[2]);
    fs::create_dir_all(&output)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let manager =
        ManifestModelManager::from_embedded_manifest(WorkspacePathPolicy::system_default()?)?;
    let mut models = BTreeMap::new();
    for profile in &manager.manifest().models {
        let paths = WorkspacePathPolicy::system_default()?.model_candidates(&profile.filename)?;
        let mut verified = None;
        for path in paths.iter().filter(|path| path.is_file()) {
            let actual = runtime.block_on(ManifestModelManager::verify_model_file(path))?;
            if actual.eq_ignore_ascii_case(&profile.sha256) {
                verified = Some((path.clone(), actual));
                break;
            }
        }
        let verified = verified.ok_or_else(|| {
            format!(
                "no verified cached model {}; benchmark never downloads",
                profile.filename
            )
        })?;
        models.insert(profile.profile_id.clone(), verified);
    }
    let model_hashes = models
        .iter()
        .map(|(id, (_, hash))| (id.clone(), hash.clone()))
        .collect::<BTreeMap<_, _>>();
    let pool = Arc::new(OrtSessionPool::new(2)?);
    let pipeline = SeparateFiveStemsUseCase::new(
        MdxInference::with_session_pool(models, None, Arc::clone(&pool))?,
        Progress,
        StftSpectralBleedGate::new(2048, 0.05, 0.25)?,
    );
    let tools = AudioTools::discover()?;
    let mut reports = Vec::new();
    for case in corpus.cases {
        if case.id.is_empty()
            || !case
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || case.license.trim().is_empty()
            || case.attribution.trim().is_empty()
        {
            return Err("case requires a safe ID, license and attribution".into());
        }
        let path = parent.join(&case.source);
        let actual = runtime.block_on(ManifestModelManager::verify_model_file(&path))?;
        if actual != case.sha256.to_ascii_lowercase() {
            return Err(format!("source hash mismatch: {}", case.id).into());
        }
        let cancel = CancellationToken::default();
        let decode_start = Instant::now();
        let audio = tools.decode_for_separation(&path, &cancel)?;
        let decode_seconds = decode_start.elapsed().as_secs_f64();
        let start = Instant::now();
        let result = pipeline.execute(&audio, &cancel)?;
        let inference_seconds = start.elapsed().as_secs_f64();
        let case_output = output.join(&case.id);
        fs::create_dir(&case_output)?;
        let mut comparisons = BTreeMap::new();
        for stem in result.stems() {
            write_wav_f32(
                &case_output.join(format!("{}.wav", key(stem.kind))),
                &stem.audio,
            )?;
            if let Some(reference) = case.references.get(key(stem.kind)) {
                let reference = tools.decode_for_separation(&parent.join(reference), &cancel)?;
                comparisons.insert(key(stem.kind), compare_audio(&reference, &stem.audio)?);
            }
        }
        reports.push(serde_json::json!({"case_id":case.id,"source_sha256":actual,"license":case.license,"attribution":case.attribution,"frames":audio.frames(),"sample_rate":audio.format().sample_rate_hz,"decode_seconds":decode_seconds,"inference_seconds":inference_seconds,"reference_metrics":comparisons}));
    }
    let report = serde_json::json!({"schema_version":1,"profile":"native-v1","cache":"disabled","onnx_runtime":ort::info(),"provider_effective":"CPUExecutionProvider","intra_threads":pool.intra_threads(),"inter_threads":1,"precision":"float32","model_hashes":model_hashes,"cases":reports});
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", output.join("report.json").display());
    Ok(())
}
