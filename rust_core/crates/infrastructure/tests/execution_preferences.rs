use atomusicator_infrastructure::{
    audio_tools::TemporaryDirectory,
    preferences::{ExecutionMode, ExecutionOptions, ExecutionPreferences},
};
#[test]
fn execution_preferences_default_to_cpu_and_persist_explicit_modes() {
    use atomusicator_infrastructure::onnx::OrtSessionPool;
    let gpu = OrtSessionPool::new(2)
        .unwrap()
        .using_execution_options(ExecutionOptions {
            mode: ExecutionMode::DirectMl,
            device_id: 0,
        })
        .unwrap();
    assert_eq!(gpu.capacity().unwrap(), 1);
    let cpu = OrtSessionPool::new(2)
        .unwrap()
        .using_execution_options(ExecutionOptions::default())
        .unwrap();
    assert_eq!(cpu.capacity().unwrap(), 2);
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let preferences = ExecutionPreferences::new(root.0.clone());
    assert_eq!(preferences.load().unwrap().mode, ExecutionMode::Cpu);
    preferences
        .save(ExecutionOptions {
            mode: ExecutionMode::Auto,
            device_id: 1,
        })
        .unwrap();
    assert_eq!(
        preferences.load().unwrap(),
        ExecutionOptions {
            mode: ExecutionMode::Auto,
            device_id: 1
        }
    );
    assert!(
        preferences
            .save(ExecutionOptions {
                mode: ExecutionMode::DirectMl,
                device_id: u32::MAX
            })
            .is_err()
    );
    std::fs::write(
        root.0.join("execution.json"),
        br#"{"mode":"unknown","device_id":0}"#,
    )
    .unwrap();
    assert!(preferences.load().is_err());
}
