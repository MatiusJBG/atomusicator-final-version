use atomusicator_domain::{AudioBufferF32, DomainError};
use rayon::prelude::*;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex, num_complex::Complex32};
use std::sync::Arc;

pub struct StftProcessor {
    frame_size: usize,
    hop_size: usize,
    window: Vec<f32>,
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
}

#[derive(Debug, Clone)]
pub struct SpectrogramF32 {
    pub sample_rate_hz: u32,
    pub channels: u16,
    /// Original interleaved audio frame count (samples per channel).
    pub sample_count: usize,
    pub frame_size: usize,
    pub hop_size: usize,
    /// Frames contain concatenated channel spectra in channel-major order.
    pub bins: Vec<Vec<Complex32>>,
}

impl StftProcessor {
    pub fn new(frame_size: usize) -> Result<Self, DomainError> {
        if frame_size < 4 || !frame_size.is_power_of_two() {
            return Err(DomainError::DspFailure(
                "STFT frame size must be a power of two of at least four".into(),
            ));
        }
        let hop_size = frame_size / 2;
        let window = (0..frame_size)
            .map(|index| {
                0.5 - 0.5 * (2.0 * std::f32::consts::PI * index as f32 / frame_size as f32).cos()
            })
            .collect();
        let mut planner = RealFftPlanner::<f32>::new();
        Ok(Self {
            frame_size,
            hop_size,
            window,
            forward: planner.plan_fft_forward(frame_size),
            inverse: planner.plan_fft_inverse(frame_size),
        })
    }

    pub fn forward(&self, audio: &AudioBufferF32) -> Result<SpectrogramF32, DomainError> {
        let channels = usize::from(audio.format().channels);
        let sample_count = audio.frames();
        if sample_count == 0 {
            return Err(DomainError::DspFailure(
                "STFT cannot process an empty audio buffer".into(),
            ));
        }
        let frame_count = sample_count.div_ceil(self.hop_size) + 1;
        let bins_per_channel = self.frame_size / 2 + 1;
        let mut frames = Vec::with_capacity(frame_count);
        let mut scratch = self.forward.make_scratch_vec();
        for frame in 0..frame_count {
            let start = frame as isize * self.hop_size as isize - self.hop_size as isize;
            let mut combined = Vec::with_capacity(channels * bins_per_channel);
            for channel in 0..channels {
                let mut time = vec![0.0f32; self.frame_size];
                for (offset, value) in time.iter_mut().enumerate() {
                    let source_index = start + offset as isize;
                    if source_index >= 0 && (source_index as usize) < audio.frames() {
                        *value = audio.samples()[source_index as usize * channels + channel]
                            * self.window[offset];
                    }
                }
                let mut frequency = self.forward.make_output_vec();
                self.forward
                    .process_with_scratch(&mut time, &mut frequency, &mut scratch)
                    .map_err(|error| DomainError::DspFailure(error.to_string()))?;
                combined.extend(frequency);
            }
            frames.push(combined);
        }
        Ok(SpectrogramF32 {
            sample_rate_hz: audio.format().sample_rate_hz,
            channels: audio.format().channels,
            sample_count: audio.frames(),
            frame_size: self.frame_size,
            hop_size: self.hop_size,
            bins: frames,
        })
    }

