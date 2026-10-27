//! Append-only, compare-and-swap job snapshots. Unconfirmed files are never read.
use atomusicator_domain::{DomainError, jobs::JobLifecycle};
use fs4::fs_std::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobRequest {
    Separation {
        source: PathBuf,
        projects: PathBuf,
        download_allowed: bool,
    },
    Ephemeral {
        operation: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalRecord {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub retry_of: Option<String>,
    pub lifecycle: JobLifecycle,
    pub request: JobRequest,
    pub source_sha256: Option<String>,
    pub profile: atomusicator_domain::quality::SeparationProfile,
    pub message: String,
    pub output_directory: Option<PathBuf>,
    #[serde(default)]
    pub staging_directory: Option<PathBuf>,
    pub failure: Option<atomusicator_domain::jobs::FailureKind>,
    pub discarded: bool,
}

pub struct JobLease {
    _file: File,
}
pub struct JobJournal {
    root: PathBuf,
}
impl JobJournal {
    pub fn hash_source(
        path: &Path,
        cancel: &atomusicator_application::CancellationToken,
    ) -> Result<String, DomainError> {
        crate::storage::checksum::file_hash(path, cancel)
    }
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    fn directory(&self, id: &str) -> Result<PathBuf, DomainError> {
        if id.is_empty()
            || id.len() > 160
            || !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        {
            return Err(failure("invalid job identifier"));
        }
        Ok(self.root.join(id))
    }
    pub fn create(
        &self,
        id: &str,
        request: JobRequest,
        retry_of: Option<String>,
    ) -> Result<JournalRecord, DomainError> {
        self.create_leased(id, request, retry_of, None)
            .map(|(record, _lease)| record)
    }
    /// Acquire ownership before making the first recoverable revision visible.
    pub fn create_leased(
        &self,
        id: &str,
        request: JobRequest,
        retry_of: Option<String>,
        source_sha256: Option<String>,
    ) -> Result<(JournalRecord, JobLease), DomainError> {
        fs::create_dir_all(&self.root).map_err(failure)?;
        let directory = self.directory(id)?;
        fs::create_dir(&directory).map_err(failure)?;
        let lease = self.lease(id)?;
        let record = JournalRecord {
            schema_version: 1,
            id: id.into(),
            revision: 0,
            retry_of,
            lifecycle: JobLifecycle::new(),
            request,
            source_sha256,
            profile: atomusicator_domain::quality::SeparationProfile::compatibility(),
            message: "Queued".into(),
            output_directory: None,
            staging_directory: None,
            failure: None,
            discarded: false,
        };
        Self::publish(&directory, &record)?;
        Ok((record, lease))
    }
    pub fn load(&self, id: &str) -> Result<JournalRecord, DomainError> {
        let directory = self.directory(id)?;
        let revision =
            latest_revision(&directory)?.ok_or_else(|| failure("job has no confirmed revision"))?;
        let path = directory.join(format!("{revision:020}.json"));
        if fs::metadata(&path).map_err(failure)?.len() > 4 * 1024 * 1024 {
            return Err(failure("job revision is too large"));
        }
        let record: JournalRecord =
            serde_json::from_slice(&fs::read(path).map_err(failure)?).map_err(failure)?;
        if record.schema_version != 1 || record.id != id || record.revision != revision {
            return Err(failure("job revision identity/version mismatch"));
        }
        Ok(record)
    }
    pub fn save(&self, record: &mut JournalRecord) -> Result<(), DomainError> {
        let directory = self.directory(&record.id)?;
        let _lock = locked(&directory.join("revision.lock"))?;
        let latest =
            latest_revision(&directory)?.ok_or_else(|| failure("job has no confirmed revision"))?;
        if latest != record.revision {
            return Err(failure("stale job revision"));
        }
        let mut next = record.clone();
        next.revision = latest
            .checked_add(1)
            .ok_or_else(|| failure("job revision overflow"))?;
        Self::publish(&directory, &next)?;
        *record = next;
        Ok(())
    }
    fn publish(directory: &Path, record: &JournalRecord) -> Result<(), DomainError> {
        let pending = directory.join(format!("{}.pending", crate::audio_tools::unique_id()));
        let bytes = serde_json::to_vec(record).map_err(failure)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)
            .map_err(failure)?;
        file.write_all(&bytes).map_err(failure)?;
        file.sync_all().map_err(failure)?;
        drop(file);
        fs::rename(
            &pending,
            directory.join(format!("{:020}.json", record.revision)),
        )
        .map_err(failure)
    }
    pub fn lease(&self, id: &str) -> Result<JobLease, DomainError> {
        self.try_lease(id)?
            .ok_or_else(|| failure("job is still active"))
    }
    fn try_lease(&self, id: &str) -> Result<Option<JobLease>, DomainError> {
        let path = self.directory(id)?.join("active.lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(failure)?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(JobLease { _file: file })),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.raw_os_error() == fs4::lock_contended_error().raw_os_error() =>
            {
                Ok(None)
            }
            Err(error) => Err(failure(error)),
        }
    }
    pub fn recoverable(&self) -> Result<Vec<JournalRecord>, DomainError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut records = Vec::new();
        let mut superseded = std::collections::BTreeSet::new();
        for entry in fs::read_dir(&self.root).map_err(failure)? {
            let entry = entry.map_err(failure)?;
            if !entry.file_type().map_err(failure)?.is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            let directory = self.directory(&id)?;
            if latest_revision(&directory)?.is_none() {
                continue;
            }
            let mut record = self.load(&id)?;
            if let Some(parent) = &record.retry_of {
                superseded.insert(parent.clone());
            }
            let Some(_lease) = self.try_lease(&id)? else {
                continue;
            };
            record = self.load(&id)?;
            record.lifecycle.interrupt();
            if !record.discarded
                && matches!(record.request, JobRequest::Separation { .. })
                && record.lifecycle.status() != atomusicator_domain::jobs::JobStatus::Completed
            {
                records.push(record);
            }
        }
        records.retain(|record| !superseded.contains(&record.id));
        records.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(records)
    }
    pub fn discard(&self, id: &str) -> Result<(), DomainError> {
        let _lease = self.lease(id)?;
        let mut record = self.load(id)?;
        self.cleanup_staging(&record)?;
        record.discarded = true;
        self.save(&mut record)
    }
    pub fn cleanup_staging(&self, record: &JournalRecord) -> Result<(), DomainError> {
        let Some(path) = record
            .staging_directory
            .as_ref()
            .filter(|path| path.exists())
        else {
            return Ok(());
        };
        let JobRequest::Separation { projects, .. } = &record.request else {
            return Err(failure("unexpected staging owner"));
        };
        let directory = path.canonicalize().map_err(failure)?;
        let projects = projects.canonicalize().map_err(failure)?;
        if directory.parent() != Some(projects.as_path())
            || !directory
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(".atomusicator-"))
        {
            return Err(failure(
                "recovery staging is outside its owned project directory",
            ));
        }
        let marker = directory.join(".job-owner");
        if fs::metadata(&marker).map_err(failure)?.len() > 160
            || fs::read_to_string(marker).map_err(failure)? != record.id
        {
            return Err(failure("recovery staging ownership mismatch"));
        }
        fs::remove_dir_all(directory).map_err(failure)
    }
}
fn latest_revision(directory: &Path) -> Result<Option<u64>, DomainError> {
    let mut latest = None;
    for entry in fs::read_dir(directory).map_err(failure)? {
        let name = entry.map_err(failure)?.file_name();
        let name = name.to_string_lossy();
        if name.len() == 25
            && name.ends_with(".json")
            && name[..20].bytes().all(|c| c.is_ascii_digit())
        {
            let revision = name[..20].parse::<u64>().map_err(failure)?;
            latest = Some(latest.map_or(revision, |value: u64| value.max(revision)));
        }
    }
    Ok(latest)
}
fn locked(path: &Path) -> Result<File, DomainError> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(failure)?;
    file.lock_exclusive().map_err(failure)?;
    Ok(file)
}
fn failure(error: impl std::fmt::Display) -> DomainError {
    DomainError::WorkspaceFailure(error.to_string())
}
