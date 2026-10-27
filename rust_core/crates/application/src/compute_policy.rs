//! A conservative admission policy, not a claim of benchmark-optimal threading.
use atomusicator_domain::DomainError;

#[derive(Debug, Clone, Copy)]
pub struct ComputeBudget {
    threads: usize,
}
impl ComputeBudget {
    pub fn interactive(logical_cpus: usize) -> Self {
        Self {
            threads: logical_cpus.saturating_sub(2).clamp(1, 6),
        }
    }
    pub fn threads_per_inference(self) -> usize {
        self.threads
    }
    pub fn threads_for_branches(self, branches: usize) -> Result<usize, DomainError> {
        if branches == 0 || branches > self.threads {
            return Err(DomainError::DspFailure(
                "branch count exceeds the compute budget".into(),
            ));
        }
        Ok(self.threads / branches)
    }
}
