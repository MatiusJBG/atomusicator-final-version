//! Background job ownership and durable transitions. Audio composition lives in Studio.
use crate::api::{UiJobDto, UiJobFailure, UiJobStatus, UiSessionDto};
use atomusicator_application::CancellationToken;
use atomusicator_domain::jobs::{FailureKind, JobStatus};
use atomusicator_infrastructure::{
    audio_tools::unique_id,
    job_journal::{JobJournal, JobRequest, JournalRecord},
};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Debug)]
pub(crate) struct JobFailure {
    pub kind: FailureKind,
    pub message: String,
}
impl JobFailure {
    pub fn new(kind: FailureKind, message: impl std::fmt::Display) -> Self {
        Self {
            kind,
            message: message.to_string(),
        }
    }
    pub fn from_domain(error: atomusicator_domain::DomainError) -> Self {
        use atomusicator_domain::DomainError;
        let kind = match &error {
            DomainError::InsufficientResources { .. } => FailureKind::InsufficientResources,
            DomainError::SeparationCancelled => FailureKind::Cancelled,
            DomainError::CacheFailure(_) => FailureKind::InvalidArtifact,
            DomainError::WorkspaceFailure(_) => FailureKind::Storage,
            DomainError::AudioDecodeFailure(_) => FailureKind::SourceUnavailable,
            _ => FailureKind::Processing,
        };
        Self::new(kind, error)
    }
}
impl From<String> for JobFailure {
    fn from(message: String) -> Self {
        Self {
            kind: FailureKind::Processing,
            message,
        }
    }
}
impl From<&str> for JobFailure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

