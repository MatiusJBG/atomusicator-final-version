//! One latest-value setting, read without locks by the DSP producer.
use atomusicator_domain::{DomainError, PitchShiftSpec};
use std::sync::{
    Mutex,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

pub struct LivePitchControl {
    callback_frames: AtomicUsize,
    buffered_frames: AtomicUsize,
    target: AtomicU64,
    applied: AtomicU64,
    epoch: AtomicU64,
    writer: Mutex<()>,
}
#[derive(Debug, Clone, Copy)]
pub struct PitchSnapshot {
    pub epoch: u64,
    pub revision: u64,
    pub applied_revision: u64,
    pub spec: PitchShiftSpec,
}
impl Default for LivePitchControl {
    fn default() -> Self {
        Self {
            callback_frames: AtomicUsize::new(0),
            buffered_frames: AtomicUsize::new(0),
            target: AtomicU64::new(12 | 64),
            applied: AtomicU64::new(0),
            epoch: AtomicU64::new(0),
            writer: Mutex::new(()),
        }
    }
}
impl LivePitchControl {
    pub fn begin_output_probe(&self) {
        self.callback_frames.store(0, Ordering::Release);
        self.buffered_frames.store(0, Ordering::Release);
    }
    pub fn observe_output_buffer(&self, frames: usize) {
        self.callback_frames.fetch_max(frames, Ordering::Relaxed);
    }
    pub fn output_buffer_frames(&self) -> usize {
        self.callback_frames.load(Ordering::Acquire)
    }
    pub fn queue_target(&self) -> usize {
        let frames = self.output_buffer_frames();
        if frames == 0 {
            512
        } else {
            frames.saturating_add(crate::live_pitch::PITCH_BLOCK)
        }
    }
    pub fn update_buffered(&self, frames: usize) {
        self.buffered_frames.store(frames, Ordering::Release);
    }
    pub fn output_primed(&self) -> bool {
        self.output_buffer_frames() > 0
            && self.buffered_frames.load(Ordering::Acquire) >= self.queue_target()
    }
    pub fn snapshot(&self) -> PitchSnapshot {
        let word = self.target.load(Ordering::Acquire);
        PitchSnapshot {
            epoch: self.epoch.load(Ordering::Acquire),
            revision: word >> 8,
            applied_revision: self.applied.load(Ordering::Acquire),
            spec: PitchShiftSpec::try_new(((word & 63) as i32 - 12) as f32, word & 64 != 0)
                .expect("validated packed pitch"),
        }
    }
    pub fn set(
        &self,
        epoch: u64,
        revision: u64,
        spec: PitchShiftSpec,
    ) -> Result<PitchSnapshot, DomainError> {
        if spec.semitones().fract() != 0.0 {
            return Err(fail("interactive pitch uses integer semitones"));
        }
        let _writer = self
            .writer
            .lock()
            .map_err(|_| fail("pitch control state poisoned"))?;
        if epoch != self.epoch.load(Ordering::Acquire) {
            return Err(fail("pitch session changed; refresh the session"));
        }
        if revision == 0 || revision >= (1u64 << 56) {
            return Err(fail("invalid pitch revision"));
        }
        if revision > self.target.load(Ordering::Acquire) >> 8 {
            let word = (revision << 8)
                | (spec.semitones() as i32 + 12) as u64
                | if spec.preserve_formants() { 64 } else { 0 };
            self.target.store(word, Ordering::Release);
        }
        Ok(self.snapshot())
    }
    /// Called only after the previous producer/callback have been stopped.
    pub fn reset_session(&self) -> Result<(), DomainError> {
        let _writer = self
            .writer
            .lock()
            .map_err(|_| fail("pitch control state poisoned"))?;
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.target.store(12 | 64, Ordering::Release);
        self.applied.store(0, Ordering::Release);
        Ok(())
    }
    pub fn mark_applied(&self, revision: u64) {
        self.applied.fetch_max(revision, Ordering::Relaxed);
    }
}
fn fail(s: &str) -> DomainError {
    DomainError::DspFailure(s.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn priming_tracks_the_actual_callback_and_resets_between_devices() {
        let control = LivePitchControl::default();
        assert_eq!(control.queue_target(), 512);
        control.update_buffered(512);
        assert!(!control.output_primed());
        control.observe_output_buffer(1056);
        assert_eq!(control.queue_target(), 1312);
        control.update_buffered(1311);
        assert!(!control.output_primed());
        control.update_buffered(1312);
        assert!(control.output_primed());
        control.observe_output_buffer(512);
        assert_eq!(control.queue_target(), 1312);
        control.begin_output_probe();
        assert!(!control.output_primed());
        assert_eq!(control.queue_target(), 512);
    }
    #[test]
    fn latest_revision_wins_and_old_sessions_cannot_change_new_audio() {
        let control = LivePitchControl::default();
        control
            .set(0, 2, PitchShiftSpec::try_new(7.0, true).unwrap())
            .unwrap();
        control
            .set(0, 1, PitchShiftSpec::try_new(-7.0, false).unwrap())
            .unwrap();
        assert_eq!(control.snapshot().spec.semitones(), 7.0);
        control.reset_session().unwrap();
        assert!(
            control
                .set(0, 3, PitchShiftSpec::try_new(12.0, true).unwrap())
                .is_err()
        );
        assert_eq!(control.snapshot().spec.semitones(), 0.0);
    }
}
