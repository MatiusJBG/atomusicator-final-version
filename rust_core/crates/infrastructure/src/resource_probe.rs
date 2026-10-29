//! OS observations for admission. Estimates do not replace process/RSS measurements.
use crate::onnx::OrtSessionPool;
use atomusicator_application::resource_admission::{ResourceLease, ResourcePool, Resources};
use atomusicator_domain::DomainError;
use std::sync::Arc;

const SYSTEM_RESERVE: u64 = 512 * 1024 * 1024;
const CACHED_WORKING_SET: u64 = 3 * 1024 * 1024 * 1024;
// Includes one model's ORT workspace and the bounded DSP buffers. The measured
// desktop native pipeline peaked at 1.36 GiB with two retained sessions; reserve
// 2 GiB after evicting idle sessions, without reducing FFT/model precision.
const SERIAL_WORKING_SET: u64 = 2 * 1024 * 1024 * 1024;

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
    pub fn reserve_separation(
        &self,
        sessions: &OrtSessionPool,
    ) -> Result<ResourceLease, DomainError> {
        self.reserve_separation_with_probe(sessions, available_memory_bytes)
    }
    fn reserve_separation_with_probe(
        &self,
        sessions: &OrtSessionPool,
        mut probe: impl FnMut() -> Result<u64, DomainError>,
    ) -> Result<ResourceLease, DomainError> {
        // Acquire all inference threads before touching the shared session cache.
        // A failed probe or admission drops the lease, allowing a safe retry.
        let lease = self.pool.reserve(Resources {
            threads: self.threads,
            memory_bytes: 1536 * 1024 * 1024,
        })?;
        let available = probe()?.saturating_sub(SYSTEM_RESERVE);
        if available >= CACHED_WORKING_SET {
            return Ok(lease);
        }
        sessions.trim_idle_and_resize(1).map_err(failure)?;
        // Sample again after releasing cached ORT allocations. No stale-memory
        // observation can admit the compact plan if pressure has since increased.
        let available = probe()?.saturating_sub(SYSTEM_RESERVE);
        if available < SERIAL_WORKING_SET {
            return Err(DomainError::InsufficientResources {
                resource: "physical memory bytes after system reserve".into(),
                requested: SERIAL_WORKING_SET,
                available,
            });
        }
        Ok(lease)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reported_memory_budget_admits_serial_low_memory_separation() {
        let admission = NativeResourceAdmission::new(2).unwrap();
        let sessions = OrtSessionPool::new(2).unwrap();
        let result =
            admission.reserve_separation_with_probe(&sessions, || Ok(2442457088 + SYSTEM_RESERVE));
        assert!(
            result.is_ok(),
            "reported RAM should fit the bounded one-model path"
        );
        assert_eq!(sessions.capacity().unwrap(), 1);
    }

    #[test]
    fn cached_path_keeps_two_sessions_with_sufficient_memory() {
        let admission = NativeResourceAdmission::new(2).unwrap();
        let sessions = OrtSessionPool::new(2).unwrap();
        let lease = admission
            .reserve_separation_with_probe(&sessions, || Ok(CACHED_WORKING_SET + SYSTEM_RESERVE))
            .unwrap();
        assert_eq!(sessions.capacity().unwrap(), 2);
        assert!(
            admission
                .reserve_separation_with_probe(&sessions, || Ok(u64::MAX))
                .is_err()
        );
        drop(lease);
        assert!(
            admission
                .reserve_separation_with_probe(&sessions, || Ok(u64::MAX))
                .is_ok()
        );
    }

    #[test]
    fn compact_path_rejects_insufficient_memory_and_releases_its_lease() {
        let admission = NativeResourceAdmission::new(2).unwrap();
        let sessions = OrtSessionPool::new(2).unwrap();
        let error = admission
            .reserve_separation_with_probe(
                &sessions,
                || Ok(SERIAL_WORKING_SET + SYSTEM_RESERVE - 1),
            )
            .err()
            .unwrap();
        assert!(
            matches!(error, DomainError::InsufficientResources { requested: SERIAL_WORKING_SET, available, .. } if available == SERIAL_WORKING_SET - 1)
        );
        assert!(
            admission
                .reserve_separation_with_probe(
                    &sessions,
                    || Ok(SERIAL_WORKING_SET + SYSTEM_RESERVE)
                )
                .is_ok()
        );
    }

    #[test]
    fn compact_path_rechecks_pressure_after_cache_eviction() {
        let admission = NativeResourceAdmission::new(2).unwrap();
        let sessions = OrtSessionPool::new(2).unwrap();
        let mut readings = [
            Ok(SERIAL_WORKING_SET + SYSTEM_RESERVE),
            Ok(SYSTEM_RESERVE - 1),
        ]
        .into_iter();
        let result =
            admission.reserve_separation_with_probe(&sessions, || readings.next().unwrap());
        assert!(matches!(
            result,
            Err(DomainError::InsufficientResources { available: 0, .. })
        ));
    }

    #[test]
    fn failed_memory_probe_never_keeps_the_cpu_reservation() {
        let admission = NativeResourceAdmission::new(2).unwrap();
        let sessions = OrtSessionPool::new(2).unwrap();
        assert!(
            admission
                .reserve_separation_with_probe(&sessions, || Err(failure("probe failed")))
                .is_err()
        );
        assert!(
            admission
                .reserve_separation_with_probe(&sessions, || Ok(u64::MAX))
                .is_ok()
        );
    }
}
