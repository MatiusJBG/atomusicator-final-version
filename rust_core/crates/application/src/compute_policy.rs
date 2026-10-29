//! A conservative admission policy, not a claim of benchmark-optimal threading.
use atomusicator_domain::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComputeProfile {
    Eco,
    #[default]
    Balanced,
    Throughput,
}
impl std::str::FromStr for ComputeProfile {
    type Err = DomainError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "eco" => Ok(Self::Eco),
            "balanced" => Ok(Self::Balanced),
            "throughput" => Ok(Self::Throughput),
            _ => Err(DomainError::DspFailure(
                "CPU profile must be eco, balanced or throughput".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ComputeBudget {
    threads: usize,
}
impl ComputeBudget {
    pub fn interactive(logical_cpus: usize) -> Self {
        Self::for_profile(ComputeProfile::Balanced, logical_cpus, false)
    }
    /// Conservative host limits. Playback changes apply at the next inference
    /// boundary; they never interrupt a running model or the audio callback.
    pub fn for_profile(profile: ComputeProfile, logical_cpus: usize, playing: bool) -> Self {
        let limit = match profile {
            ComputeProfile::Eco => 2,
            ComputeProfile::Balanced => 6,
            ComputeProfile::Throughput => 12,
        };
        let mut threads = logical_cpus.saturating_sub(2).clamp(1, limit);
        if playing {
            threads = Self::playback_limit(threads, logical_cpus);
        }
        Self { threads }
    }
    pub fn playback_limit(requested: usize, logical_cpus: usize) -> usize {
        requested
            .max(1)
            .div_ceil(2)
            .min(logical_cpus.saturating_sub(4).clamp(1, 6))
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
