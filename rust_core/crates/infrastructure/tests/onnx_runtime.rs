use std::path::Path;

use atomusicator_infrastructure::onnx::{ExecutionProviderKind, OnnxSessionRunner, OrtSessionPool};

#[test]
fn playback_limits_thread_budget_without_changing_requested_profile() {
    use atomusicator_application::compute_policy::ComputeProfile;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let playing = Arc::new(AtomicBool::new(false));
    let probe = playing.clone();
    let pool = OrtSessionPool::with_compute_profile(1, ComputeProfile::Throughput)
        .unwrap()
        .with_playback_probe(Arc::new(move || probe.load(Ordering::Acquire)));
    let requested = pool.intra_threads();
    assert_eq!(pool.effective_intra_threads(), requested);
    playing.store(true, Ordering::Release);
    assert!(pool.effective_intra_threads() <= 6);
    assert!(pool.effective_intra_threads() <= requested);
    assert_eq!(pool.intra_threads(), requested);
    playing.store(false, Ordering::Release);
    assert_eq!(pool.effective_intra_threads(), requested);
    pool.set_compute_profile(ComputeProfile::Eco);
    assert!(pool.intra_threads() <= 2);
}

#[test]
#[ignore = "requires locally cached ONNX model"]
fn playback_thread_change_rebuilds_real_session_and_preserves_capacity() {
    use atomusicator_application::compute_policy::ComputeProfile;
    use atomusicator_infrastructure::{
        models::manifest_manager::ManifestModelManager, storage::paths::WorkspacePathPolicy,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let policy = WorkspacePathPolicy::system_default().unwrap();
    let manager = ManifestModelManager::from_embedded_manifest(policy.clone()).unwrap();
    let model = policy
        .model_candidates(&manager.manifest().models[0].filename)
        .unwrap()
        .into_iter()
        .find(|path| path.is_file())
        .expect("cached model required");
    let playing = Arc::new(AtomicBool::new(false));
    let probe = playing.clone();
    let pool = OrtSessionPool::with_compute_profile(1, ComputeProfile::Throughput)
        .unwrap()
        .with_playback_probe(Arc::new(move || probe.load(Ordering::Acquire)));
    let first = pool.load_session(&model).unwrap();
    let weak = Arc::downgrade(&first);
    playing.store(true, Ordering::Release);
    if pool.effective_intra_threads() == pool.intra_threads() {
        return;
    }
    assert!(
        pool.load_session(&model).is_err(),
        "live old variant must count toward capacity"
    );
    drop(first);
    let second = pool.load_session(&model).unwrap();
    assert!(weak.upgrade().is_none(), "old workspace must be released");
    assert_eq!(pool.capacity().unwrap(), 1);
    assert_eq!(pool.diagnostics().len(), 2);
    drop(second);
}

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
fn benchmark_cpu_threads_are_explicit_positive_and_bounded() {
    assert!(OrtSessionPool::with_cpu_threads(2, 0).is_err());
    assert!(OrtSessionPool::with_cpu_threads(2, 13).is_err());
    let pool = OrtSessionPool::with_cpu_threads(2, 1).unwrap();
    assert_eq!(pool.intra_threads(), 1);
    assert_eq!(pool.provider_order(), &[ExecutionProviderKind::Cpu]);
}

#[test]
fn default_runtime_requires_cpu_until_accelerators_are_qualified() {
    let pool = OrtSessionPool::new(2).unwrap();
    assert_eq!(pool.provider_order(), &[ExecutionProviderKind::Cpu]);
    assert!((1..=6).contains(&pool.intra_threads()));
}

#[test]
#[ignore = "requires two locally cached MDX models; never downloads"]
fn live_session_leases_cannot_exceed_the_pool_capacity() {
    use atomusicator_infrastructure::{
        models::manifest_manager::ManifestModelManager, storage::paths::WorkspacePathPolicy,
    };
    let policy = WorkspacePathPolicy::system_default().unwrap();
    let manager = ManifestModelManager::from_embedded_manifest(policy.clone()).unwrap();
    let paths = manager
        .manifest()
        .models
        .iter()
        .take(2)
        .map(|profile| {
            policy
                .model_candidates(&profile.filename)
                .unwrap()
                .into_iter()
                .find(|path| path.is_file())
                .expect("cached model required")
        })
        .collect::<Vec<_>>();
    let pool = std::sync::Arc::new(OrtSessionPool::new(1).unwrap());
    let live = pool.load_session(&paths[0]).unwrap();
    let diagnostics = pool.diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].provider_effective.as_deref(),
        Some("CPUExecutionProvider")
    );
    assert!(diagnostics[0].fallback_errors.is_empty());
    assert!(diagnostics[0].initialization_seconds > 0.0);
    pool.trim_idle_and_resize(1).unwrap();
    assert_eq!(pool.capacity().unwrap(), 1);
    assert!(std::sync::Arc::ptr_eq(
        &live,
        &pool.load_session(&paths[0]).unwrap()
    ));
    assert!(
        pool.load_session(&paths[1]).is_err(),
        "evicting a leased Arc must not admit another live session"
    );
    drop(live);
    assert!(pool.load_session(&paths[1]).is_ok());
    pool.trim_idle_and_resize(1).unwrap();
    let first = pool.load_session(&paths[0]).unwrap();
    let first_weak = std::sync::Arc::downgrade(&first);
    drop(first);
    drop(pool.idle_scope());
    assert!(
        first_weak.upgrade().is_none(),
        "idle cached models must actually be released"
    );
    pool.trim_idle_and_resize(2).unwrap();
    let first = pool.load_session(&paths[0]).unwrap();
    let second = pool.load_session(&paths[1]).unwrap();
    assert!(
        pool.trim_idle_and_resize(1).is_err(),
        "cannot hide two live model allocations behind one slot"
    );
    assert_eq!(pool.capacity().unwrap(), 2);
    drop((first, second));
}

