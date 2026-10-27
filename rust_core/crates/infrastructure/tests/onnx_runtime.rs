use std::path::Path;

use atomusicator_infrastructure::onnx::{ExecutionProviderKind, OnnxSessionRunner, OrtSessionPool};

#[test]
fn provider_order_requires_cpu_fallback() {
    assert!(OrtSessionPool::with_provider_order(1, vec![ExecutionProviderKind::Cuda]).is_err());
    assert!(
        OrtSessionPool::with_provider_order(
            1,
            vec![ExecutionProviderKind::Cpu, ExecutionProviderKind::Cuda]
        )
        .is_err()
    );

    let pool = OrtSessionPool::with_provider_order(
        1,
        vec![ExecutionProviderKind::Cuda, ExecutionProviderKind::Cpu],
    )
    .unwrap();
    assert_eq!(
        pool.provider_order(),
        &[ExecutionProviderKind::Cuda, ExecutionProviderKind::Cpu]
    );
}

#[test]
fn provider_pool_rejects_zero_capacity() {
    assert!(OrtSessionPool::new(0).is_err());
}

#[test]
fn default_runtime_requires_cpu_until_accelerators_are_qualified() {
    let pool = OrtSessionPool::new(2).unwrap();
    assert_eq!(pool.provider_order(), &[ExecutionProviderKind::Cpu]);
    assert!((1..=6).contains(&pool.intra_threads()));
}

#[test]
fn runner_rejects_tensor_shape_mismatch_before_loading_a_model() {
    let pool = std::sync::Arc::new(OrtSessionPool::new(1).unwrap());
    let runner = OnnxSessionRunner::new(pool);
    let error = runner
        .run_f32(Path::new("missing-model.onnx"), "input", &[1, 4], &[0.0])
        .unwrap_err();

    assert!(error.to_string().contains("describes 4 values"));
}

#[test]
fn runner_rejects_non_positive_tensor_dimensions_before_loading_a_model() {
    let pool = std::sync::Arc::new(OrtSessionPool::new(1).unwrap());
    let runner = OnnxSessionRunner::new(pool);
    let error = runner
        .run_f32(Path::new("missing-model.onnx"), "input", &[1, 0], &[])
        .unwrap_err();

    assert!(error.to_string().contains("positive dimensions"));
}

#[test]
fn runner_rejects_non_finite_tensor_values_before_loading_a_model() {
    let pool = std::sync::Arc::new(OrtSessionPool::new(1).unwrap());
    let runner = OnnxSessionRunner::new(pool);
    let error = runner
        .run_f32(Path::new("missing-model.onnx"), "input", &[1], &[f32::NAN])
        .unwrap_err();

    assert!(error.to_string().contains("non-finite values"));
}
