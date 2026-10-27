//! OS observations for admission. Estimates do not replace process/RSS measurements.
use atomusicator_application::resource_admission::{ResourceLease, ResourcePool, Resources};
use atomusicator_domain::DomainError;
use std::sync::Arc;

/// Conservative CPU-only estimates for the four pinned profiles, pending per-host tuning.
pub struct NativeResourceAdmission {
    pool: Arc<ResourcePool>,
    _resident: ResourceLease,
    threads: u64,
}
impl NativeResourceAdmission {
    pub fn new(threads: usize) -> Result<Self, DomainError> {
        let threads = threads.max(1) as u64;
        let pool = Arc::new(ResourcePool::new(Resources {
            threads,
            memory_bytes: 4 * 1024 * 1024 * 1024,
        }));
        let resident = pool.reserve(Resources {
            threads: 0,
            memory_bytes: 1536 * 1024 * 1024,
        })?;
        Ok(Self {
            pool,
            _resident: resident,
            threads,
        })
    }
    pub fn reserve_separation(&self) -> Result<ResourceLease, DomainError> {
        ensure_memory(3 * 1024 * 1024 * 1024, 512 * 1024 * 1024)?;
        self.pool.reserve(Resources {
            threads: self.threads,
            memory_bytes: 1536 * 1024 * 1024,
        })
    }
}
pub fn available_memory_bytes() -> Result<u64, DomainError> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        // SAFETY: MEMORYSTATUSEX contains integer fields only; the API receives a valid,
        // initialized buffer of the documented size, live for the entire synchronous call.
        let mut state: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        state.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if unsafe { GlobalMemoryStatusEx(&mut state) } == 0 {
            return Err(failure(std::io::Error::last_os_error()));
        }
        Ok(state.ullAvailPhys)
    }
    #[cfg(target_os = "linux")]
    {
        let values = std::fs::read_to_string("/proc/meminfo").map_err(failure)?;
        values
            .lines()
            .find_map(|line| {
                line.strip_prefix("MemAvailable:")
                    .and_then(|value| value.split_whitespace().next())
                    .and_then(|value| value.parse::<u64>().ok())
            })
            .and_then(|value| value.checked_mul(1024))
            .ok_or_else(|| failure("available memory could not be observed"))
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err(failure(
            "memory admission has not been qualified on this OS",
        ))
    }
}
pub fn ensure_memory(
    estimated_working_set: u64,
    reserve_for_system: u64,
) -> Result<(), DomainError> {
    let available = available_memory_bytes()?.saturating_sub(reserve_for_system);
    if estimated_working_set > available {
        return Err(DomainError::InsufficientResources {
            resource: "physical memory bytes after system reserve".into(),
            requested: estimated_working_set,
            available,
        });
    }
    Ok(())
}
pub fn ensure_disk(path: &std::path::Path, needed: u64) -> Result<(), DomainError> {
    std::fs::create_dir_all(path).map_err(failure)?;
    let available = fs4::available_space(path).map_err(failure)?;
    if needed > available {
        return Err(DomainError::InsufficientResources {
            resource: "working disk bytes".into(),
            requested: needed,
            available,
        });
    }
    Ok(())
}
fn failure(e: impl std::fmt::Display) -> DomainError {
    DomainError::DspFailure(e.to_string())
}
