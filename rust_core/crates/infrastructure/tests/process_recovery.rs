//! Real child termination at both publication boundaries; all data belongs to this test.
use atomusicator_application::{CancellationToken, ports::AudioBlockStore};
use atomusicator_domain::{
    AudioAsset, AudioAssetId, AudioAssetMetadata, AudioFormat, StemType, jobs::JobStatus,
};
use atomusicator_infrastructure::{
    audio_tools::TemporaryDirectory,
    block_workspace::{PublicationHooks, publish_blocks},
    job_journal::{JobJournal, JobRequest},
    session_reader::read_session_files,
    storage::float_spool::FloatSpoolStore,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct ChildOwner(Child);
impl Drop for ChildOwner {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn terminating_the_owner_preserves_source_and_recovers_pre_and_post_commit() {
    for boundary in ["decode", "before", "after"] {
        let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "journal_child_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("ATOMUSICATOR_RECOVERY_FIXTURE", &root.0)
            .env("ATOMUSICATOR_RECOVERY_BOUNDARY", boundary)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW for the isolated test child.
        }
        let mut child = ChildOwner(command.spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(15);
        while !root.0.join("ready").exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "fixture exited before reaching {boundary}"
            );
            assert!(Instant::now() < deadline, "fixture deadline: {boundary}");
            std::thread::sleep(Duration::from_millis(5));
        }
        let journal = JobJournal::new(root.0.join("jobs"));
        assert!(
            journal.recoverable().unwrap().is_empty(),
            "live owner must not be offered for recovery"
        );
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let recovered = journal.recoverable().unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].lifecycle.status(), JobStatus::Recoverable);
        let source = root.0.join("original.txt");
        assert_eq!(fs::read(&source).unwrap(), b"owned original must survive");
        assert_eq!(
            recovered[0].source_sha256.as_ref().unwrap(),
            &JobJournal::hash_source(&source, &CancellationToken::default()).unwrap()
        );
        let published = root.0.join("projects/attempt");
        if boundary == "decode" {
            assert!(!published.exists());
        } else if boundary == "before" {
            assert!(!published.exists());
            let staging = recovered[0].staging_directory.as_ref().unwrap();
            assert!(staging.exists());
            journal.cleanup_staging(&recovered[0]).unwrap();
            assert!(!staging.exists());
        } else {
            assert_eq!(
                read_session_files(&published, &CancellationToken::default())
                    .unwrap()
                    .stems
                    .len(),
                5
            );
            journal.cleanup_staging(&recovered[0]).unwrap();
            assert!(published.join("session.json").exists());
        }
        let work = recovered[0].work_directory.as_ref().unwrap();
        journal.cleanup_staging(&recovered[0]).unwrap();
        assert!(
            !work.exists(),
            "decoded work must be cleaned even after a publication crash"
        );
        journal.discard("attempt").unwrap();
        assert!(journal.recoverable().unwrap().is_empty());
    }
}

#[test]
#[ignore = "internal subprocess fixture; exercised by terminating_the_owner"]
fn journal_child_fixture() {
    let Some(root) = std::env::var_os("ATOMUSICATOR_RECOVERY_FIXTURE") else {
        return;
    };
    let root = PathBuf::from(root);
    let boundary = std::env::var("ATOMUSICATOR_RECOVERY_BOUNDARY").unwrap();
    let source = root.join("original.txt");
    fs::write(&source, b"owned original must survive").unwrap();
    let projects = root.join("projects");
    fs::create_dir(&projects).unwrap();
    let cancel = CancellationToken::default();
    let journal = JobJournal::new(root.join("jobs"));
    let (mut record, _lease) = journal
        .create_leased(
            "attempt",
            JobRequest::Separation {
                source: source.clone(),
                projects: projects.clone(),
                download_allowed: false,
            },
            None,
            Some(JobJournal::hash_source(&source, &cancel).unwrap()),
        )
        .unwrap();
    record.lifecycle.start().unwrap();
    record.output_directory = Some(projects.join("attempt"));
    journal.save(&mut record).unwrap();
    let work = TemporaryDirectory::for_job(&projects, "attempt").unwrap();
    record.work_directory = Some(work.0.clone());
    journal.save(&mut record).unwrap();
    fs::write(work.0.join("decoded.f32"), b"owned decoded spool").unwrap();
    if boundary == "decode" {
        park(&root);
    }
    let record = RefCell::new(record);
    let store = FloatSpoolStore::new(root.join("spools"));
    let mut writer = store
        .create(
            "fixture",
            AudioAssetMetadata {
                format: AudioFormat::new(44100, 2).unwrap(),
                frame_count: 8,
            },
        )
        .unwrap();
    writer.write(&[0.1; 16]).unwrap();
    let audio = writer.finish().unwrap();
    let stems: BTreeMap<_, _> = StemType::ALL
        .into_iter()
        .map(|kind| (kind, audio.clone()))
        .collect();
    let asset = AudioAsset {
        id: AudioAssetId::new("fixture").unwrap(),
        path: source,
        display_name: "test source".into(),
    };
    publish_blocks(
        &projects,
        "attempt",
        &asset,
        &stems,
        serde_json::json!({}),
        &cancel,
        PublicationHooks {
            staged: |path: &std::path::Path| {
                let mut record = record.borrow_mut();
                record.staging_directory = Some(path.to_owned());
                journal.save(&mut record).map_err(|e| e.to_string())
            },
            commit: || {
                if boundary == "before" {
                    park(&root);
                }
                let mut record = record.borrow_mut();
                record.lifecycle.begin_commit().unwrap();
                journal.save(&mut record).map_err(|e| e.to_string())
            },
        },
    )
    .unwrap();
    park(&root);
}
fn park(root: &std::path::Path) -> ! {
    fs::write(root.join("ready"), b"boundary reached").unwrap();
    loop {
        std::thread::sleep(Duration::from_millis(100));
    }
}