    pub fn inverse(&self, spectrum: &SpectrogramF32) -> Result<AudioBufferF32, DomainError> {
        if spectrum.frame_size != self.frame_size || spectrum.hop_size != self.hop_size {
            return Err(DomainError::DspFailure(
                "spectrogram was produced with a different STFT configuration".into(),
            ));
        }
        atomusicator_domain::AudioFormat::new(spectrum.sample_rate_hz, spectrum.channels)?;
        if spectrum.sample_count == 0
            || spectrum.bins.len() != spectrum.sample_count.div_ceil(self.hop_size) + 1
        {
            return Err(DomainError::DspFailure(
                "spectrogram frame count does not match its original sample count".into(),
            ));
        }
        let channels = usize::from(spectrum.channels);
        let bins_per_channel = self.frame_size / 2 + 1;
        let output_len = spectrum
            .bins
            .len()
            .checked_add(1)
            .and_then(|frame_count| frame_count.checked_mul(self.hop_size))
            .ok_or_else(|| DomainError::DspFailure("iSTFT output size overflows usize".into()))?;
        let output_samples = output_len
            .checked_mul(channels)
            .ok_or_else(|| DomainError::DspFailure("iSTFT output size overflows usize".into()))?;
        let mut output = vec![0.0f32; output_samples];
        let mut normalization = vec![0.0f32; output_len];
        let mut time = self.inverse.make_output_vec();
        let mut scratch = self.inverse.make_scratch_vec();

        for (frame_index, frame) in spectrum.bins.iter().enumerate() {
            if frame.len() != channels * bins_per_channel {
                return Err(DomainError::DspFailure(
                    "spectrogram frame has an unexpected number of bins".into(),
                ));
            }
            if frame
                .iter()
                .any(|bin| !bin.re.is_finite() || !bin.im.is_finite())
            {
                return Err(DomainError::DspFailure(
                    "spectrogram contains a non-finite frequency bin".into(),
                ));
            }
            let start = frame_index as isize * self.hop_size as isize - self.hop_size as isize;
            for channel in 0..channels {
                let begin = channel * bins_per_channel;
                let end = begin + bins_per_channel;
                let mut frequency = frame[begin..end].to_vec();
                self.inverse
                    .process_with_scratch(&mut frequency, &mut time, &mut scratch)
                    .map_err(|error| DomainError::DspFailure(error.to_string()))?;
                for (offset, sample) in time.iter().enumerate().take(self.frame_size) {
                    let target = start + offset as isize;
                    if target < 0 || target as usize >= output_len {
                        continue;
                    }
                    let target = target as usize;
                    output[target * channels + channel] +=
                        sample / self.frame_size as f32 * self.window[offset];
                    if channel == 0 {
                        normalization[target] += self.window[offset] * self.window[offset];
                    }
                }
            }
        }

        for frame in 0..output_len {
            if normalization[frame] > f32::EPSILON {
                for channel in 0..channels {
                    output[frame * channels + channel] /= normalization[frame];
                }
            }
        }
        output.truncate(spectrum.sample_count * channels);
        AudioBufferF32::new(
            atomusicator_domain::AudioFormat::new(spectrum.sample_rate_hz, spectrum.channels)?,
            output,
        )
    }
}

pub struct PhaseSubtractor;

impl PhaseSubtractor {
    pub fn subtract(
        mix: &AudioBufferF32,
        stem: &AudioBufferF32,
    ) -> Result<AudioBufferF32, DomainError> {
        Self::ensure_aligned(mix, stem)?;
        let samples = mix
            .samples()
            .par_iter()
            .zip(stem.samples().par_iter())
            .map(|(mix, stem)| mix - stem)
            .collect();
        AudioBufferF32::new(mix.format(), samples)
    }

    pub fn subtract_two(
        mix: &AudioBufferF32,
        first: &AudioBufferF32,
        second: &AudioBufferF32,
    ) -> Result<AudioBufferF32, DomainError> {
        Self::ensure_aligned(mix, first)?;
        Self::ensure_aligned(mix, second)?;
        let samples = mix
            .samples()
            .par_iter()
            .zip(first.samples().par_iter())
            .zip(second.samples().par_iter())
            .map(|((mix, first), second)| mix - first - second)
            .collect();
        AudioBufferF32::new(mix.format(), samples)
    }

    fn ensure_aligned(left: &AudioBufferF32, right: &AudioBufferF32) -> Result<(), DomainError> {
        if left.format() != right.format() || left.frames() != right.frames() {
            Err(DomainError::AudioAlignmentMismatch)
        } else {
            Ok(())
        }
    }
}

pub struct SpectralBleedGate;

impl SpectralBleedGate {
    /// Moves gated backing energy into the lead stem so their complex sum stays
    /// unchanged in every bin, preserving the vocal bus reconstruction.
    pub fn rebalance(
        lead: &mut SpectrogramF32,
        backing: &mut SpectrogramF32,
        floor: f32,
        ceiling: f32,
    ) -> Result<(), DomainError> {
        if !floor.is_finite() || !ceiling.is_finite() || floor < 0.0 || ceiling <= floor {
            return Err(DomainError::DspFailure(
                "bleed gate requires finite bounds with 0 <= floor < ceiling".into(),
            ));
        }
        if lead.sample_rate_hz != backing.sample_rate_hz
            || lead.channels != backing.channels
            || lead.sample_count != backing.sample_count
            || lead.frame_size != backing.frame_size
            || lead.hop_size != backing.hop_size
            || lead.bins.len() != backing.bins.len()
        {
            return Err(DomainError::AudioAlignmentMismatch);
        }
        for (lead_frame, backing_frame) in lead.bins.iter_mut().zip(backing.bins.iter_mut()) {
            if lead_frame.len() != backing_frame.len() {
                return Err(DomainError::AudioAlignmentMismatch);
            }
            for (lead_bin, backing_bin) in lead_frame.iter_mut().zip(backing_frame.iter_mut()) {
                let original_backing = *backing_bin;
                let magnitude = original_backing.norm();
                let gain = ((magnitude - floor) / (ceiling - floor)).clamp(0.0, 1.0);
                *backing_bin = original_backing * gain;
                *lead_bin += original_backing - *backing_bin;
            }
        }
        Ok(())
    }
}
