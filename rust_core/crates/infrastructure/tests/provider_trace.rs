use atomusicator_infrastructure::provider_trace::ProviderAssignment;

#[test]
fn registration_and_fences_do_not_count_as_accelerator_execution() {
    let trace = br#"[
        {"cat":"Session","name":"DirectMLExecutionProvider"},
        {"cat":"Node","name":"x_fence_before","args":{"provider":"DmlExecutionProvider"}},
        {"cat":"Node","name":"x_kernel_time","args":{"provider":"CPUExecutionProvider"}},
        {"cat":"Node","name":"y_kernel_time","args":{"provider":"DmlExecutionProvider"}}
    ]"#;
    let result = ProviderAssignment::from_json(trace).unwrap();
    assert_eq!(result.kernel_events["CPUExecutionProvider"], 1);
    assert_eq!(result.kernel_events["DmlExecutionProvider"], 1);
    assert!(result.directml_executed());
    assert!(
        !ProviderAssignment::from_json(br#"[]"#)
            .unwrap()
            .directml_executed()
    );
    assert!(ProviderAssignment::from_json(b"{}").is_err());
}
