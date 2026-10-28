//! Legal job transitions, independent of threads, files and UI strings.
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Cancelling,
    Committing,
    Cancelled,
    Failed,
    Recoverable,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Cancelled,
    SourceUnavailable,
    InvalidArtifact,
    InsufficientResources,
    Storage,
    DeviceUnavailable,
    Processing,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid job transition from {from:?} to {to:?}")]
pub struct InvalidJobTransition {
    pub from: JobStatus,
    pub to: JobStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobLifecycle {
    status: JobStatus,
}
impl Default for JobLifecycle {
    fn default() -> Self {
        Self::new()
    }
}
impl JobLifecycle {
    pub fn new() -> Self {
        Self {
            status: JobStatus::Queued,
        }
    }
    pub fn status(&self) -> JobStatus {
        self.status
    }
    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            JobStatus::Queued | JobStatus::Running | JobStatus::Cancelling | JobStatus::Committing
        )
    }
    fn transition(&mut self, target: JobStatus, allowed: bool) -> Result<(), InvalidJobTransition> {
        if !allowed {
            return Err(InvalidJobTransition {
                from: self.status,
                to: target,
            });
        }
        self.status = target;
        Ok(())
    }
    pub fn start(&mut self) -> Result<(), InvalidJobTransition> {
        self.transition(JobStatus::Running, self.status == JobStatus::Queued)
    }
    pub fn request_cancel(&mut self) -> Result<bool, InvalidJobTransition> {
        if self.status == JobStatus::Committing || !self.is_active() {
            return Ok(false);
        }
        self.status = JobStatus::Cancelling;
        Ok(true)
    }
    pub fn begin_commit(&mut self) -> Result<(), InvalidJobTransition> {
        self.transition(JobStatus::Committing, self.status == JobStatus::Running)
    }
    pub fn complete(&mut self) -> Result<(), InvalidJobTransition> {
        self.transition(JobStatus::Completed, self.status == JobStatus::Committing)
    }
    pub fn fail(&mut self, reason: FailureKind) -> Result<(), InvalidJobTransition> {
        let target = if reason == FailureKind::Cancelled {
            JobStatus::Cancelled
        } else {
            JobStatus::Failed
        };
        self.transition(target, self.is_active())
    }
    pub fn interrupt(&mut self) {
        if self.is_active() {
            self.status = JobStatus::Recoverable;
        }
    }
}
