use atomusicator_infrastructure::{
    audio_tools::TemporaryDirectory,
    job_diagnostics::{DiagnosticOptions, StageTimer, run_job},
};

#[test]
fn panic_flushes_json_summary_with_numeric_memory_fields() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let options = DiagnosticOptions::parse(None, None).unwrap();
    let failed = std::panic::catch_unwind(|| {
        run_job(&root.0, "panic", options, || panic!("expected panic"))
    });
    assert!(failed.is_err());
    let text = std::fs::read_to_string(root.0.join("panic.diagnostics.jsonl")).unwrap();
    let events: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let summary = events
        .iter()
        .find(|event| event["fields"]["message"] == "job diagnostic summary")
        .unwrap();
    assert_eq!(summary["fields"]["panicking"], true);
    if summary["fields"]["rss_observation_available"] == true {
        assert!(summary["fields"]["sampled_peak_rss_bytes"].is_u64());
    }
}
#[test]
fn diagnostics_are_scoped_to_one_job_and_remain_valid_after_failure() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let options = DiagnosticOptions::parse(None, None).unwrap();
    let result: Result<(), &str> = run_job(&root.0, "job-one", options, || {
        let _stage = StageTimer::new("decode");
        tracing::info!(provider = "CPUExecutionProvider", "test provider");
        Err("expected job failure")
    })
    .unwrap();
    assert_eq!(result.unwrap_err(), "expected job failure");
    let text = std::fs::read_to_string(root.0.join("job-one.diagnostics.jsonl")).unwrap();
    let events: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(events.iter().any(|v| v["fields"]["stage"] == "decode"));
    assert!(
        events
            .iter()
            .any(|v| v["fields"]["provider"] == "CPUExecutionProvider")
    );
    assert!(events.iter().any(|v| v["fields"]["memory_scope"]
        == "current process only; excludes child processes and VRAM"));
    assert!(DiagnosticOptions::parse(Some("invalid"), None).is_err());
    assert!(run_job(&root.0, "../escape", options, || ()).is_err());
    let off = DiagnosticOptions::parse(Some("false"), None).unwrap();
    assert_eq!(run_job(&root.0, "disabled", off, || 42).unwrap(), 42);
    assert!(!root.0.join("disabled.diagnostics.jsonl").exists());
}
