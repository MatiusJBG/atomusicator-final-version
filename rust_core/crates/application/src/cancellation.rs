use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Small application-owned cancellation contract, keeping the application
/// crate independent of any async runtime.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
