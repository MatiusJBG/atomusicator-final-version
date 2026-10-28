use atomusicator_domain::{jobs::*, quality::*};

#[test]
fn stable_profile_records_every_effective_quality_decision() {
    let profile = SeparationProfile::compatibility();
    assert_eq!(profile.id, "native-v1");
    assert_eq!(profile.models.len(), 4);
    assert_eq!(profile.models[0].n_fft, 7680);
    assert!(profile.models[1].invert);
    assert_eq!(profile.vad_seconds, Some(8));
    assert_eq!(profile.vad_dbfs, Some(-65.0));
    assert_eq!(profile.reconstruction_tolerance, 1e-5);
    let json = serde_json::to_string(&profile).unwrap();
    assert_eq!(profile, serde_json::from_str(&json).unwrap());
    let candidate = SeparationProfile::context_candidate();
    assert_ne!(candidate, profile);
    assert_eq!(candidate.vad_seconds, None);
    assert_eq!(candidate.bleed_gate_fft, None);
}

#[test]
fn cancel_and_commit_have_an_explicit_linearization_point() {
    let mut job = JobLifecycle::new();
    assert!(job.complete().is_err());
    job.start().unwrap();
    job.request_cancel().unwrap();
    assert!(job.begin_commit().is_err());
    job.fail(FailureKind::Cancelled).unwrap();
    assert_eq!(job.status(), JobStatus::Cancelled);
    let mut committed = JobLifecycle::new();
    committed.start().unwrap();
    committed.begin_commit().unwrap();
    assert!(!committed.request_cancel().unwrap());
    committed.complete().unwrap();
    assert_eq!(committed.status(), JobStatus::Completed);
    assert!(committed.start().is_err());
}

#[test]
fn failure_and_interruption_are_distinct_from_success() {
    let mut job = JobLifecycle::new();
    job.start().unwrap();
    job.interrupt();
    assert_eq!(job.status(), JobStatus::Recoverable);
    assert!(!job.is_active());
    assert!(job.complete().is_err());
}