struct Job {
    dto: UiJobDto,
    record: JournalRecord,
    cancel: CancellationToken,
}
pub(crate) struct IdleGuard<'a> {
    _guard: MutexGuard<'a, Option<Job>>,
}
pub(crate) struct JobRegistry {
    active: Mutex<Option<Job>>,
    pub journal: JobJournal,
}
impl JobRegistry {
    pub fn new(journal: JobJournal) -> Self {
        Self {
            active: Mutex::new(None),
            journal,
        }
    }
    pub fn idle(&self) -> Result<IdleGuard<'_>, String> {
        let guard = self.active.lock().map_err(err)?;
        if guard
            .as_ref()
            .is_some_and(|j| j.record.lifecycle.is_active())
        {
            return Err("another audio job is already running".into());
        }
        Ok(IdleGuard { _guard: guard })
    }
    pub fn snapshot(&self, id: &str) -> Result<UiJobDto, String> {
        self.active
            .lock()
            .map_err(err)?
            .as_ref()
            .filter(|j| j.dto.id == id)
            .map(|j| j.dto.clone())
            .ok_or("job not found".into())
    }
    pub fn cancel(&self, id: &str) -> Result<(), String> {
        let mut guard = self.active.lock().map_err(err)?;
        let job = guard
            .as_mut()
            .filter(|j| j.dto.id == id)
            .ok_or("job not found")?;
        let mut next = job.record.clone();
        if next.lifecycle.request_cancel().map_err(err)? {
            self.journal.save(&mut next).map_err(err)?;
            job.record = next;
            job.dto.status = status(job.record.lifecycle.status());
            job.cancel.cancel();
        }
        Ok(())
    }
    pub fn start(
        self: &Arc<Self>,
        request: JobRequest,
        retry_of: Option<String>,
        operation: impl FnOnce(&str, &CancellationToken) -> Result<Option<UiSessionDto>, JobFailure>
        + Send
        + 'static,
    ) -> Result<String, String> {
        let id = unique_id();
        let cancel = CancellationToken::default();
        let mut slot = self.active.lock().map_err(err)?;
        if slot
            .as_ref()
            .is_some_and(|j| j.record.lifecycle.is_active())
        {
            return Err("another audio job is already running".into());
        }
        let expected_source = retry_of
            .as_ref()
            .map(|id| self.journal.load(id))
            .transpose()
            .map_err(err)?
            .and_then(|record| record.source_sha256);
        let (mut record, lease) = self
            .journal
            .create_leased(&id, request, retry_of.clone(), expected_source)
            .map_err(err)?;
        record.lifecycle.start().map_err(err)?;
        self.journal.save(&mut record).map_err(err)?;
        let dto = UiJobDto {
            id: id.clone(),
            status: UiJobStatus::Running,
            retry_of,
            failure: None,
            progress: 0.0,
            message: "Starting".into(),
            session: None,
            output_directory: None,
        };
        *slot = Some(Job {
            dto,
            record,
            cancel: cancel.clone(),
        });
        let registry = Arc::clone(self);
        let worker_id = id.clone();
        let spawned = std::thread::Builder::new()
            .name("atomusicator-job".into())
            .spawn(move || {
                let _lease = lease;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    operation(&worker_id, &cancel)
                }))
                .unwrap_or_else(|_| Err("native job failed unexpectedly".into()));
                registry.finish(&worker_id, result, _lease);
            });
        if let Err(error) = spawned {
            if let Some(job) = slot.as_mut() {
                job.record
                    .lifecycle
                    .fail(FailureKind::Processing)
                    .map_err(err)?;
                job.record.message = err(&error);
                self.journal.save(&mut job.record).map_err(err)?;
                job.dto.status = UiJobStatus::Failed;
                job.dto.failure = Some(UiJobFailure::Processing);
                job.dto.message = err(&error);
            }
            return Err(err(error));
        }
        Ok(id)
    }
    pub fn report(&self, id: &str, progress: f32, message: &str) {
        if let Ok(mut guard) = self.active.lock()
            && let Some(job) = guard.as_mut().filter(|j| j.dto.id == id)
        {
            job.dto.progress = job.dto.progress.max(progress.clamp(0.0, 1.0));
            job.dto.message = message.into();
        }
    }
    pub fn source_hash(&self, id: &str, hash: String) -> Result<(), String> {
        let mut guard = self.active.lock().map_err(err)?;
        let job = guard
            .as_mut()
            .filter(|j| j.dto.id == id)
            .ok_or("job not found")?;
        job.record.source_sha256 = Some(hash);
        self.journal.save(&mut job.record).map_err(err)
    }
    pub fn staging(&self, id: &str, path: std::path::PathBuf) -> Result<(), String> {
        let mut guard = self.active.lock().map_err(err)?;
        let job = guard
            .as_mut()
            .filter(|j| j.dto.id == id)
            .ok_or("job not found")?;
        job.record.staging_directory = Some(path);
        self.journal.save(&mut job.record).map_err(err)
    }
    pub fn begin_commit(&self, id: &str) -> Result<(), String> {
        let mut guard = self.active.lock().map_err(err)?;
        let job = guard
            .as_mut()
            .filter(|j| j.dto.id == id)
            .ok_or("job not found")?;
        if job.cancel.is_cancelled() {
            return Err("operation cancelled".into());
        }
        let mut next = job.record.clone();
        next.lifecycle.begin_commit().map_err(err)?;
        self.journal.save(&mut next).map_err(err)?;
        job.record = next;
        job.dto.status = UiJobStatus::Committing;
        Ok(())
    }
    pub fn output(&self, id: &str, path: std::path::PathBuf) -> Result<(), String> {
        let mut guard = self.active.lock().map_err(err)?;
        let job = guard
            .as_mut()
            .filter(|j| j.dto.id == id)
            .ok_or("job not found")?;
        job.record.output_directory = Some(path.clone());
        self.journal.save(&mut job.record).map_err(err)?;
        job.dto.output_directory = Some(path.to_string_lossy().into_owned());
        Ok(())
    }
    fn finish(
        &self,
        id: &str,
        result: Result<Option<UiSessionDto>, JobFailure>,
        lease: atomusicator_infrastructure::job_journal::JobLease,
    ) {
        let Ok(mut guard) = self.active.lock() else {
            return;
        };
        let Some(job) = guard.as_mut().filter(|j| j.dto.id == id) else {
            return;
        };
        let finish: Result<(), String> = (|| {
            match result {
                Ok(session) if !job.cancel.is_cancelled() => {
                    if job.record.lifecycle.status() == JobStatus::Running {
                        job.record.lifecycle.begin_commit().map_err(err)?;
                    }
                    job.record.lifecycle.complete().map_err(err)?;
                    job.dto.session = session;
                    job.dto.progress = 1.0;
                    job.record.message = "Completed".into();
                }
                result => {
                    let error = result.err().unwrap_or_else(|| "operation cancelled".into());
                    let reason = if job.cancel.is_cancelled() {
                        FailureKind::Cancelled
                    } else {
                        error.kind
                    };
                    job.record.lifecycle.fail(reason).map_err(err)?;
                    job.record.failure = Some(reason);
                    job.dto.failure = Some(failure(reason));
                    job.record.message = error.message;
                }
            }
            self.journal.save(&mut job.record).map_err(err)
        })();
        drop(lease);
        job.dto.status = status(job.record.lifecycle.status());
        job.dto.message = job.record.message.clone();
        if let Err(error) = finish {
            job.dto.status = UiJobStatus::Failed;
            job.dto.failure = Some(UiJobFailure::Storage);
            job.dto.message = format!("Job state could not be confirmed: {error}");
        }
    }
}
fn status(value: JobStatus) -> UiJobStatus {
    match value {
        JobStatus::Queued => UiJobStatus::Queued,
        JobStatus::Running => UiJobStatus::Running,
        JobStatus::Cancelling => UiJobStatus::Cancelling,
        JobStatus::Committing => UiJobStatus::Committing,
        JobStatus::Cancelled => UiJobStatus::Cancelled,
        JobStatus::Failed => UiJobStatus::Failed,
        JobStatus::Recoverable => UiJobStatus::Recoverable,
        JobStatus::Completed => UiJobStatus::Completed,
    }
}
fn failure(value: FailureKind) -> UiJobFailure {
    match value {
        FailureKind::Cancelled => UiJobFailure::Cancelled,
        FailureKind::SourceUnavailable => UiJobFailure::SourceUnavailable,
        FailureKind::InvalidArtifact => UiJobFailure::InvalidArtifact,
        FailureKind::InsufficientResources => UiJobFailure::InsufficientResources,
        FailureKind::Storage => UiJobFailure::Storage,
        FailureKind::DeviceUnavailable => UiJobFailure::DeviceUnavailable,
        FailureKind::Processing => UiJobFailure::Processing,
        FailureKind::Interrupted => UiJobFailure::Interrupted,
    }
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use atomusicator_infrastructure::audio_tools::TemporaryDirectory;
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    fn wait(registry: &JobRegistry, id: &str) -> UiJobDto {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let dto = registry.snapshot(id).unwrap();
            if !matches!(
                dto.status,
                UiJobStatus::Running | UiJobStatus::Cancelling | UiJobStatus::Committing
            ) {
                return dto;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn a_late_cancel_cannot_relabel_a_confirmed_publication_as_cancelled() {
        let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let registry = Arc::new(JobRegistry::new(JobJournal::new(root.0.clone())));
        let worker = Arc::clone(&registry);
        let (ready, reached) = mpsc::channel();
        let (finish, released) = mpsc::channel();
        let id = registry
            .start(
                JobRequest::Ephemeral {
                    operation: "commit-test".into(),
                },
                None,
                move |id, _| {
                    worker.begin_commit(id)?;
                    ready.send(()).unwrap();
                    released.recv().unwrap();
                    Ok(None)
                },
            )
            .unwrap();
        reached.recv_timeout(Duration::from_secs(5)).unwrap();
        registry.cancel(&id).unwrap();
        assert_eq!(
            registry.snapshot(&id).unwrap().status,
            UiJobStatus::Committing
        );
        finish.send(()).unwrap();
        assert_eq!(wait(&registry, &id).status, UiJobStatus::Completed);
        assert_eq!(
            registry.journal.load(&id).unwrap().lifecycle.status(),
            JobStatus::Completed
        );
    }
    #[test]
    fn cancellation_before_the_commit_point_prevents_publication() {
        let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let registry = Arc::new(JobRegistry::new(JobJournal::new(root.0.clone())));
        let worker = Arc::clone(&registry);
        let id = registry
            .start(
                JobRequest::Ephemeral {
                    operation: "cancel-test".into(),
                },
                None,
                move |id, cancel| {
                    while !cancel.is_cancelled() {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    worker.begin_commit(id)?;
                    panic!("cancelled job must not publish")
                },
            )
            .unwrap();
        registry.cancel(&id).unwrap();
        assert_eq!(wait(&registry, &id).status, UiJobStatus::Cancelled);
        assert_eq!(
            registry.journal.load(&id).unwrap().lifecycle.status(),
            JobStatus::Cancelled
        );
    }
}
