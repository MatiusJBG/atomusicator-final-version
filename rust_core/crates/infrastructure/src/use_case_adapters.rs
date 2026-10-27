use atomusicator_application::ports::{PhaseDspPort, SpectralBleedGatePort};
use atomusicator_domain::{AudioBufferF32, DomainError};

use crate::dsp::{PhaseSubtractor, SpectralBleedGate, StftProcessor};

/// Native implementation of the application's phase DSP boundary.
pub struct NativePhaseDsp {
    bleed_gate: StftSpectralBleedGate,
}

impl NativePhaseDsp {
    pub fn new(frame_size: usize, floor: f32, ceiling: f32) -> Result<Self, DomainError> {
        Ok(Self {
            bleed_gate: StftSpectralBleedGate::new(frame_size, floor, ceiling)?,
        })
    }
}

impl PhaseDspPort for NativePhaseDsp {
    fn subtract_phase(
        &self,
        minuend: &AudioBufferF32,
        subtrahend: &AudioBufferF32,
    ) -> Result<AudioBufferF32, DomainError> {
        PhaseSubtractor::subtract(minuend, subtrahend)
    }

    fn apply_spectral_bleed_gate(
        &self,
        lead_vocal: &AudioBufferF32,
        raw_backing_vocals: &AudioBufferF32,
    ) -> Result<(AudioBufferF32, AudioBufferF32), DomainError> {
        self.bleed_gate.rebalance(lead_vocal, raw_backing_vocals)
    }
}

pub struct StftSpectralBleedGate {
    processor: StftProcessor,
    floor: f32,
    ceiling: f32,
}

impl StftSpectralBleedGate {
    pub fn new(frame_size: usize, floor: f32, ceiling: f32) -> Result<Self, DomainError> {
        if !floor.is_finite() || !ceiling.is_finite() || floor < 0.0 || ceiling <= floor {
            return Err(DomainError::DspFailure(
                "bleed gate requires finite bounds with 0 <= floor < ceiling".into(),
            ));
        }
        Ok(Self {
            processor: StftProcessor::new(frame_size)?,
            floor,
            ceiling,
        })
    }
}

impl SpectralBleedGatePort for StftSpectralBleedGate {
    fn rebalance(
        &self,
        lead: &AudioBufferF32,
        backing: &AudioBufferF32,
    ) -> Result<(AudioBufferF32, AudioBufferF32), DomainError> {
        let mut lead_spectrum = self.processor.forward(lead)?;
        let mut backing_spectrum = self.processor.forward(backing)?;
        SpectralBleedGate::rebalance(
            &mut lead_spectrum,
            &mut backing_spectrum,
            self.floor,
            self.ceiling,
        )?;
        Ok((
            self.processor.inverse(&lead_spectrum)?,
            self.processor.inverse(&backing_spectrum)?,
        ))
    }
}
