//! Conservative admission, not a guarantee against allocations made by other processes.
#[derive(Clone, Copy, Debug)]
pub struct GpuBudget {
    pub dedicated_bytes: u64,
    pub software: bool,
    pub budget_bytes: u64,
    pub used_bytes: u64,
}
pub fn admission_reason(budget: Option<GpuBudget>, available_ram: u64) -> Option<&'static str> {
    let Some(budget) = budget else {
        return Some("GPU memory budget could not be observed");
    };
    if budget.software {
        return Some("software GPU adapters are not admitted");
    }
    // Dedicated GPU: measured ~400 MiB process RSS, while ~1.82 GiB lives in
    // video memory. Reserve the larger 1.5 GiB CPU-fallback set plus 512 MiB
    // system margin. UMA additionally reserves GPU backing in system RAM.
    let system_reserve = 2 * 1024 * 1024 * 1024;
    let gpu_reserve = 2560 * 1024 * 1024;
    let ram_required = if budget.dedicated_bytes < gpu_reserve {
        system_reserve + gpu_reserve
    } else {
        system_reserve
    };
    if available_ram < ram_required {
        return Some(if budget.dedicated_bytes < gpu_reserve {
            "DirectML shared-memory adapter requires at least 4.5 GiB available physical RAM"
        } else {
            "DirectML dedicated-memory adapter requires at least 2 GiB available physical RAM"
        });
    }
    if budget.budget_bytes.saturating_sub(budget.used_bytes) < gpu_reserve {
        return Some("GPU budget headroom is below the 2.5 GiB admission reserve");
    }
    None
}
