//! Bounded latest-value autosave. No disk operation runs in a pitch control call.
use atomusicator_domain::{DomainError, PitchShiftSpec};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
};

#[derive(Serialize, Deserialize)]
struct Saved {
    version: u32,
    engine: String,
    spec: PitchShiftSpec,
}
#[derive(Default)]
struct Pending {
    value: Option<(String, PitchShiftSpec)>,
    writing: bool,
    closing: bool,
    error: Option<String>,
}
pub struct LivePitchStore {
    root: PathBuf,
    state: Arc<(Mutex<Pending>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}
impl LivePitchStore {
    pub fn new(root: PathBuf) -> Result<Self, DomainError> {
        let state = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let shared = state.clone();
        let destination = root.clone();
        let worker = thread::Builder::new()
            .name("atomusicator-pitch-settings".into())
            .spawn(move || {
                let (lock, changed) = &*shared;
                loop {
                    let mut pending = match lock.lock() {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    while pending.value.is_none() && !pending.closing {
                        pending = match changed.wait(pending) {
                            Ok(v) => v,
                            Err(_) => return,
                        };
                    }
                    let Some((key, spec)) = pending.value.take() else {
                        return;
                    };
                    pending.writing = true;
                    drop(pending);
                    let result = write(&destination, &key, spec);
                    let mut pending = match lock.lock() {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    pending.writing = false;
                    pending.error = result.err().map(|e| e.to_string());
                    if let Some(error) = &pending.error {
                        eprintln!("Pitch settings autosave failed: {error}");
                    }
                    changed.notify_all();
                }
            })
            .map_err(fail)?;
        Ok(Self {
            root,
            state,
            worker: Some(worker),
        })
    }
    pub fn queue(&self, key: String, spec: PitchShiftSpec) -> Result<(), DomainError> {
        let mut pending = self
            .state
            .0
            .lock()
            .map_err(|_| fail("pitch autosave state poisoned"))?;
        pending.value = Some((key, spec));
        self.state.1.notify_one();
        Ok(())
    }
    /// Used when changing sessions, never on the audio or pitch control path.
    pub fn flush(&self) -> Result<(), DomainError> {
        let mut pending = self
            .state
            .0
            .lock()
            .map_err(|_| fail("pitch autosave state poisoned"))?;
        while pending.value.is_some() || pending.writing {
            pending = self
                .state
                .1
                .wait(pending)
                .map_err(|_| fail("pitch autosave state poisoned"))?;
        }
        if let Some(error) = pending.error.take() {
            return Err(fail(error));
        }
        Ok(())
    }
    pub fn load(&self, key: &str) -> Result<Option<PitchShiftSpec>, DomainError> {
        let file = self
            .root
            .join(format!("{:x}.json", Sha256::digest(key.as_bytes())));
        match std::fs::metadata(&file) {
            Ok(metadata) if metadata.len() > 4096 => {
                return Err(fail("pitch settings file is oversized"));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(fail(error)),
            _ => {}
        }
        let bytes = match std::fs::read(file) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(fail(e)),
        };
        let saved: Saved = serde_json::from_slice(&bytes).map_err(fail)?;
        if saved.version != 1 || saved.engine != crate::live_pitch::ENGINE_ID {
            return Ok(None);
        }
        if saved.spec.semitones().fract() != 0.0 {
            return Err(fail("saved interactive pitch is not an integer semitone"));
        }
        Ok(Some(saved.spec))
    }
}
impl Drop for LivePitchStore {
    fn drop(&mut self) {
        if let Ok(mut pending) = self.state.0.lock() {
            pending.closing = true;
            self.state.1.notify_one();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn write(root: &Path, key: &str, spec: PitchShiftSpec) -> Result<(), DomainError> {
    use std::io::Write;
    std::fs::create_dir_all(root).map_err(fail)?;
    let key = format!("{:x}", Sha256::digest(key.as_bytes()));
    let temp = root.join(format!(".{key}-{}.tmp", crate::audio_tools::unique_id()));
    let data = serde_json::to_vec(&Saved {
        version: 1,
        engine: crate::live_pitch::ENGINE_ID.into(),
        spec,
    })
    .map_err(fail)?;
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(fail)?;
        file.write_all(&data).map_err(fail)?;
        file.sync_all().map_err(fail)?;
        drop(file);
        std::fs::rename(&temp, root.join(format!("{key}.json"))).map_err(fail)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
fn fail(e: impl std::fmt::Display) -> DomainError {
    DomainError::WorkspaceFailure(e.to_string())
}
