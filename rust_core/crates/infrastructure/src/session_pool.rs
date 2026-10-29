//! Admission tests use lightweight sessions so concurrency invariants run in CI.
use crate::errors::InfrastructureError;
use lru::LruCache;
use std::{
    num::NonZeroUsize,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
};

enum State<T> {
    Loading,
    Ready(Arc<T>),
    Failed(String),
}
struct Slot<T> {
    state: Mutex<State<T>>,
    changed: Condvar,
}
pub(crate) struct SessionPool<T> {
    entries: Mutex<LruCache<PathBuf, Arc<Slot<T>>>>,
}
fn poisoned() -> InfrastructureError {
    InfrastructureError::OnnxRuntime("session pool lock was poisoned".into())
}
impl<T> SessionPool<T> {
    pub(crate) fn new(capacity: usize) -> Result<Self, InfrastructureError> {
        let capacity = NonZeroUsize::new(capacity).ok_or_else(|| {
            InfrastructureError::InvalidManifest(
                "ONNX session pool capacity must be positive".into(),
            )
        })?;
        Ok(Self {
            entries: Mutex::new(LruCache::new(capacity)),
        })
    }
    pub(crate) fn capacity(&self) -> Result<usize, InfrastructureError> {
        Ok(self.entries.lock().map_err(|_| poisoned())?.cap().get())
    }
    pub(crate) fn load(
        &self,
        key: PathBuf,
        loader: impl FnOnce() -> Result<T, String>,
    ) -> Result<Arc<T>, InfrastructureError> {
        let (slot, owner) = {
            let mut entries = self.entries.lock().map_err(|_| poisoned())?;
            if let Some(slot) = entries.get(&key) {
                (slot.clone(), false)
            } else {
                if entries.len() == entries.cap().get() {
                    let mut evict = None;
                    for (path, slot) in entries.iter().rev() {
                        let state = slot.state.lock().map_err(|_| poisoned())?;
                        if Arc::strong_count(slot) == 1
                            && matches!(&*state, State::Ready(value) if Arc::strong_count(value) == 1)
                        {
                            evict = Some(path.clone());
                            break;
                        }
                    }
                    let path = evict.ok_or(InfrastructureError::SessionCapacityBusy)?;
                    entries.pop(&path);
                }
                let slot = Arc::new(Slot {
                    state: Mutex::new(State::Loading),
                    changed: Condvar::new(),
                });
                entries.put(key.clone(), slot.clone());
                (slot, true)
            }
        };
        if owner {
            // No global pool lock is held while ORT reads or compiles the model.
            // Convert loader panics to failure so reservations never strand waiters.
            let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(loader))
                .unwrap_or_else(|_| Err("session loader panicked".into()));
            let failed = loaded.is_err();
            {
                let mut state = slot.state.lock().map_err(|_| poisoned())?;
                *state = match loaded {
                    Ok(value) => State::Ready(Arc::new(value)),
                    Err(error) => State::Failed(error),
                };
                slot.changed.notify_all();
            }
            if failed {
                let mut entries = self.entries.lock().map_err(|_| poisoned())?;
                if entries
                    .peek(&key)
                    .is_some_and(|current| Arc::ptr_eq(current, &slot))
                {
                    entries.pop(&key);
                }
            }
        }
        let mut state = slot.state.lock().map_err(|_| poisoned())?;
        loop {
            match &*state {
                State::Ready(value) => return Ok(value.clone()),
                State::Failed(error) => {
                    return Err(InfrastructureError::OnnxRuntime(error.clone()));
                }
                State::Loading => state = slot.changed.wait(state).map_err(|_| poisoned())?,
            }
        }
    }
    pub(crate) fn trim(&self, capacity: usize) -> Result<(), InfrastructureError> {
        let capacity = NonZeroUsize::new(capacity).ok_or_else(|| {
            InfrastructureError::InvalidManifest(
                "ONNX session pool capacity must be positive".into(),
            )
        })?;
        let mut entries = self.entries.lock().map_err(|_| poisoned())?;
        let mut idle = Vec::new();
        for (key, slot) in entries.iter() {
            let state = slot.state.lock().map_err(|_| poisoned())?;
            if Arc::strong_count(slot) == 1
                && matches!(&*state, State::Ready(value) if Arc::strong_count(value) == 1)
            {
                idle.push(key.clone());
            }
        }
        if entries.len() - idle.len() > capacity.get() {
            return Err(InfrastructureError::SessionCapacityBusy);
        }
        for key in idle {
            entries.pop(&key);
        }
        entries.resize(capacity);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Barrier,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn concurrent_misses_share_one_load_and_do_not_block_other_keys() {
        let pool = Arc::new(SessionPool::<usize>::new(2).unwrap());
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let calls = Arc::new(AtomicUsize::new(0));
        std::thread::scope(|scope| {
            let (p, e, r, c) = (
                pool.clone(),
                entered.clone(),
                release.clone(),
                calls.clone(),
            );
            let first = scope.spawn(move || {
                p.load(PathBuf::from("a"), || {
                    c.fetch_add(1, Ordering::SeqCst);
                    e.wait();
                    r.wait();
                    Ok(7)
                })
                .unwrap()
            });
            entered.wait();
            let p = pool.clone();
            let c = calls.clone();
            let waiter = scope.spawn(move || {
                p.load(PathBuf::from("a"), || {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok(8)
                })
                .unwrap()
            });
            // This must finish while a's loader is still blocked.
            let b = pool.load(PathBuf::from("b"), || Ok(9)).unwrap();
            assert_eq!(*b, 9);
            assert!(
                pool.trim(1).is_err(),
                "loading reservations count toward capacity"
            );
            release.wait();
            assert!(Arc::ptr_eq(&first.join().unwrap(), &waiter.join().unwrap()));
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn failed_load_releases_capacity_and_live_leases_prevent_eviction() {
        let pool = SessionPool::<usize>::new(1).unwrap();
        assert!(
            pool.load(PathBuf::from("a"), || Err("bad model".into()))
                .is_err()
        );
        let live = pool.load(PathBuf::from("a"), || Ok(1)).unwrap();
        assert!(pool.load(PathBuf::from("b"), || Ok(2)).is_err());
        drop(live);
        assert_eq!(*pool.load(PathBuf::from("b"), || Ok(2)).unwrap(), 2);
        pool.trim(1).unwrap();
        assert!(pool.trim(0).is_err());
    }

    #[test]
    fn loader_panic_does_not_strand_a_reservation() {
        let pool = SessionPool::<usize>::new(1).unwrap();
        assert!(
            pool.load(PathBuf::from("a"), || panic!("simulated loader failure"))
                .is_err()
        );
        assert_eq!(*pool.load(PathBuf::from("a"), || Ok(42)).unwrap(), 42);
    }
}
