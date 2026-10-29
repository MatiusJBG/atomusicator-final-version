//! Fresh-cache bounded separation and streaming reference metrics.
use atomusicator_application::{
    CancellationToken,
    ports::{
        AudioBlockSource, AudioBlockStore, AudioBlockWriter, SeparationProgressPort,
        SpectralBleedGatePort,
    },
    use_cases::bounded_separation::{BoundedSeparation, RhythmPolicy},
};
use atomusicator_domain::{AudioAssetMetadata, AudioBufferF32, DomainError, StemType};
use atomusicator_infrastructure::{
    audio_evidence::compare_sources,
    audio_io::wav_codec::write_block_wav_hashed,
    audio_tools::AudioTools,
    inference_diagnostics::InferenceDiagnostics,
    mdx::MdxInference,
    models::manifest_manager::ManifestModelManager,
    onnx::{ExecutionProviderKind, OrtSessionPool},
    storage::{float_spool::FloatSpoolStore, paths::WorkspacePathPolicy},
    use_case_adapters::StftSpectralBleedGate,
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};
type Source = Arc<dyn AudioBlockSource>;
#[derive(Deserialize)]
struct Corpus {
    #[serde(default)]
    profile: Option<String>,
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
    fn report(&self, d: u8, t: u8, m: &str) {
        eprintln!("{d}/{t}: {m}");
    }
}
fn key(k: StemType) -> &'static str {
    match k {
        StemType::LeadVocal => "lead_vocal",
        StemType::BackingVocals => "backing_vocals",
        StemType::Drums => "drums",
        StemType::Bass => "bass",
        StemType::Instrumental => "instrumental",
    }
}
fn alias(k: &str) -> &str {
    match k {
        "lead_vocal" => "vocals",
        "backing_vocals" => "secondary",
        "instrumental" => "other",
        k => k,
    }
}
struct ObservedStore {
    store: FloatSpoolStore,
    levels: Arc<InferenceDiagnostics>,
    artifacts: Arc<Mutex<BTreeMap<String, Source>>>,
}
struct ObservedWriter {
    inner: Box<dyn AudioBlockWriter>,
    key: String,
    levels: Arc<InferenceDiagnostics>,
    artifacts: Arc<Mutex<BTreeMap<String, Source>>>,
}
impl AudioBlockStore for ObservedStore {
    fn create(
        &self,
        key: &str,
        metadata: AudioAssetMetadata,
    ) -> Result<Box<dyn AudioBlockWriter>, DomainError> {
        Ok(Box::new(ObservedWriter {
            inner: self.store.create(key, metadata)?,
            key: key.into(),
            levels: self.levels.clone(),
            artifacts: self.artifacts.clone(),
        }))
    }
    // No-cache defaults ensure that all model stages execute.
}
impl AudioBlockWriter for ObservedWriter {
    fn write(&mut self, samples: &[f32]) -> Result<(), DomainError> {
        self.inner.write(samples)?;
        self.levels
            .record(&format!("stage_pcm/{}", self.key), samples);
        Ok(())
    }
    fn finish(self: Box<Self>) -> Result<Source, DomainError> {
        let source = self.inner.finish()?;
        self.artifacts
            .lock()
            .map_err(|_| DomainError::WorkspaceFailure("diagnostic lock poisoned".into()))?
            .insert(self.key, source.clone());
        Ok(source)
    }
}
struct ObservedGate {
    inner: StftSpectralBleedGate,
    levels: Arc<InferenceDiagnostics>,
}
impl SpectralBleedGatePort for ObservedGate {
    fn rebalance(
        &self,
        a: &AudioBufferF32,
        b: &AudioBufferF32,
    ) -> Result<(AudioBufferF32, AudioBufferF32), DomainError> {
        self.levels.record("gate_before/lead", a.samples());
        self.levels.record("gate_before/backing", b.samples());
        let out = self.inner.rebalance(a, b)?;
        self.levels.record("gate_after/lead", out.0.samples());
        self.levels.record("gate_after/backing", out.1.samples());
        Ok(out)
    }
}
fn quality_profile(
    value: Option<&str>,
) -> Result<atomusicator_domain::quality::SeparationProfile, Box<dyn std::error::Error>> {
    use atomusicator_domain::quality::SeparationProfile;
    match value {
        None | Some("native-v1") => Ok(SeparationProfile::compatibility()),
        Some("native-v2-original-mix-bass") => Ok(SeparationProfile::production()),
        _ => Err("unsupported benchmark quality profile".into()),
    }
}
fn benchmark_pool(
    provider: &str,
    threads: usize,
) -> Result<OrtSessionPool, Box<dyn std::error::Error>> {
    let order = match provider {
        "cpu" => vec![ExecutionProviderKind::Cpu],
        "directml" => vec![ExecutionProviderKind::DirectMl, ExecutionProviderKind::Cpu],
        _ => return Err("provider must be cpu or directml".into()),
    };
    Ok(OrtSessionPool::with_cpu_threads(1, threads)?.using_provider_order(order)?)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if !(3..=5).contains(&args.len()) {
        return Err("usage: audio_bench <corpus.json> <new-output-directory> [cpu|directml] [intra-threads]".into());
    }
    let path = Path::new(&args[1]);
    let corpus: Corpus = serde_json::from_slice(&fs::read(path)?)?;
    let quality = quality_profile(corpus.profile.as_deref())?;
    if corpus.cases.is_empty() {
        return Err("bounded benchmark requires cases".into());
    }
    let parent = path.parent().unwrap_or(Path::new("."));
    let output = PathBuf::from(&args[2]);
    fs::create_dir(&output)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let manager =
        ManifestModelManager::from_embedded_manifest(WorkspacePathPolicy::system_default()?)?;
    let mut models = BTreeMap::new();
    let mut inventory = Vec::new();
    for profile in &manager.manifest().models {
        let model = runtime
            .block_on(manager.cached_profile(&profile.profile_id))?
            .ok_or("verified cached models required; no downloads")?;
        inventory.push(serde_json::json!({"profile_id":profile.profile_id,"filename":profile.filename,"sha256":profile.sha256,"bytes":fs::metadata(&model.local_path)?.len(),"transform":format!("{:?}",atomusicator_infrastructure::mdx::MdxConfig::for_profile(&profile.profile_id)?)}));
        models.insert(
            profile.profile_id.clone(),
            (model.local_path, model.sha256_hash),
        );
    }
    let tools = AudioTools::discover()?;
    let provider = args.get(3).and_then(|v| v.to_str()).unwrap_or("cpu");
    let threads = args
        .get(4)
        .map(|v| {
            v.to_str()
                .ok_or("invalid thread argument")?
                .parse::<usize>()
                .map_err(|_| "invalid thread argument")
        })
        .transpose()?
        .unwrap_or(2);
    let pool = Arc::new(benchmark_pool(provider, threads)?);
    let mut reports = Vec::new();
    for case in corpus.cases {
        if case.id.is_empty()
            || !case
                .id
                .bytes()
                .all(|v| v.is_ascii_alphanumeric() || v == b'-' || v == b'_')
            || case.license.trim().is_empty()
            || case.attribution.trim().is_empty()
        {
            return Err("safe case ID, license and attribution required".into());
        }
        let source = parent.join(&case.source);
        let cancel = CancellationToken::default();
        let hash = runtime.block_on(ManifestModelManager::verify_model_file(&source))?;
        if hash != case.sha256.to_ascii_lowercase() {
            return Err("input hash mismatch".into());
        }
        let work = output.join(&case.id);
        fs::create_dir(&work)?;
        let started = Instant::now();
        let mix: Source =
            Arc::new(tools.decode_to_spool(&source, &work.join("decoded.f32"), &cancel)?);
        let decode_seconds = started.elapsed().as_secs_f64();
        let levels = Arc::new(InferenceDiagnostics::default());
        let artifacts = Arc::new(Mutex::new(BTreeMap::new()));
        let pipeline = BoundedSeparation::new(
            MdxInference::with_session_pool(models.clone(), None, pool.clone())?
                .with_diagnostics(levels.clone()),
            Progress,
            ObservedGate {
                inner: StftSpectralBleedGate::new(2048, 0.05, 0.25)?,
                levels: levels.clone(),
            },
            ObservedStore {
                store: FloatSpoolStore::new(work.join("stages")),
                levels: levels.clone(),
                artifacts: artifacts.clone(),
            },
        )
        .with_rhythm_policy(RhythmPolicy::from_profile(&quality)?);
        let infer = Instant::now();
        let stems = pipeline.execute(mix.clone(), &cancel)?;
        let inference_seconds = infer.elapsed().as_secs_f64();
        let render = work.join("outputs");
        fs::create_dir(&render)?;
        let mut hashes = BTreeMap::new();
        for (kind, stem) in &stems {
            hashes.insert(
                key(*kind),
                write_block_wav_hashed(
                    &render.join(format!("{}.wav", key(*kind))),
                    stem.as_ref(),
                    &cancel,
                )?,
            );
        }
        let processing_seconds = started.elapsed().as_secs_f64();
        let mut metrics = BTreeMap::new();
        let mut references = BTreeMap::new();
        let mut roundtrip = BTreeMap::new();
        for kind in StemType::ALL {
            let k = key(kind);
            let persisted: Source = Arc::new(tools.decode_to_spool(
                &render.join(format!("{k}.wav")),
                &work.join(format!("persisted-{k}.f32")),
                &cancel,
            )?);
            roundtrip.insert(
                k,
                compare_sources(stems[&kind].as_ref(), persisted.as_ref(), &cancel)?,
            );
            if let Some(path) = case
                .references
                .get(k)
                .or_else(|| case.references.get(alias(k)))
            {
                let path = parent.join(path);
                let reference: Source = Arc::new(tools.decode_to_spool(
                    &path,
                    &work.join(format!("reference-{k}.f32")),
                    &cancel,
                )?);
                metrics.insert(k,serde_json::json!({"engine_stem":alias(k),"reference_sha256":runtime.block_on(ManifestModelManager::verify_model_file(&path))?,"metrics":compare_sources(reference.as_ref(),persisted.as_ref(),&cancel)?}));
                references.insert(k, reference);
            }
        }
        let metadata = mix.metadata();
        let channels = metadata.format.channels as usize;
        let store = FloatSpoolStore::new(work.join("evidence"));
        let mut summed = store.create("reconstruction", metadata)?;
        let mut buffer = vec![0.0; 65536 * channels];
        let mut sum = vec![0.0; buffer.len()];
        for start in (0..metadata.frame_count).step_by(65536) {
            let count = (metadata.frame_count - start).min(65536) as usize;
            sum[..count * channels].fill(0.0);
            for stem in stems.values() {
                stem.read_frames(start, &mut buffer[..count * channels])?;
                for (total, v) in sum[..count * channels].iter_mut().zip(&buffer) {
                    *total += v;
                }
            }
            summed.write(&sum[..count * channels])?;
        }
        let summed = summed.finish()?;
        let reconstruction = compare_sources(mix.as_ref(), summed.as_ref(), &cancel)?;
        let intermediates = artifacts.lock().unwrap().clone();
        let mut rhythm = BTreeMap::new();
        for name in ["instrumental-master", "drums", "drum-free", "bass"] {
            if let Some(stage) = intermediates.get(name)
                && let Some(reference) =
                    references.get(if name == "bass" { "bass" } else { "drums" })
            {
                rhythm.insert(
                    name,
                    compare_sources(reference.as_ref(), stage.as_ref(), &cancel)?,
                );
            }
        }
        let duration = metadata.frame_count as f64 / f64::from(metadata.format.sample_rate_hz);
        reports.push(serde_json::json!({"case_id":case.id,"source_sha256":hash,"license":case.license,"attribution":case.attribution,"frames":metadata.frame_count,"sample_rate":metadata.format.sample_rate_hz,"duration_seconds":duration,"decode_seconds":decode_seconds,"inference_seconds":inference_seconds,"processing_seconds":processing_seconds,"rtf":processing_seconds/duration,"metrics":metrics,"reconstruction":reconstruction,"wav_roundtrip":roundtrip,"wav_sha256":hashes,"stage_diagnostics":levels.report(),"rhythm_intermediates":rhythm,"output_directory":render}));
    }
    let report = serde_json::json!({"schema_version":3,"pipeline":format!("BoundedSeparation/{}",quality.id),"quality_profile":quality,"provider_requested":provider,"stage_cache":"disabled; fresh artifacts","measurement_scope":"RTF includes decode, separation, diagnostic aggregation and WAV writing; excludes reference scoring","sdr_definition":"10 log10(reference energy / squared sample error); no BSS Eval filtering","si_sdr_definition":"zero-mean per-channel scalar projection; no time shift","stem_mapping":{"lead_vocal":"vocals","backing_vocals":"secondary","drums":"drums","bass":"bass","instrumental":"other"},"models":inventory,"onnx_runtime":ort::info(),"session_diagnostics":pool.diagnostics(),"intra_threads":pool.intra_threads(),"inter_threads":1,"cases":reports});
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", output.join("report.json").display());
    if provider == "directml"
        && pool
            .diagnostics()
            .iter()
            .any(|d| d.provider_registered != ExecutionProviderKind::DirectMl)
    {
        return Err(
            "DirectML fell back to CPU; report saved, accelerator qualification failed".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn quality_profiles_and_providers_are_explicit() {
        assert_eq!(super::quality_profile(None).unwrap().id, "native-v1");
        assert_eq!(
            super::quality_profile(Some("native-v2-original-mix-bass")).unwrap(),
            atomusicator_domain::quality::SeparationProfile::production()
        );
        assert!(super::quality_profile(Some("latest")).is_err());
        assert!(super::benchmark_pool("unknown", 2).is_err());
    }
}
