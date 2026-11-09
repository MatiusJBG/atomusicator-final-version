//! OS observations for admission. Estimates do not replace process/RSS measurements.
use crate::onnx::OrtSessionPool;
use atomusicator_application::resource_admission::{ResourceLease, ResourcePool, Resources};
use atomusicator_domain::DomainError;
use std::sync::Arc;

pub const SYSTEM_RESERVE: u64 = 512 * 1024 * 1024;
pub const CACHED_WORKING_SET: u64 = 3 * 1024 * 1024 * 1024;
// Includes one model's ORT workspace and the bounded DSP buffers. The measured
// desktop native pipeline peaked at 1.36 GiB with two retained sessions; reserve
// 2 GiB after evicting idle sessions, without reducing FFT/model precision.
pub const SERIAL_WORKING_SET: u64 = 2 * 1024 * 1024 * 1024;
// Calibrated working set for mobile devices (single-session execution with bounded DSP buffer reuse)
pub const MOBILE_SERIAL_WORKING_SET: u64 = 800 * 1024 * 1024;
pub const MOBILE_CACHED_WORKING_SET: u64 = 1536 * 1024 * 1024;

/// Conservative CPU-only estimates for the four pinned profiles, pending per-host tuning.
pub struct NativeResourceAdmission {
    pool: Arc<ResourcePool>,
    _resident: ResourceLease,
    threads: u64,
    serial_working_set: u64,
    cached_working_set: u64,
}
impl NativeResourceAdmission {
    pub fn new(threads: usize) -> Result<Self, DomainError> {
        Self::with_working_sets(threads, SERIAL_WORKING_SET, CACHED_WORKING_SET)
    }

    pub fn for_mobile(threads: usize) -> Result<Self, DomainError> {
        Self::with_working_sets(
            threads,
            MOBILE_SERIAL_WORKING_SET,
            MOBILE_CACHED_WORKING_SET,
        )
    }

    pub fn with_limits(
        threads: usize,
        limits: atomusicator_application::compute_policy::ComputeLimits,
    ) -> Result<Self, DomainError> {
        if limits == atomusicator_application::compute_policy::ComputeLimits::mobile() {
            Self::for_mobile(threads)
        } else {
            Self::new(threads)
        }
    }

    pub fn with_working_sets(
        threads: usize,
        serial_working_set: u64,
        cached_working_set: u64,
    ) -> Result<Self, DomainError> {
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
            serial_working_set,
            cached_working_set,
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
        let is_mobile = sessions.compute_limits()
            == atomusicator_application::compute_policy::ComputeLimits::mobile();
        let cached_threshold = if is_mobile && self.cached_working_set == CACHED_WORKING_SET {
            MOBILE_CACHED_WORKING_SET
        } else {
            self.cached_working_set
        };
        let serial_threshold = if is_mobile && self.serial_working_set == SERIAL_WORKING_SET {
            MOBILE_SERIAL_WORKING_SET
        } else {
            self.serial_working_set
        };

        let available = probe()?.saturating_sub(SYSTEM_RESERVE);
        if available >= cached_threshold {
            return Ok(lease);
        }
        sessions.trim_idle_and_resize(1).map_err(failure)?;
        // Sample again after releasing cached ORT allocations. No stale-memory
        // observation can admit the compact plan if pressure has since increased.
        let available = probe()?.saturating_sub(SYSTEM_RESERVE);
        if available < serial_threshold {
            return Err(DomainError::InsufficientResources {
                resource: "physical memory bytes after system reserve".into(),
                requested: serial_threshold,
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
    #[cfg(any(target_os = "linux", target_os = "android"))]
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
    #[cfg(not(any(windows, target_os = "linux", target_os = "android")))]
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

    #[test]
    fn mobile_memory_probe_calibrates_lower_working_set() {
        use atomusicator_application::compute_policy::ComputeLimits;

        let admission = NativeResourceAdmission::for_mobile(2).unwrap();
        let sessions = OrtSessionPool::with_compute_profile_and_limits(
            1,
            atomusicator_application::compute_policy::ComputeProfile::Balanced,
            ComputeLimits::mobile(),
        )
        .unwrap();

        // 1 GiB available (less than 2 GiB desktop requirement, but fits mobile 800 MiB)
        let memory_1gb = 1024 * 1024 * 1024 + SYSTEM_RESERVE;
        let result = admission.reserve_separation_with_probe(&sessions, || Ok(memory_1gb));
        assert!(
            result.is_ok(),
            "mobile profile should admit separation when 1 GiB free memory is available"
        );

        // The admitted lease owns the CPU reservation; release it before the
        // next admission so the failure below is about memory, not contention.
        drop(result);
        // Below 800 MiB mobile threshold
        let memory_low = MOBILE_SERIAL_WORKING_SET + SYSTEM_RESERVE - 1;
        let error = admission
            .reserve_separation_with_probe(&sessions, || Ok(memory_low))
            .err()
            .unwrap();
        assert!(matches!(
            error,
            DomainError::InsufficientResources {
                requested,
                available,
                ..
            } if requested == MOBILE_SERIAL_WORKING_SET && available == MOBILE_SERIAL_WORKING_SET - 1
        ));
    }
}