#[test]
fn memory_pressure_cannot_resize_the_session_cache_to_zero() {
    let pool = OrtSessionPool::new(2).unwrap();
    assert!(pool.trim_idle_and_resize(0).is_err());
    assert_eq!(pool.capacity().unwrap(), 2);
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

#[test]
fn borrowed_runner_validates_shape_and_finiteness_before_loading() {
    let runner = OnnxSessionRunner::new(std::sync::Arc::new(OrtSessionPool::new(1).unwrap()));
    let mut output = vec![9.0];
    assert!(
        runner
            .run_f32_into(
                Path::new("missing.onnx"),
                "input",
                &[1, 4],
                &mut [0.0],
                &mut output
            )
            .unwrap_err()
            .to_string()
            .contains("describes 4")
    );
    assert!(
        runner
            .run_f32_into(
                Path::new("missing.onnx"),
                "input",
                &[1],
                &mut [f32::NAN],
                &mut output
            )
            .unwrap_err()
            .to_string()
            .contains("non-finite")
    );
    assert_eq!(output, vec![9.0]);
}

#[test]
#[ignore = "requires the four verified local models; compares the borrowed path with ORT-owned tensors"]
fn borrowed_inputs_match_owned_ort_tensors_and_reuse_output_capacity() {
    use atomusicator_infrastructure::{
        mdx::MdxConfig, models::manifest_manager::ManifestModelManager,
        storage::paths::WorkspacePathPolicy,
    };
    let manager = ManifestModelManager::from_embedded_manifest(
        WorkspacePathPolicy::system_default().unwrap(),
    )
    .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let pool = std::sync::Arc::new(OrtSessionPool::with_cpu_threads(1, 2).unwrap());
    let runner = OnnxSessionRunner::new(pool.clone());
    for profile in &manager.manifest().models {
        let model = runtime
            .block_on(manager.cached_profile(&profile.profile_id))
            .unwrap()
            .unwrap();
        let c = MdxConfig::for_profile(&profile.profile_id).unwrap();
        let shape = vec![1, 4, c.dim_f as i64, c.dim_t as i64];
        let mut input: Vec<f32> = (0..4 * c.dim_f * c.dim_t)
            .map(|i| (i as f32 * 0.01).sin() * 0.1)
            .collect();
        let original = input.clone();
        let expected = {
            let session = pool.load_session(&model.local_path).unwrap();
            let session = session.lock().unwrap();
            let tensor: ort::value::Tensor<f32> =
                (shape.clone(), input.clone()).try_into().unwrap();
            let output = session
                .run(ort::inputs! {"input"=>tensor}.unwrap())
                .unwrap();
            output
                .iter()
                .next()
                .unwrap()
                .1
                .try_extract_tensor::<f32>()
                .unwrap()
                .iter()
                .copied()
                .collect::<Vec<_>>()
        };
        let mut output = Vec::new();
        runner
            .run_f32_into(&model.local_path, "input", &shape, &mut input, &mut output)
            .unwrap();
        assert_eq!(output, expected);
        assert_eq!(input, original);
        let allocation = output.as_ptr();
        runner
            .run_f32_into(&model.local_path, "input", &shape, &mut input, &mut output)
            .unwrap();
        assert_eq!(allocation, output.as_ptr());
        assert_eq!(output, expected);
    }
}
