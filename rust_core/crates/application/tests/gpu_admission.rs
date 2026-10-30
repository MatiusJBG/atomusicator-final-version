use atomusicator_application::gpu_admission::{GpuBudget, admission_reason};

#[test]
fn accelerator_requires_observed_headroom_and_hardware_adapter() {
    let good = GpuBudget {
        dedicated_bytes: 8 << 30,
        software: false,
        budget_bytes: 8 << 30,
        used_bytes: 1 << 30,
    };
    assert!(admission_reason(Some(good), 4 << 30).is_none());
    assert!(admission_reason(None, 4 << 30).is_some());
    assert!(
        admission_reason(
            Some(GpuBudget {
                software: true,
                ..good
            }),
            4 << 30
        )
        .is_some()
    );
    assert!(
        admission_reason(
            Some(GpuBudget {
                used_bytes: 8 << 30,
                ..good
            }),
            4 << 30
        )
        .is_some()
    );
    assert!(
        admission_reason(
            Some(GpuBudget {
                used_bytes: 9 << 30,
                ..good
            }),
            4 << 30
        )
        .is_some()
    );
    assert!(admission_reason(Some(good), 2 << 30).is_none());
    assert!(admission_reason(Some(good), (2 << 30) - 1).is_some());
    let shared = GpuBudget {
        dedicated_bytes: 512 << 20,
        ..good
    };
    assert!(admission_reason(Some(shared), 4 << 30).is_some());
    assert!(admission_reason(Some(shared), 8 << 30).is_none());
    assert!(admission_reason(Some(shared), (4 << 30) + (512 << 20)).is_none());
}
