//! Composition and background job ownership. The bridge passes paths/peaks, never PCM.
#[cfg(test)]
mod identity_tests;
mod live_pitch;
use crate::api::{UiJobDto, UiPlaybackDto, UiPlaybackFailure, UiSessionDto, UiTrackMeterDto};
use crate::session_preparation::stem_key;
use atomusicator_application::{
    CancellationToken, ports::SeparationProgressPort,
    use_cases::bounded_separation::BoundedSeparation,
};
use atomusicator_domain::{AudioAsset, AudioAssetId, PitchShiftSpec, StemType};
use atomusicator_infrastructure::{
    audio_io::playback::CpalRealtimeMixer,
    audio_tools::{AudioTools, ExportOptions, TemporaryDirectory, unique_id},
    mdx::MdxInference,
    models::manifest_manager::ManifestModelManager,
    onnx::OrtSessionPool,
    pitch_cache::PitchRenderCache,
    preferences::{NativeLegacyProjects, ProjectsPreferences},
    storage::paths::WorkspacePathPolicy,
    use_case_adapters::StftSpectralBleedGate,
};
use live_pitch::pitch_persistence_key;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

pub struct Studio {
    pub mixer: CpalRealtimeMixer,
    jobs: Arc<crate::jobs::JobRegistry>,
    loaded: Mutex<Option<LoadedSession>>,
    projects: Mutex<PathBuf>,
    runtime: tokio::runtime::Runtime,
    sessions: Arc<OrtSessionPool>,
    resources: atomusicator_infrastructure::resource_probe::NativeResourceAdmission,
    pitch_cache: PitchRenderCache,
    live_pitch_store: atomusicator_infrastructure::live_pitch_store::LivePitchStore,
}
#[derive(Clone)]
struct LoadedSession {
    dto: UiSessionDto,
    originals: Vec<(StemType, PathBuf)>,
    waveforms: BTreeMap<StemType, Arc<atomusicator_infrastructure::waveform_index::WaveformIndex>>,
    _pitch_batch: Option<atomusicator_infrastructure::pitch_cache::LeasedPitchBatch>,
    pitch_key: String,
}
static STUDIO: OnceLock<Result<Arc<Studio>, String>> = OnceLock::new();
pub fn studio() -> Result<Arc<Studio>, String> {
    STUDIO
        .get_or_init(|| {
            let policy = WorkspacePathPolicy::system_default().map_err(err)?;
            let projects = ProjectsPreferences::new(policy.clone(), NativeLegacyProjects)
                .load()
                .map_err(err)?;
            Ok(Arc::new(Studio {
                mixer: CpalRealtimeMixer::new().map_err(err)?,
                jobs: Arc::new(crate::jobs::JobRegistry::new(
                    atomusicator_infrastructure::job_journal::JobJournal::new(
                        policy.app_data_root().join("jobs"),
                    ),
                )),
                loaded: Mutex::new(None),
                projects: Mutex::new(projects),
                runtime: tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .map_err(err)?,
                sessions: Arc::new(OrtSessionPool::new(2).map_err(err)?),
                pitch_cache: PitchRenderCache::new(policy.stage_cache_dir().join("pitch-v1")),
                live_pitch_store:
                    atomusicator_infrastructure::live_pitch_store::LivePitchStore::new(
                        policy.app_data_root().join("live-pitch"),
                    )
                    .map_err(err)?,
                resources:
                    atomusicator_infrastructure::resource_probe::NativeResourceAdmission::new(
                        atomusicator_application::compute_policy::ComputeBudget::interactive(
                            std::thread::available_parallelism().map_or(1, usize::from),
                        )
                        .threads_per_inference(),
                    )
                    .map_err(err)?,
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
        let _idle = self.jobs.idle()?;
        let policy = WorkspacePathPolicy::system_default().map_err(err)?;
        let path = ProjectsPreferences::new(policy.clone(), NativeLegacyProjects)
            .save(path)
            .map_err(err)?;
        *self.projects.lock().map_err(err)? = path;
        Ok(())
    }
    pub fn job(&self, id: &str) -> Result<UiJobDto, String> {
        self.jobs.snapshot(id)
    }
    pub fn cancel(&self, id: &str) -> Result<(), String> {
        self.jobs.cancel(id)
    }
    pub fn start(
        self: &Arc<Self>,
        kind: &str,
        operation: impl FnOnce(
            &Arc<Self>,
            &str,
            &CancellationToken,
        ) -> Result<Option<UiSessionDto>, crate::jobs::JobFailure>
        + Send
        + 'static,
    ) -> Result<String, String> {
        self.start_request(
            atomusicator_infrastructure::job_journal::JobRequest::Ephemeral {
                operation: kind.into(),
            },
            None,
            operation,
        )
    }
    fn start_request(
        self: &Arc<Self>,
        request: atomusicator_infrastructure::job_journal::JobRequest,
        retry_of: Option<String>,
        operation: impl FnOnce(
            &Arc<Self>,
            &str,
            &CancellationToken,
        ) -> Result<Option<UiSessionDto>, crate::jobs::JobFailure>
        + Send
        + 'static,
    ) -> Result<String, String> {
        let backend = Arc::clone(self);
        let id = self.jobs.start(request, retry_of, move |id, cancel| {
            operation(&backend, id, cancel)
        })?;
        self.mixer.pause();
        Ok(id)
    }
    fn report(&self, id: &str, progress: f32, message: &str) {
        self.jobs.report(id, progress, message);
    }
    pub fn recoverable(&self) -> Result<Vec<crate::api::UiRecoverableJobDto>, String> {
        self.jobs
            .journal
            .recoverable()
            .map_err(err)?
            .into_iter()
            .map(|record| {
                let atomusicator_infrastructure::job_journal::JobRequest::Separation {
                    source, ..
                } = record.request.clone()
                else {
                    unreachable!()
                };
                Ok(crate::api::UiRecoverableJobDto {
                    id: record.id,
                    source_path: source.to_string_lossy().into_owned(),
                    message: record.message,
                    output_directory: record
                        .output_directory
                        .map(|p| p.to_string_lossy().into_owned()),
                })
            })
            .collect()
    }
    pub fn discard(&self, id: &str) -> Result<(), String> {
        self.jobs.journal.discard(id).map_err(err)
    }
    pub fn resume(self: &Arc<Self>, id: &str) -> Result<String, String> {
        let _lease = self.jobs.journal.lease(id).map_err(err)?;
        let mut record = self.jobs.journal.load(id).map_err(err)?;
        if record.discarded
            || record.lifecycle.status() == atomusicator_domain::jobs::JobStatus::Completed
        {
            return Err("job cannot be resumed".into());
        }
        if record.profile != atomusicator_domain::quality::SeparationProfile::compatibility() {
            return Err("job quality profile is incompatible; start a new separation".into());
        }
        self.jobs.journal.cleanup_staging(&record).map_err(err)?;
        let atomusicator_infrastructure::job_journal::JobRequest::Separation {
            source,
            projects,
            download_allowed,
        } = record.request.clone()
        else {
            return Err("this operation does not support recovery".into());
        };
        let published = record
            .output_directory
            .clone()
            .filter(|path| path.join("session.json").is_file());
        let published = if let Some(directory) = published {
            let directory = directory.canonicalize().map_err(err)?;
            let projects = projects.canonicalize().map_err(err)?;
            if !directory.starts_with(&projects) {
                return Err("recovery output escapes its project directory".into());
            }
            match atomusicator_infrastructure::session_reader::read_session_files(
                &directory,
                &CancellationToken::default(),
            ) {
                Ok(_) => Some(directory),
                Err(_) if source.is_file() => None,
                Err(error) => {
                    return Err(format!(
                        "Saved output is invalid ({error}) and the original source is unavailable. Select the source again to rebuild the session."
                    ));
                }
            }
        } else {
            None
        };
        let next = if let Some(directory) = published {
            self.start_request(
                record.request.clone(),
                Some(record.id.clone()),
                move |backend, id, cancel| {
                    check_cancel(cancel)?;
                    backend.jobs.begin_commit(id)?;
                    let session = backend.load_directory(&directory).map_err(|error| {
                        crate::jobs::JobFailure::new(
                            atomusicator_domain::jobs::FailureKind::InvalidArtifact,
                            error,
                        )
                    })?;
                    backend.jobs.output(id, PathBuf::from(&session.directory))?;
                    Ok(Some(session))
                },
            )?
        } else {
            self.separate_request(
                source,
                projects,
                download_allowed,
                Some(record.id.clone()),
                record.source_sha256.clone(),
            )?
        };
        record.discarded = true;
        if let Err(error) = self.jobs.journal.save(&mut record) {
            self.jobs.report(
                &next,
                0.0,
                &format!("Retry started; previous attempt could not be marked superseded: {error}"),
            );
        }
        Ok(next)
    }
    pub fn separate(
        self: &Arc<Self>,
        source: PathBuf,
        download_allowed: bool,
    ) -> Result<String, String> {
        let source = source.canonicalize().map_err(err)?;
        let projects = self.projects.lock().map_err(err)?.clone();
        self.separate_request(source, projects, download_allowed, None, None)
    }
    fn separate_request(
        self: &Arc<Self>,
        source: PathBuf,
        projects: PathBuf,
        download_allowed: bool,
        retry_of: Option<String>,
        expected_hash: Option<String>,
    ) -> Result<String, String> {
        if !source.is_file() {
            return Err("source audio does not exist".into());
        }
        let policy = WorkspacePathPolicy::system_default().map_err(err)?;
        let active = policy.active_projects_destination(&projects);
        let projects = WorkspacePathPolicy::validate_projects_destination(&active).map_err(err)?;
        let request = atomusicator_infrastructure::job_journal::JobRequest::Separation {
            source: source.clone(),
            projects: projects.clone(),
            download_allowed,
        };
        self.start_request(request, retry_of, move |backend,id,cancel| {
            let _resources=backend.resources.reserve_separation(&backend.sessions).map_err(crate::jobs::JobFailure::from_domain)?;
            let source_hash = atomusicator_infrastructure::job_journal::JobJournal::hash_source(&source, cancel).map_err(err)?;
            if expected_hash.is_some_and(|expected| expected != source_hash) {
                return Err("Source content changed since the interrupted attempt; start a new separation".into());
            }
            backend.jobs.source_hash(id, source_hash.clone())?;
            let policy = WorkspacePathPolicy::system_default().map_err(err)?;
            let manager = ManifestModelManager::from_embedded_manifest(policy.clone()).map_err(err)?;
            let runtime = &backend.runtime;
            let mut models = BTreeMap::new();
            let mut usages = Vec::new();
            for (index, profile) in manager.manifest().models.iter().enumerate() {
                check_cancel(cancel)?;
                backend.report(id,index as f32 * 0.025,&format!("Verifying {}",profile.filename));
                let local=runtime.block_on(manager.cached_profile(&profile.profile_id)).map_err(err)?;
                let path = match local {
                    Some(model) => model.local_path,
                    None if download_allowed => runtime.block_on(manager.ensure_profile_ready_cancellable(&profile.profile_id,cancel)).map_err(err)?.local_path,
                    None => return Err("Models are missing or corrupt. Authorize the verified model download and retry.".into()),
                };
                models.insert(profile.profile_id.clone(),(path,profile.sha256.clone()));
                usages.push(serde_json::json!({"profile_id":profile.profile_id,"sha256":profile.sha256,"license":profile.license,"attribution":profile.attribution,"runtime":"native-mdx-v1 / ort 2.0.0-rc.9"}));
            }
            check_cancel(cancel)?;
            backend.report(id,0.1,"Decoding audio to 44100 Hz stereo");
            let work = TemporaryDirectory::for_job(&projects,id).map_err(err)?;
            backend.jobs.work_directory(id,work.0.clone())?;
            let audio = Arc::new(AudioTools::discover().map_err(err)?
                .decode_to_spool(&source,&work.0.join("decoded.f32"),cancel).map_err(crate::jobs::JobFailure::from_domain)?);
            use atomusicator_application::ports::AudioBlockSource;
            atomusicator_infrastructure::audio_io::wav_codec::validate_block_wav_size(audio.metadata()).map_err(crate::jobs::JobFailure::from_domain)?;
            let working_bytes=audio.metadata().frame_count.checked_mul(8*15).ok_or("audio working size overflow")?;
            for destination in [&policy.stage_cache_dir(),&projects,&work.0] {
                atomusicator_infrastructure::resource_probe::ensure_disk(destination,working_bytes).map_err(crate::jobs::JobFailure::from_domain)?;
            }
            let store = atomusicator_infrastructure::storage::float_spool::FloatSpoolStore::new(policy.stage_cache_dir().join("bounded-v1"));
            let inference = MdxInference::with_session_pool(models,None,Arc::clone(&backend.sessions)).map_err(err)?;
            let progress = JobProgress { studio: Arc::clone(backend), id: id.into() };
            let pipeline = BoundedSeparation::new(inference,progress,StftSpectralBleedGate::new(2048,0.05,0.25).map_err(err)?,store);
            let result = pipeline.execute(audio,cancel).map_err(crate::jobs::JobFailure::from_domain)?;
            check_cancel(cancel)?;
            if atomusicator_infrastructure::job_journal::JobJournal::hash_source(&source,cancel).map_err(err)? != source_hash {
                return Err("Source content changed during separation; result was not published".into());
            }
            backend.report(id, 0.92, "Publishing five aligned WAV tracks");
            let asset = AudioAsset {
                id: AudioAssetId::new(unique_id()).map_err(err)?,
                display_name: source.file_name().unwrap_or_default().to_string_lossy().into_owned(), path: source,
            };
            let engine=serde_json::json!({"name":"Atomusicator native MDX","models":usages,
                "source_sha256":source_hash,"profile":atomusicator_domain::quality::SeparationProfile::compatibility(),
                "sample_rate":44100,"overlap":0.35,"residuals":"exact phase subtraction","vad_dbfs":-65,
                "compute":{"provider":"CPUExecutionProvider","intra_threads":backend.sessions.intra_threads(),
                    "inter_threads":1,"execution":"sequential","precision":"float32","pipeline":"bounded-v1"}});
            backend.jobs.output(id,projects.join(id))?;
            let published=atomusicator_infrastructure::block_workspace::publish_blocks(&projects,id,&asset,&result,engine,cancel,
                atomusicator_infrastructure::block_workspace::PublicationHooks{
                    staged:|path:&Path|backend.jobs.staging(id,path.to_path_buf()),commit:||backend.jobs.begin_commit(id)
                }).map_err(crate::jobs::JobFailure::from_domain)?;
            let session = backend.load_directory(&published.directory)?;
            drop(result);
            if let Err(error)=atomusicator_infrastructure::storage::float_spool::FloatSpoolStore::new(policy.stage_cache_dir().join("bounded-v1")).prune(4*1024*1024*1024) {
                eprintln!("Nonfatal audio cache cleanup failure: {error}");
            }
            Ok(Some(session))
        })
    }
    pub fn open(&self, path: &Path) -> Result<UiSessionDto, String> {
        let _idle = self.jobs.idle()?;
        self.load_directory(path)
    }
    pub fn waveform_peaks(
        &self,
        session_id: &str,
        stem_type: &str,
        file_path: &str,
        max_buckets: u32,
    ) -> Result<Vec<f32>, String> {
        let kind = parse_stem(stem_type)?;
        let index = {
            let loaded = self.loaded.lock().map_err(err)?;
            let loaded = loaded.as_ref().ok_or("no session loaded")?;
            if loaded.dto.id != session_id
                || !loaded
                    .dto
                    .tracks
                    .iter()
                    .any(|t| t.stem_type == stem_type && t.file_path == file_path)
            {
                return Err("session or pitch changed while requesting waveform detail".into());
            }
            Arc::clone(loaded.waveforms.get(&kind).ok_or("waveform unavailable")?)
        };
        index
            .range(0, index.metadata().frame_count, max_buckets as usize)
            .map(|r| r.peaks)
            .map_err(err)
    }
    fn load_directory(&self, path: &Path) -> Result<UiSessionDto, String> {
        let policy = WorkspacePathPolicy::system_default().map_err(err)?;
        self.load_directory_with_policy(path, &policy)
    }
    fn load_directory_with_policy(
        &self,
        path: &Path,
        policy: &WorkspacePathPolicy,
    ) -> Result<UiSessionDto, String> {
        let mut loaded_slot = self.loaded.lock().map_err(err)?;
        if let Err(error) = self.live_pitch_store.flush() {
            eprintln!("Pitch settings flush failed: {error}");
        }

        let active_path =
            atomusicator_infrastructure::storage::legacy_import::resolve_session(policy, path)
                .map_err(err)?;
        let files = atomusicator_infrastructure::session_reader::read_session_files(
            &active_path,
            &CancellationToken::default(),
        )
        .map_err(err)?;
        let path = files.directory;
        let originals = files.stems;
        let session_id = files.id;
        let name = files.name;
        let prepared = crate::session_preparation::prepare_tracks(
            &self.mixer,
            session_id,
            path.to_string_lossy().into_owned(),
            name,
            &originals,
            0,
        )?;
        let mut dto = prepared.dto;
        let pitch_key = pitch_persistence_key(&dto)?;
        match self.restore_pitch_settings(&dto, &pitch_key, policy) {
            Ok(Some(spec)) => {
                let epoch = self.mixer.live_pitch().snapshot().epoch;
                self.mixer.live_pitch().set(epoch, 1, spec).map_err(err)?;
                dto.pitch_semitones = spec.semitones() as i32;
            }
            Ok(None) => {}
            Err(error) => eprintln!("Ignoring invalid saved pitch settings: {error}"),
        }
        *loaded_slot = Some(LoadedSession {
            dto: dto.clone(),
            originals,
            waveforms: prepared.waveforms,
            _pitch_batch: None,
            pitch_key,
        });
        Ok(dto)
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
            let cache = &backend.pitch_cache;
            let batch = if semitones == 0 {
                None
            } else {
                let tools = AudioTools::discover().map_err(err)?;
                let runtime = tools.pitch_runtime_signature(cancel).map_err(err)?;
                let mut index = 0;
                Some(
                    cache
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
                        .map_err(crate::jobs::JobFailure::from_domain)?,
                )
            };
            let paths = batch
                .as_ref()
                .map(|b| b.paths.as_slice())
                .unwrap_or(&loaded.originals);
            check_cancel(cancel)?;
            backend.jobs.begin_commit(id)?;
            let mut loaded_slot = backend.loaded.lock().map_err(err)?;
            let prepared = crate::session_preparation::prepare_tracks(
                &backend.mixer,
                loaded.dto.id.clone(),
                loaded.dto.directory.clone(),
                loaded.dto.name.clone(),
                paths,
                semitones,
            )?;
            let dto = prepared.dto;
            backend.mixer.seek(position).map_err(err)?;
            let pitch_key = pitch_persistence_key(&dto)?;
            *loaded_slot = Some(LoadedSession {
                dto: dto.clone(),
                originals: loaded.originals.clone(),
                waveforms: prepared.waveforms,
                _pitch_batch: batch,
                pitch_key,
            });
            drop(loaded);
            if let Err(error) = cache.prune(2 * 1024 * 1024 * 1024) {
                eprintln!("Nonfatal pitch cache cleanup failure: {error}");
            }
            Ok(Some(dto))
        })
    }
    pub fn export(
        self: &Arc<Self>,
        destination: PathBuf,
        selected: Vec<String>,
        options: ExportOptions,
    ) -> Result<String, String> {
        self.export_with_pitch(destination, selected, options, None)
    }
    pub fn export_with_pitch(
        self: &Arc<Self>,
        destination: PathBuf,
        selected: Vec<String>,
        options: ExportOptions,
        pitch: Option<crate::api::UiPitchExportDto>,
    ) -> Result<String, String> {
        if !destination.is_dir() {
            return Err("export destination must exist".into());
        }
        if selected.is_empty() {
            return Err("select at least one track".into());
        }
        let (loaded, live_spec) = {
            let slot = self.loaded.lock().map_err(err)?;
            let loaded = slot.clone().ok_or("no session loaded")?;
            let snapshot = self.mixer.live_pitch().snapshot();
            let spec = if let Some(pitch) = pitch {
                if pitch.session_id != loaded.dto.id || pitch.epoch != snapshot.epoch {
                    return Err("export pitch session changed".into());
                }
                PitchShiftSpec::try_new(pitch.semitones as f32, pitch.preserve_formants)
                    .map_err(err)?
            } else {
                snapshot.spec
            };
            (loaded, spec)
        };
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
                        {
                            if live_spec.semitones() != 0.0 {
                                let rendered =
                                    stage.0.join(format!(".pitch-{}.wav", track.stem_type));
                                atomusicator_infrastructure::live_pitch::render_file(
                                    Path::new(&track.file_path),
                                    &rendered,
                                    live_spec,
                                    cancel,
                                )
                                .map_err(err)?;
                                rendered
                            } else {
                                PathBuf::from(&track.file_path)
                            }
                        }
                        .as_path(),
                        &stage
                            .0
                            .join(format!("{}.{}", track.stem_type, options.format)),
                        &options,
                        cancel,
                    )
                    .map_err(err)?;
                let temporary = stage.0.join(format!(".pitch-{}.wav", track.stem_type));
                if temporary.exists() {
                    fs::remove_file(temporary).map_err(err)?;
                }
            }
            check_cancel(cancel)?;
            let target = destination.join(format!("Atomusicator-export-{}", unique_id()));
            fs::write(stage.0.join("export.json"),serde_json::to_vec_pretty(&serde_json::json!({
                "version":1,"source_session_id":loaded.dto.id,
                "live_pitch":{"semitones":live_spec.semitones(),"preserve_formants":live_spec.preserve_formants(),"engine":atomusicator_infrastructure::live_pitch::ENGINE_ID},
                "format":options.format,"sample_rate":options.sample_rate,"channels":options.channels,"bit_depth":options.bit_depth,"normalize":options.normalize,
                "selected_stems":selected
            })).map_err(err)?).map_err(err)?;
            backend.jobs.begin_commit(id)?;
            fs::rename(&stage.0, &target).map_err(err)?;
            backend.jobs.output(id, target)?;
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
            failure: if self.mixer.device_failed() {
                Some(UiPlaybackFailure::DeviceUnavailable)
            } else if self.mixer.source_failed() {
                Some(UiPlaybackFailure::StemUnreadable)
            } else {
                None
            },
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
pub fn parse_stem(key: &str) -> Result<StemType, String> {
    StemType::ALL
        .into_iter()
        .find(|k| stem_key(*k) == key)
        .ok_or_else(|| "unknown stem type".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::UiJobStatus;
    use atomusicator_domain::{AudioBufferF32, AudioFormat};
    use atomusicator_infrastructure::audio_io::wav_codec::write_wav_f32;
    use std::time::{Duration, Instant};
    pub(super) fn backend(root: &Path) -> Arc<Studio> {
        Arc::new(Studio {
            mixer: CpalRealtimeMixer::new().unwrap(),
            jobs: Arc::new(crate::jobs::JobRegistry::new(
                atomusicator_infrastructure::job_journal::JobJournal::new(root.join("jobs")),
            )),
            loaded: Mutex::new(None),
            projects: Mutex::new(root.to_path_buf()),
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap(),
            sessions: Arc::new(OrtSessionPool::new(2).unwrap()),
            pitch_cache: PitchRenderCache::new(root.join("pitch-cache")),
            live_pitch_store: atomusicator_infrastructure::live_pitch_store::LivePitchStore::new(
                root.join("live-pitch"),
            )
            .unwrap(),
            resources: atomusicator_infrastructure::resource_probe::NativeResourceAdmission::new(
                atomusicator_application::compute_policy::ComputeBudget::interactive(
                    std::thread::available_parallelism().map_or(1, usize::from),
                )
                .threads_per_inference(),
            )
            .unwrap(),
        })
    }
    fn wait(backend: &Studio, id: &str) -> UiJobDto {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let job = backend.job(id).unwrap();
            if !matches!(
                job.status,
                UiJobStatus::Running | UiJobStatus::Cancelling | UiJobStatus::Committing
            ) {
                return job;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    pub(super) fn legacy_session(root: &Path) {
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
    fn live_pitch_is_not_a_job_and_survives_reopen_without_changing_originals() {
        let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        legacy_session(&root.0);
        let backend = backend(&root.0);
        let session = backend.open(&root.0).unwrap();
        let original = fs::read(&session.tracks[0].file_path).unwrap();
        let status = backend.live_pitch_status().unwrap();
        let accepted = backend
            .set_live_pitch(&session.id, status.epoch, 1, 7, false)
            .unwrap();
        assert_eq!(accepted.semitones, 7);
        assert!(backend.jobs.idle().is_ok());
        assert_eq!(backend.open(&root.0).unwrap().pitch_semitones, 7);
        let restored = backend.live_pitch_status().unwrap();
        assert!(!restored.preserve_formants);
        assert!(restored.epoch > status.epoch);
        assert!(
            backend
                .set_live_pitch(&session.id, status.epoch, 2, -7, true)
                .is_err()
        );
        assert_eq!(fs::read(&session.tracks[0].file_path).unwrap(), original);
    }
    #[test]
    #[ignore = "requires the bundled FFmpeg runtime"]
    fn live_export_freezes_clicked_settings_while_playback_settings_keep_changing() {
        let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        legacy_session(&root.0);
        let backend = backend(&root.0);
        let session = backend.open(&root.0).unwrap();
        let status = backend.live_pitch_status().unwrap();
        let id = backend
            .export_with_pitch(
                root.0.clone(),
                vec!["bass".into()],
                ExportOptions::new("wav", 44100, 2, 32, false).unwrap(),
                Some(crate::api::UiPitchExportDto {
                    session_id: session.id.clone(),
                    epoch: status.epoch,
                    semitones: 7,
                    preserve_formants: true,
                }),
            )
            .unwrap();
        backend
            .set_live_pitch(&session.id, status.epoch, 1, -7, false)
            .unwrap();
        let job = wait(&backend, &id);
        assert_eq!(job.status, UiJobStatus::Completed, "{}", job.message);
        let info: serde_json::Value = serde_json::from_slice(
            &fs::read(PathBuf::from(job.output_directory.unwrap()).join("export.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(info["live_pitch"]["semitones"], 7.0);
        assert_eq!(backend.live_pitch_status().unwrap().semitones, -7);
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
        assert_eq!(wait(&backend, &first).status, UiJobStatus::Cancelled);
        let failed = backend
            .start("failure", |_, _, _| Err("broken codec".into()))
            .unwrap();
        let job = wait(&backend, &failed);
        assert_eq!(job.status, UiJobStatus::Failed);
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
        let track = &session.tracks[0];
        assert!(
            !backend
                .waveform_peaks(&session.id, &track.stem_type, &track.file_path, 4000)
                .unwrap()
                .is_empty()
        );
        assert!(
            backend
                .waveform_peaks(
                    "superseded-session",
                    &track.stem_type,
                    &track.file_path,
                    4000
                )
                .is_err()
        );
        assert!(
            backend
                .waveform_peaks(&session.id, &track.stem_type, "previous-pitch.wav", 4000)
                .is_err()
        );
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
    fn recovery_finishes_an_already_published_bundle_without_requiring_the_source() {
        use atomusicator_infrastructure::job_journal::JobRequest;
        let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        legacy_session(&root.0);
        let backend = backend(&root.0);
        let mut record = backend
            .jobs
            .journal
            .create(
                "crashed",
                JobRequest::Separation {
                    source: root.0.join("missing-source.flac"),
                    projects: root.0.clone(),
                    download_allowed: false,
                },
                None,
            )
            .unwrap();
        record.lifecycle.start().unwrap();
        record.lifecycle.begin_commit().unwrap();
        record.output_directory = Some(root.0.clone());
        backend.jobs.journal.save(&mut record).unwrap();
        let id = backend.resume("crashed").unwrap();
        let result = wait(&backend, &id);
        assert_eq!(result.status, UiJobStatus::Completed, "{}", result.message);
        assert_eq!(result.session.unwrap().id, "legacy");
        assert!(backend.jobs.journal.load("crashed").unwrap().discarded);
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
        assert_eq!(job.status, UiJobStatus::Completed, "{}", job.message);
        let pitched = job.session.unwrap();
        assert_eq!(pitched.pitch_semitones, 3);
        let render_path = PathBuf::from(&pitched.tracks[0].file_path);
        let modified = fs::metadata(&render_path).unwrap().modified().unwrap();
        let id = backend.pitch(3, true).unwrap();
        let reused = wait(&backend, &id);
        assert_eq!(reused.status, UiJobStatus::Completed, "{}", reused.message);
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
        assert_eq!(job.status, UiJobStatus::Completed, "{}", job.message);
        let directory = PathBuf::from(job.output_directory.unwrap());
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 2);
        let id = backend.pitch(0, false).unwrap();
        let job = wait(&backend, &id);
        assert_eq!(job.status, UiJobStatus::Completed, "{}", job.message);
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
        assert_eq!(job.status, UiJobStatus::Completed, "{}", job.message);
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
        // Simulate interruption after publishing, followed by a damaged final WAV.
        // Recovery must rebuild from verified checkpoints instead of retrying the
        // invalid published bundle forever or overwriting it in place.
        fs::write(path.join("outputs/lead_vocal.wav"), b"damaged").unwrap();
        let mut record = backend
            .jobs
            .journal
            .create(
                "damaged-publication",
                atomusicator_infrastructure::job_journal::JobRequest::Separation {
                    source: root.0.join("test.wav"),
                    projects: root.0.clone(),
                    download_allowed: false,
                },
                None,
            )
            .unwrap();
        record.lifecycle.start().unwrap();
        record.lifecycle.begin_commit().unwrap();
        record.output_directory = Some(path.clone());
        backend.jobs.journal.save(&mut record).unwrap();
        let retry = backend.resume("damaged-publication").unwrap();
        let recovered = wait(&backend, &retry);
        assert_eq!(
            recovered.status,
            UiJobStatus::Completed,
            "{}",
            recovered.message
        );
        let rebuilt = PathBuf::from(recovered.session.unwrap().directory);
        assert_ne!(rebuilt, path);
        assert_eq!(backend.open(&rebuilt).unwrap().tracks.len(), 5);
    }
}
