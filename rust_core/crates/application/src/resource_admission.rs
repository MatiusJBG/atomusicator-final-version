//! Shared admission for estimated working sets; this is not an OS hard memory limit.
use atomusicator_domain::DomainError;
use std::sync::{Arc, Mutex};
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Resources {
    pub threads: u64,
    pub memory_bytes: u64,
}
pub struct ResourcePool {
    capacity: Resources,
    used: Mutex<Resources>,
}
pub struct ResourceLease {
    pool: Arc<ResourcePool>,
    resources: Resources,
}
impl ResourcePool {
    pub fn new(capacity: Resources) -> Self {
        Self {
            capacity,
            used: Mutex::new(Resources::default()),
        }
    }
    pub fn used(&self) -> Result<Resources, DomainError> {
        self.used
            .lock()
            .map(|used| *used)
            .map_err(|_| DomainError::DspFailure("resource admission state was poisoned".into()))
    }
    pub fn reserve(self: &Arc<Self>, requested: Resources) -> Result<ResourceLease, DomainError> {
        if requested.threads == 0 && requested.memory_bytes == 0 {
            return Err(DomainError::DspFailure("empty resource reservation".into()));
        }
        let mut used = self
            .used
            .lock()
            .map_err(|_| DomainError::DspFailure("resource admission state was poisoned".into()))?;
        for (resource, request, available) in [
            (
                "CPU threads",
                requested.threads,
                self.capacity.threads.saturating_sub(used.threads),
            ),
            (
                "working memory bytes",
                requested.memory_bytes,
                self.capacity.memory_bytes.saturating_sub(used.memory_bytes),
            ),
        ] {
            if request > available {
                return Err(DomainError::InsufficientResources {
                    resource: resource.into(),
                    requested: request,
                    available,
                });
            }
        }
        used.threads += requested.threads;
        used.memory_bytes += requested.memory_bytes;
        Ok(ResourceLease {
            pool: Arc::clone(self),
            resources: requested,
        })
    }
}
impl Drop for ResourceLease {
    fn drop(&mut self) {
        let mut used = self
            .pool
            .used
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        used.threads = used.threads.saturating_sub(self.resources.threads);
        used.memory_bytes = used
            .memory_bytes
            .saturating_sub(self.resources.memory_bytes);
    }
}
