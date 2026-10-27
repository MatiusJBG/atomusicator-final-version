//! Composition and background job ownership. The bridge passes paths/peaks, never PCM.
use crate::api::{
    UiJobDto, UiPlaybackDto, UiPlaybackFailure, UiSessionDto, UiStemTrackDto, UiTrackMeterDto,
};
use atomusicator_application::{
    CancellationToken,
    ports::{AudioDecoderPort, RealtimePlaybackPort, SeparationProgressPort, WaveformCachePort},
    use_cases::separate_five_stems::SeparateFiveStemsUseCase,
};
use atomusicator_domain::{AudioAsset, AudioAssetId, PitchShiftSpec, Stem, StemType};
use atomusicator_infrastructure::{
    audio_io::{SymphoniaAudioDecoder, playback::CpalRealtimeMixer, wav_codec::read_wav_f32},
    audio_tools::{AudioTools, ExportOptions, TemporaryDirectory, unique_id},
    cache::ContentAddressedCache,
    mdx::MdxInference,
    models::manifest_manager::ManifestModelManager,
    onnx::OrtSessionPool,
    pitch_cache::PitchRenderCache,
    preferences::{NativeLegacyProjects, ProjectsPreferences},
    session_workspace::LocalSessionWorkspace,
    storage::paths::WorkspacePathPolicy,
    use_case_adapters::StftSpectralBleedGate,
    waveform::WaveformPeakCache,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

pub struct Studio {
    pub mixer: CpalRealtimeMixer,
    job: Mutex<Option<Job>>,
    loaded: Mutex<Option<LoadedSession>>,
    projects: Mutex<PathBuf>,
    runtime: tokio::runtime::Runtime,
    sessions: Arc<OrtSessionPool>,
}
struct Job {
    dto: UiJobDto,
    cancel: CancellationToken,
}
#[derive(Clone)]
struct LoadedSession {
    dto: UiSessionDto,
    originals: Vec<(StemType, PathBuf)>,
}
static STUDIO: OnceLock<Result<Arc<Studio>, String>> = OnceLock::new();
pub fn studio() -> Result<Arc<Studio>, String> {
    STUDIO
        .get_or_init(|| {
            let policy = WorkspacePathPolicy::system_default().map_err(err)?;
            let projects = ProjectsPreferences::new(policy, NativeLegacyProjects)
                .load()
                .map_err(err)?;
            Ok(Arc::new(Studio {
                mixer: CpalRealtimeMixer::new().map_err(err)?,
                job: Mutex::new(None),
                loaded: Mutex::new(None),
                projects: Mutex::new(projects),
                runtime: tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .map_err(err)?,
                sessions: Arc::new(OrtSessionPool::new(2).map_err(err)?),
            }))
        })
        .clone()
}
impl Studio {
    pub fn projects_directory(&self) -> Result<String, String> {
        Ok(self
            .projects
            .lock()
            .map_err(err)?
            .to_string_lossy()
            .into_owned())
    }
    pub fn set_projects_directory(&self, path: &Path) -> Result<(), String> {
        let job = self.job.lock().map_err(err)?;
        ensure_idle(&job)?;
        let policy = WorkspacePathPolicy::system_default().map_err(err)?;
        let path = ProjectsPreferences::new(policy, NativeLegacyProjects)
            .save(path)
            .map_err(err)?;
        *self.projects.lock().map_err(err)? = path;
        Ok(())
    }
    pub fn job(&self, id: &str) -> Result<UiJobDto, String> {
        let jobs = self.job.lock().map_err(err)?;
        let job = jobs
            .as_ref()
            .filter(|j| j.dto.id == id)
            .ok_or("job not found")?;
        Ok(job.dto.clone())
    }
    pub fn cancel(&self, id: &str) -> Result<(), String> {
        let jobs = self.job.lock().map_err(err)?;
        let job = jobs
            .as_ref()
            .filter(|j| j.dto.id == id)
            .ok_or("job not found")?;
        if job.dto.status == "running" {
            job.cancel.cancel();
        }
        Ok(())
    }
    pub fn start(
        self: &Arc<Self>,
        kind: &str,
        operation: impl FnOnce(
            &Arc<Self>,
            &str,
            &CancellationToken,
        ) -> Result<Option<UiSessionDto>, String>
        + Send
        + 'static,
    ) -> Result<String, String> {
        let id = unique_id();
        let cancel = CancellationToken::default();
        {
            let mut job = self.job.lock().map_err(err)?;
            ensure_idle(&job)?;
            *job = Some(Job {
                cancel: cancel.clone(),
                dto: UiJobDto {
                    id: id.clone(),
                    status: "running".into(),
                    message: kind.into(),
                    progress: 0.0,
                    session: None,
                    output_directory: None,
                },
            });
        }
        self.mixer.pause();
        let backend = Arc::clone(self);
        let worker_id = id.clone();
        let spawn = std::thread::Builder::new()
            .name(format!("atomusicator-{kind}"))
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    operation(&backend, &worker_id, &cancel)
                }))
                .unwrap_or_else(|_| Err("native job failed unexpectedly".into()));
                if let Ok(mut job) = backend.job.lock()
                    && let Some(job) = job.as_mut().filter(|j| j.dto.id == worker_id)
                {
                    match result {
                        Ok(session) => {
                            job.dto.status = "completed".into();
                            job.dto.progress = 1.0;
                            job.dto.message = "Completed".into();
                            job.dto.session = session;
                        }
                        Err(error) => {
                            job.dto.status = if cancel.is_cancelled() {
                                "cancelled"
                            } else {
                                "failed"
                            }
                            .into();
                            job.dto.message = error;
                        }
                    }
                }
            });
        if let Err(error) = spawn {
            if let Ok(mut job) = self.job.lock() {
                *job = None;
            }
            return Err(err(error));
        }
        Ok(id)
    }
    fn report(&self, id: &str, progress: f32, message: &str) {
        if let Ok(mut job) = self.job.lock()
            && let Some(job) = job.as_mut().filter(|j| j.dto.id == id)
        {
            job.dto.progress = job.dto.progress.max(progress.clamp(0.0, 1.0));
            job.dto.message = message.into();
        }
    }
    pub fn separate(
        self: &Arc<Self>,
        source: PathBuf,
        download_allowed: bool,
    ) -> Result<String, String> {
        if !source.is_file() {
            return Err("source audio does not exist".into());
        }
        self.start("separation",move|backend,id,cancel| {
            let policy=WorkspacePathPolicy::system_default().map_err(err)?;
            let manager=ManifestModelManager::from_embedded_manifest(policy.clone()).map_err(err)?;
            let runtime=&backend.runtime;
            let mut models=BTreeMap::new();let mut usages=Vec::new();
            for (index,profile) in manager.manifest().models.iter().enumerate(){
                check_cancel(cancel)?;
                backend.report(id,index as f32*0.025,&format!("Verifying {}",profile.filename));
                let mut local=None;
                for candidate in policy.model_candidates(&profile.filename).map_err(err)? {
                    if candidate.is_file() && runtime.block_on(ManifestModelManager::verify_model_file(&candidate)).map(|h|h==profile.sha256).unwrap_or(false){local=Some(candidate);break;}
                }
                let path=match local { Some(path)=>path,None if download_allowed=>runtime.block_on(manager.ensure_profile_ready_cancellable(&profile.profile_id, cancel)).map_err(err)?.local_path,None=>return Err("Models are missing or corrupt. Authorize the verified model download and retry.".into()) };
                models.insert(profile.profile_id.clone(),(path,profile.sha256.clone()));
                usages.push(serde_json::json!({"profile_id":profile.profile_id,"sha256":profile.sha256,"license":profile.license,"attribution":profile.attribution,"runtime":"native-mdx-v1 / ort 2.0.0-rc.9"}));
            }
            check_cancel(cancel)?;
            backend.report(id,0.1,"Decoding audio to 44100 Hz stereo");
            let audio=AudioTools::discover().map_err(err)?.decode_for_separation(&source,cancel).map_err(err)?;
            let cache=ContentAddressedCache::new(policy.stage_cache_dir(),1,4*1024*1024*1024).ok();
            let inference=MdxInference::with_session_pool(models,cache,Arc::clone(&backend.sessions)).map_err(err)?;
            let progress=JobProgress{studio:Arc::clone(backend),id:id.into()};
            let pipeline=SeparateFiveStemsUseCase::new(inference,progress,StftSpectralBleedGate::new(2048,0.05,0.25).map_err(err)?);
            let result=pipeline.execute(&audio,cancel).map_err(err)?;
            check_cancel(cancel)?;
            backend.report(id,0.92,"Publishing five aligned WAV tracks");
            let workspace=LocalSessionWorkspace::new(backend.projects.lock().map_err(err)?.clone());
            let asset=AudioAsset{id:AudioAssetId::new(unique_id()).map_err(err)?,display_name:source.file_name().unwrap_or_default().to_string_lossy().into_owned(),path:source};
            let published=workspace.persist_separation_result(&asset,&result).map_err(err)?;
            let mut metadata=workspace.read_metadata(&published).map_err(err)?;
            let record=metadata.additional.get_mut("separation").and_then(|r|r.as_object_mut()).ok_or("separation metadata missing")?;
            record.insert("engine".into(),serde_json::json!({"name":"Atomusicator native MDX","models":usages,"sample_rate":44100,"overlap":0.35,"residuals":"exact phase subtraction","vad_dbfs":-65,"compute":{"provider":"CPUExecutionProvider","intra_threads":backend.sessions.intra_threads(),"inter_threads":1,"execution":"sequential","precision":"float32"}}));
            workspace.record_separation(&published,serde_json::Value::Object(record.clone())).map_err(err)?;
            let session=backend.load_directory(&published.directory)?;
            Ok(Some(session))
        })
    }
    pub fn open(&self, path: &Path) -> Result<UiSessionDto, String> {
        let job = self.job.lock().map_err(err)?;
        ensure_idle(&job)?;
        self.load_directory(path)
    }
    fn load_directory(&self, path: &Path) -> Result<UiSessionDto, String> {
        let path = path.canonicalize().map_err(err)?;
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(path.join("session.json")).map_err(err)?)
                .map_err(err)?;
        let stems = value
            .pointer("/separation/stems")
            .ok_or("session has no complete separation")?;
        let mut originals = Vec::new();
        for kind in StemType::ALL {
            let key = stem_key(kind);
            let entry = if let Some(array) = stems.as_array() {
                array.iter().find(|s| s["kind"].as_str() == Some(key))
            } else {
                stems.get(format!("{kind:?}"))
            }
            .ok_or_else(|| format!("session is missing {key}"))?;
            let relative = entry["path"].as_str().ok_or("invalid stem path")?;
            let candidate = path.join(relative).canonicalize().map_err(err)?;
            if !candidate.starts_with(&path) {
                return Err("stem path escapes the session directory".into());
            }
            originals.push((kind, candidate));
        }
        let session_id = value["session_id"]
            .as_str()
            .ok_or("missing session id")?
            .to_owned();
        let name = value["file_name"]
            .as_str()
            .unwrap_or("Audio session")
            .to_owned();
        let dto = self.prepare_tracks(
            session_id,
            path.to_string_lossy().into_owned(),
            name,
            &originals,
            0,
        )?;
        *self.loaded.lock().map_err(err)? = Some(LoadedSession {
            dto: dto.clone(),
            originals,
        });
        Ok(dto)
    }
    fn prepare_tracks(
        &self,
        id: String,
        directory: String,
        name: String,
        paths: &[(StemType, PathBuf)],
        pitch: i32,
    ) -> Result<UiSessionDto, String> {
        let peaks = WaveformPeakCache::new(5).ok_or("invalid peak cache")?;
        let mut stems = Vec::new();
        let mut tracks = Vec::new();
        for (kind, path) in paths {
            let audio = read_wav_f32(path)
                .or_else(|_| SymphoniaAudioDecoder.decode_to_f32(path))
                .map_err(err)?;
            let buckets = peaks.peaks(&audio, audio.frames().div_ceil(1000).max(1));
            tracks.push(UiStemTrackDto {
                stem_type: stem_key(*kind).into(),
                file_path: path.to_string_lossy().into_owned(),
                sample_rate: audio.format().sample_rate_hz,
                channels: audio.format().channels,
                duration_seconds: audio.frames() as f64 / f64::from(audio.format().sample_rate_hz),
                peaks: buckets
                    .into_iter()
                    .flat_map(|(min, max)| [min, max])
                    .collect(),
            });
            stems.push(Stem { kind: *kind, audio });
        }
        // Validation occurs before replacing the currently loaded session.
        self.mixer.load_session(stems).map_err(err)?;
        Ok(UiSessionDto {
            id,
            directory,
            name,
            tracks,
            pitch_semitones: pitch,
        })
    }
    pub fn pitch(self: &Arc<Self>, semitones: i32, preserve: bool) -> Result<String, String> {
        let spec = PitchShiftSpec::try_new(semitones as f32, preserve).map_err(err)?;
        let loaded = self
            .loaded
            .lock()
            .map_err(err)?
            .clone()
            .ok_or("no session loaded")?;
        self.start("pitch", move |backend, id, cancel| {
            let position = backend.mixer.position_seconds();
            let paths = if semitones == 0 {
                loaded.originals.clone()
            } else {
                let directory = PathBuf::from(&loaded.dto.directory).join("renders");
                let tools = AudioTools::discover().map_err(err)?;
                let runtime = tools.pitch_runtime_signature(cancel).map_err(err)?;
                let mut index = 0;
                PitchRenderCache::new(directory)
                    .get_or_render(
                        &loaded.originals,
                        spec,
                        &runtime,
                        cancel,
                        |source, target| {
                            backend.report(
                                id,
                                index as f32 / 5.0,
                                &format!("Rendering track {} of 5", index + 1),
                            );
                            index += 1;
                            tools.pitch(source, target, spec, cancel)
                        },
                    )
                    .map_err(err)?
            };
            check_cancel(cancel)?;
            let dto = backend.prepare_tracks(
                loaded.dto.id.clone(),
                loaded.dto.directory.clone(),
                loaded.dto.name.clone(),
                &paths,
                semitones,
            )?;
            backend.mixer.seek(position).map_err(err)?;
            *backend.loaded.lock().map_err(err)? = Some(LoadedSession {
                dto: dto.clone(),
                originals: loaded.originals,
            });
            Ok(Some(dto))
        })
    }
    pub fn export(
        self: &Arc<Self>,
        destination: PathBuf,
        selected: Vec<String>,
        options: ExportOptions,
    ) -> Result<String, String> {
        if !destination.is_dir() {
            return Err("export destination must exist".into());
        }
        if selected.is_empty() {
            return Err("select at least one track".into());
        }
        let loaded = self
            .loaded
            .lock()
            .map_err(err)?
            .clone()
            .ok_or("no session loaded")?;
        for key in &selected {
            if !loaded.dto.tracks.iter().any(|t| &t.stem_type == key) {
                return Err("unknown export track".into());
            }
        }
        self.start("export", move |backend, id, cancel| {
            let stage = TemporaryDirectory::new(&destination).map_err(err)?;
            let tools = AudioTools::discover().map_err(err)?;
            let tracks = loaded
                .dto
                .tracks
                .iter()
                .filter(|t| selected.contains(&t.stem_type))
                .collect::<Vec<_>>();
            for (index, track) in tracks.iter().enumerate() {
                check_cancel(cancel)?;
                backend.report(
                    id,
                    index as f32 / tracks.len() as f32,
                    &format!("Exporting {}", track.stem_type),
                );
                tools
                    .export(
                        Path::new(&track.file_path),
                        &stage
                            .0
                            .join(format!("{}.{}", track.stem_type, options.format)),
                        &options,
                        cancel,
                    )
                    .map_err(err)?;
            }
            check_cancel(cancel)?;
            let target = destination.join(format!("Atomusicator-export-{}", unique_id()));
            fs::rename(&stage.0, &target).map_err(err)?;
            if let Ok(mut job) = backend.job.lock()
                && let Some(job) = job.as_mut()
            {
                job.dto.output_directory = Some(target.to_string_lossy().into_owned());
            }
            Ok(None)
        })
    }
    pub fn playback(&self) -> UiPlaybackDto {
        UiPlaybackDto {
            playing: self.mixer.is_playing(),
            position_seconds: self.mixer.position_seconds(),
            peak: self.mixer.peak(),
            track_meters: StemType::ALL
                .into_iter()
                .zip(self.mixer.track_peaks())
                .map(|(kind, peak)| UiTrackMeterDto {
                    stem_type: stem_key(kind).into(),
                    peak,
                })
                .collect(),
            failure: self
                .mixer
                .device_failed()
                .then_some(UiPlaybackFailure::DeviceUnavailable),
        }
    }
}
struct JobProgress {
    studio: Arc<Studio>,
    id: String,
}
impl SeparationProgressPort for JobProgress {
    fn report(&self, done: u8, total: u8, message: &str) {
        self.studio.report(
            &self.id,
            0.15 + 0.7 * f32::from(done) / f32::from(total),
            message,
        );
    }
}
fn ensure_idle(job: &Option<Job>) -> Result<(), String> {
    if job.as_ref().is_some_and(|j| j.dto.status == "running") {
        Err("another audio job is already running".into())
    } else {
        Ok(())
    }
}
fn check_cancel(cancel: &CancellationToken) -> Result<(), String> {
    if cancel.is_cancelled() {
        Err("operation cancelled".into())
    } else {
        Ok(())
    }
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
pub fn stem_key(kind: StemType) -> &'static str {
    match kind {
        StemType::LeadVocal => "lead_vocal",
        StemType::BackingVocals => "backing_vocals",
        StemType::Drums => "drums",
        StemType::Bass => "bass",
        StemType::Instrumental => "instrumental",
    }
}
pub fn parse_stem(key: &str) -> Result<StemType, String> {
    StemType::ALL
        .into_iter()
        .find(|k| stem_key(*k) == key)
        .ok_or_else(|| "unknown stem type".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use atomusicator_domain::{AudioBufferF32, AudioFormat};
    use atomusicator_infrastructure::audio_io::wav_codec::write_wav_f32;
    use std::time::{Duration, Instant};
    fn backend(root: &Path) -> Arc<Studio> {
        Arc::new(Studio {
            mixer: CpalRealtimeMixer::new().unwrap(),
            job: Mutex::new(None),
            loaded: Mutex::new(None),
            projects: Mutex::new(root.to_path_buf()),
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap(),
            sessions: Arc::new(OrtSessionPool::new(2).unwrap()),
        })
    }
    fn wait(backend: &Studio, id: &str) -> UiJobDto {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let job = backend.job(id).unwrap();
            if job.status != "running" {
                return job;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn legacy_session(root: &Path) {
        let audio =
            AudioBufferF32::new(AudioFormat::new(44100, 2).unwrap(), vec![0.1; 88200]).unwrap();
        let mut stems = Vec::new();
        for kind in StemType::ALL {
            let relative = format!("outputs/{}.wav", stem_key(kind));
            write_wav_f32(&root.join(&relative), &audio).unwrap();
            stems.push(serde_json::json!({"kind":stem_key(kind),"path":relative}));
        }
        fs::write(root.join("session.json"),serde_json::to_vec(&serde_json::json!({"session_id":"legacy","file_name":"example.wav","separation":{"status":"complete","stems":stems}})).unwrap()).unwrap();
    }
    #[test]
    fn jobs_reject_overlap_cancel_and_keep_failures_visible() {
        let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let backend = backend(&root.0);
        let first = backend
            .start("test", |_, _, cancel| {
                while !cancel.is_cancelled() {
                    std::thread::sleep(Duration::from_millis(5));
                }
                check_cancel(cancel)?;
                Ok(None)
            })
            .unwrap();
        assert!(backend.start("second", |_, _, _| Ok(None)).is_err());
        backend.cancel(&first).unwrap();
        assert_eq!(wait(&backend, &first).status, "cancelled");
        let failed = backend
            .start("failure", |_, _, _| Err("broken codec".into()))
            .unwrap();
        let job = wait(&backend, &failed);
        assert_eq!(job.status, "failed");
        assert_eq!(job.message, "broken codec");
        assert!(backend.job(&first).is_err());
    }
    #[test]
    fn legacy_sessions_restore_waveforms_and_reject_external_stems() {
        let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        legacy_session(&root.0);
        let backend = backend(&root.0);
        let session = backend.open(&root.0).unwrap();
        assert_eq!(session.tracks.len(), 5);
        assert!(!session.tracks[0].peaks.is_empty());
        let source = root.0.join("session.json");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&source).unwrap()).unwrap();
        value["separation"]["stems"][0]["path"] = serde_json::Value::String(
            std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        );
        fs::write(&source, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(backend.open(&root.0).unwrap_err().contains("escapes"));
        assert_eq!(
            backend.loaded.lock().unwrap().as_ref().unwrap().dto.id,
            "legacy"
        );
    }
    #[test]
    #[ignore = "requires the pinned FFmpeg runtime"]
    fn pitch_and_export_jobs_preserve_originals_and_publish_complete_batches() {
        let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        legacy_session(&root.0);
        let backend = backend(&root.0);
        backend.open(&root.0).unwrap();
        let original = fs::read(root.0.join("outputs/lead_vocal.wav")).unwrap();
        let id = backend.pitch(3, true).unwrap();
        let job = wait(&backend, &id);
        assert_eq!(job.status, "completed", "{}", job.message);
        let pitched = job.session.unwrap();
        assert_eq!(pitched.pitch_semitones, 3);
        let render_path = PathBuf::from(&pitched.tracks[0].file_path);
        let modified = fs::metadata(&render_path).unwrap().modified().unwrap();
        let id = backend.pitch(3, true).unwrap();
        let reused = wait(&backend, &id);
        assert_eq!(reused.status, "completed", "{}", reused.message);
        assert_eq!(
            reused.session.unwrap().tracks[0].file_path,
            pitched.tracks[0].file_path
        );
        assert_eq!(
            fs::metadata(&render_path).unwrap().modified().unwrap(),
            modified
        );
        assert_eq!(
            fs::read(root.0.join("outputs/lead_vocal.wav")).unwrap(),
            original
        );
        let id = backend
            .export(
                root.0.clone(),
                vec!["bass".into(), "lead_vocal".into()],
                ExportOptions::new("flac", 48000, 1, 24, false).unwrap(),
            )
            .unwrap();
        let job = wait(&backend, &id);
        assert_eq!(job.status, "completed", "{}", job.message);
        let directory = PathBuf::from(job.output_directory.unwrap());
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 2);
        let id = backend.pitch(0, false).unwrap();
        let job = wait(&backend, &id);
        assert_eq!(job.status, "completed", "{}", job.message);
        assert_eq!(
            job.session.unwrap().tracks[0].file_path,
            root.0
                .join("outputs/lead_vocal.wav")
                .canonicalize()
                .unwrap()
                .to_string_lossy()
        );
    }
    #[test]
    #[ignore = "requires cached MDX models and the pinned FFmpeg runtime"]
    fn separation_job_publishes_a_reopenable_session() {
        let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let backend = backend(&root.0);
        let source = root.0.join("test.wav");
        let samples = (0..22050)
            .flat_map(|i| {
                let v = (i as f32 * 0.0627).sin() * 0.2;
                [v, v * 0.8]
            })
            .collect();
        write_wav_f32(
            &source,
            &AudioBufferF32::new(AudioFormat::new(44100, 2).unwrap(), samples).unwrap(),
        )
        .unwrap();
        let id = backend.separate(source, false).unwrap();
        let job = wait(&backend, &id);
        assert_eq!(job.status, "completed", "{}", job.message);
        let dto = job.session.unwrap();
        assert_eq!(dto.tracks.len(), 5);
        let path = PathBuf::from(dto.directory);
        assert_eq!(backend.open(&path).unwrap().tracks.len(), 5);
        let metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(path.join("session.json")).unwrap()).unwrap();
        assert_eq!(
            metadata
                .pointer("/separation/engine/models")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            4
        );
    }
}
