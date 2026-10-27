use atomusicator_domain::jobs::{FailureKind, JobStatus};
use atomusicator_infrastructure::{
    audio_tools::TemporaryDirectory,
    job_journal::{JobJournal, JobRequest},
};

fn request() -> JobRequest {
    JobRequest::Separation {
        source: "song.flac".into(),
        projects: "projects".into(),
        download_allowed: false,
    }
}
#[test]
fn journal_ignores_unconfirmed_writes_and_rejects_stale_revisions() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let store = JobJournal::new(root.0.clone());
    let mut record = store.create("attempt-1", request(), None).unwrap();
    let mut stale = record.clone();
    record.lifecycle.start().unwrap();
    store.save(&mut record).unwrap();
    assert!(store.save(&mut stale).is_err());
    std::fs::write(root.0.join("attempt-1/incomplete.pending"), b"{").unwrap();
    assert_eq!(
        store.load("attempt-1").unwrap().lifecycle.status(),
        JobStatus::Running
    );
    assert!(store.create("../escape", request(), None).is_err());
}
#[test]
fn recovery_excludes_live_and_completed_jobs_and_retains_attempt_history() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let store = JobJournal::new(root.0.clone());
    let mut record = store.create("first", request(), None).unwrap();
    record.lifecycle.start().unwrap();
    store.save(&mut record).unwrap();
    let lease = store.lease("first").unwrap();
    assert!(store.recoverable().unwrap().is_empty());
    drop(lease);
    assert_eq!(
        store.recoverable().unwrap()[0].lifecycle.status(),
        JobStatus::Recoverable
    );
    record.lifecycle.fail(FailureKind::Processing).unwrap();
    store.save(&mut record).unwrap();
    let mut retry = store
        .create("second", request(), Some("first".into()))
        .unwrap();
    retry.lifecycle.start().unwrap();
    retry.lifecycle.begin_commit().unwrap();
    retry.lifecycle.complete().unwrap();
    store.save(&mut retry).unwrap();
    assert_eq!(
        store.load("second").unwrap().retry_of.as_deref(),
        Some("first")
    );
    store.discard("first").unwrap();
    assert!(store.recoverable().unwrap().is_empty());
}
#[test]
fn a_corrupt_confirmed_revision_is_reported_instead_of_using_an_older_success() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let store = JobJournal::new(root.0.clone());
    store.create("bad", request(), None).unwrap();
    std::fs::write(root.0.join("bad/00000000000000000001.json"), b"corrupt").unwrap();
    assert!(store.load("bad").is_err());
}
