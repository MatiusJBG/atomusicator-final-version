//! Ordered MDX reconstruction shared by window scheduling strategies.
use crate::{
    job_diagnostics::StageTimer,
    mdx::{MdxConfig, MdxFftWorkspace, MdxTransform},
};
use atomusicator_application::ports::AudioBlockWriter;
use atomusicator_domain::DomainError;

pub type MdxObserver<'a> = dyn Fn(&str, &[f32]) + 'a;

#[derive(Clone, Copy)]
pub(super) struct OverlapLayout {
    pub frames: u64,
    pub padded: u64,
    pub step: usize,
    pub peak: f32,
}

#[derive(Clone, Copy)]
pub(super) struct WindowPosition {
    pub start: u64,
    pub actual: usize,
}

/// Only this ordered consumer mutates overlap weights or writes output.
pub(super) struct MdxOverlap<'a> {
    transform: &'a MdxTransform,
    config: MdxConfig,
    layout: OverlapLayout,
    accumulator: Vec<f32>,
    weights: Vec<f32>,
    decoded: Vec<f32>,
    output: Vec<f32>,
    uncompensated: Option<Vec<f32>>,
}

impl<'a> MdxOverlap<'a> {
    pub fn new(
        transform: &'a MdxTransform,
        config: MdxConfig,
        layout: OverlapLayout,
        tapped: bool,
    ) -> Self {
        let chunk = config.chunk_frames();
        Self {
            transform,
            config,
            layout,
            accumulator: vec![0.0; chunk * 2],
            weights: vec![0.0; chunk],
            decoded: vec![0.0; chunk * 2],
            output: Vec::with_capacity(layout.step * 2),
            uncompensated: tapped.then(|| Vec::with_capacity(layout.step * 2)),
        }
    }

    pub fn emit(
        &mut self,
        position: WindowPosition,
        raw: &[f32],
        prediction: &[f32],
        target: &mut dyn AudioBlockWriter,
        observe: Option<&MdxObserver<'_>>,
        workspace: &mut MdxFftWorkspace,
    ) -> Result<(), DomainError> {
        let WindowPosition { start, actual } = position;
        let c = self.config;
        let chunk = c.chunk_frames();
        let trim = c.n_fft / 2;
        {
            let _timer = StageTimer::new("fft_inverse");
            self.transform
                .inverse_into(prediction, &mut self.decoded, workspace)?;
        }
        if let Some(observe) = observe {
            observe("post_istft_normalized_windows", &self.decoded);
        }
        let ola_timer = StageTimer::new("overlap_add");
        for frame in 0..actual {
            let weight = if actual > 1 {
                0.5 - 0.5 * (2.0 * std::f32::consts::PI * frame as f32 / (actual - 1) as f32).cos()
            } else {
                1.0
            };
            self.weights[frame] += weight;
            for channel in 0..2 {
                self.accumulator[frame * 2 + channel] += self.decoded[frame * 2 + channel] * weight;
            }
        }
        let flush = self.layout.step.min((self.layout.padded - start) as usize);
        self.output.clear();
        if let Some(values) = &mut self.uncompensated {
            values.clear();
        }
        for frame in 0..flush {
            let position = start + frame as u64;
            if position < trim as u64 || position - trim as u64 >= self.layout.frames {
                continue;
            }
            if self.weights[frame] <= 0.0 {
                return Err(DomainError::DspFailure(
                    "MDX overlap left an uncovered frame".into(),
                ));
            }
            for channel in 0..2 {
                let restored =
                    self.accumulator[frame * 2 + channel] / self.weights[frame] * self.layout.peak;
                if let Some(values) = &mut self.uncompensated {
                    values.push(restored);
                }
                let value = restored * c.compensate;
                self.output.push(if c.invert {
                    raw[frame * 2 + channel] - value
                } else {
                    value
                });
            }
        }
        drop(ola_timer);
        if let Some(observe) = observe {
            observe(
                "post_ola_restore_pre_comp_pcm",
                self.uncompensated.as_ref().unwrap(),
            );
            observe("restored_compensated_pcm", &self.output);
        }
        {
            let _timer = StageTimer::new("spool_write");
            target.write(&self.output)?;
        }
        self.accumulator.copy_within(flush * 2.., 0);
        self.accumulator[(chunk - flush) * 2..].fill(0.0);
        self.weights.copy_within(flush.., 0);
        self.weights[chunk - flush..].fill(0.0);
        Ok(())
    }
}
