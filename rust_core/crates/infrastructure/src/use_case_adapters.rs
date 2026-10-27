use atomusicator_application::ports::SpectralBleedGatePort;
use atomusicator_domain::{AudioBufferF32, DomainError};

use crate::dsp::{SpectralBleedGate, StftProcessor};

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
