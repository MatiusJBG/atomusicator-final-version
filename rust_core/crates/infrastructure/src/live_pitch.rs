//! Streaming, fixed-memory pitch DSP. Instances belong to one producer thread.
use atomusicator_domain::{DomainError, PitchShiftSpec};
use std::ptr::NonNull;

pub const PITCH_BLOCK: usize = 256;
pub const TONALITY_HZ: f32 = 8000.0;
/// Previous production window. Musical profiles never go shorter than this.
const MIN_BLOCK: u32 = 3072;
pub const ENGINE_ID: &str =
    "signalsmith-1.3.2-88061fd4-block0.12sr-hop0.03sr-tonality8k-fft8192-v2";

/// Signalsmith `presetDefault` sizes (0.12 s block, 0.03 s hop), never shorter
/// than the previous 3072-sample window.
pub fn musical_profile(sample_rate: u32) -> (u32, u32) {
    let rate = f64::from(sample_rate);
    let block = (rate * 0.12).round() as u32;
    let block = block.clamp(MIN_BLOCK, 8192);
    let interval = ((rate * 0.03).round() as u32).clamp(64, block);
    (block, interval)
}

unsafe extern "C" {
    fn atom_pitch_create(rate: u32, window: u32, interval: u32) -> *mut std::ffi::c_void;
    fn atom_pitch_destroy(pitch: *mut std::ffi::c_void);
    fn atom_pitch_delay(pitch: *mut std::ffi::c_void) -> u32;
    fn atom_pitch_output_delay(pitch: *mut std::ffi::c_void) -> u32;
    fn atom_pitch_set(pitch: *mut std::ffi::c_void, semitones: f32);
    fn atom_pitch_process(
        pitch: *mut std::ffi::c_void,
        input: *const f32,
        output: *mut f32,
        frames: u32,
    ) -> bool;
    fn atom_pitch_reset(pitch: *mut std::ffi::c_void);
}

pub struct LivePitchProcessor {
    handle: NonNull<std::ffi::c_void>,
}
// SAFETY: the object has no thread affinity. Processing/settings require exclusive
// &mut access; it is deliberately not Sync and never shared across DSP workers.
unsafe impl Send for LivePitchProcessor {}
impl LivePitchProcessor {
    pub fn new(sample_rate: u32) -> Result<Self, DomainError> {
        let (window, interval) = musical_profile(sample_rate);
        Self::with_profile(sample_rate, window, interval)
    }
    /// Controlled qualification only; the application uses the fixed profile.
    pub fn with_profile(sample_rate: u32, window: u32, interval: u32) -> Result<Self, DomainError> {
        if !(8000..=192000).contains(&sample_rate) {
            return Err(fail("invalid pitch sample rate"));
        }
        if !(512..=8192).contains(&window) || !(64..=window).contains(&interval) {
            return Err(fail("invalid pitch processing profile"));
        }
        // SAFETY: constructor owns its returned allocation; null signals failure.
        let handle = NonNull::new(unsafe { atom_pitch_create(sample_rate, window, interval) })
            .ok_or_else(|| fail("pitch engine initialization failed"))?;
        Ok(Self { handle })
    }
    pub fn set_spec(&mut self, spec: PitchShiftSpec) {
        // SAFETY: exclusively owned valid instance; domain validates parameters.
        unsafe { atom_pitch_set(self.handle.as_ptr(), spec.semitones()) }
    }
    pub fn latency_frames(&self) -> usize {
        // SAFETY: live instance, read-only query.
        unsafe { atom_pitch_delay(self.handle.as_ptr()) as usize }
    }
    pub fn output_delay_frames(&self) -> usize {
        // SAFETY: live instance, read-only query.
        unsafe { atom_pitch_output_delay(self.handle.as_ptr()) as usize }
    }
    pub fn reset(&mut self) {
        // SAFETY: exclusive ownership; no process call can overlap.
        unsafe { atom_pitch_reset(self.handle.as_ptr()) }
    }
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) -> Result<(), DomainError> {
        if input.len() != output.len()
            || input.is_empty()
            || !input.len().is_multiple_of(2)
            || input.len() > PITCH_BLOCK * 2
            || input.iter().any(|v| !v.is_finite())
        {
            return Err(fail("invalid stereo pitch block"));
        }
        // SAFETY: disjoint Rust slices provide valid interleaved stereo buffers
        // of the exact frame count; the C++ call does not retain their pointers.
        let ok = unsafe {
            atom_pitch_process(
                self.handle.as_ptr(),
                input.as_ptr(),
                output.as_mut_ptr(),
                (input.len() / 2) as u32,
            )
        };
        if !ok || output.iter().any(|v| !v.is_finite()) {
            return Err(fail("live pitch processing failed"));
        }
        Ok(())
    }
}
impl Drop for LivePitchProcessor {
    fn drop(&mut self) {
        // SAFETY: this is the sole owner, destroyed exactly once.
        unsafe { atom_pitch_destroy(self.handle.as_ptr()) }
    }
}
/// Independent stereo processors share two foreground workers. Coordination
/// remains in the producer; the device callback never waits for this pool.
pub struct LivePitchBank {
    processors: Vec<LivePitchProcessor>,
    workers: rayon::ThreadPool,
}
impl LivePitchBank {
    pub fn new(rate: u32) -> Result<Self, DomainError> {
        let processors = (0..5)
            .map(|_| LivePitchProcessor::new(rate))
            .collect::<Result<Vec<_>, _>>()?;
        let workers = rayon::ThreadPoolBuilder::new()
            .num_threads(
                std::thread::available_parallelism()
                    .map_or(1, usize::from)
                    .min(2),
            )
            .thread_name(|index| format!("atomusicator-pitch-{index}"))
            .build()
            .map_err(|e| DomainError::DspFailure(e.to_string()))?;
        Ok(Self {
            processors,
            workers,
        })
    }
    pub fn latency_frames(&self) -> usize {
        self.processors[0].latency_frames()
    }
    pub fn output_delay_frames(&self) -> usize {
        self.processors[0].output_delay_frames()
    }
    pub fn process(
        &mut self,
        inputs: &[[f32; PITCH_BLOCK * 2]; 5],
        outputs: &mut [[f32; PITCH_BLOCK * 2]; 5],
        spec: PitchShiftSpec,
    ) -> Result<(), DomainError> {
        use rayon::prelude::*;
        self.workers.install(|| {
            self.processors
                .par_iter_mut()
                .zip(inputs.par_iter())
                .zip(outputs.par_iter_mut())
                .try_for_each(|((processor, input), output)| {
                    processor.set_spec(spec);
                    processor.process(input, output)
                })
        })
    }
}
fn fail(message: &str) -> DomainError {
    DomainError::DspFailure(message.into())
}

/// Export through the monitoring engine without replacing originals or loading
/// full files into memory. Startup delay and padded tails are removed exactly.
pub fn render_file(
    source: &std::path::Path,
    target: &std::path::Path,
    spec: PitchShiftSpec,
    cancel: &atomusicator_application::CancellationToken,
) -> Result<(), DomainError> {
    use crate::audio_io::wav_codec::WavBlockReader;
    let mut reader = WavBlockReader::open(source)?;
    let metadata = reader.metadata();
    crate::audio_io::wav_codec::validate_block_wav_size(metadata)?;
    let channels = usize::from(metadata.format.channels);
    if !(1..=2).contains(&channels) {
        return Err(fail("pitch supports mono/stereo files"));
    }
    let mut processor = LivePitchProcessor::new(metadata.format.sample_rate_hz)?;
    processor.set_spec(spec);
    let delay = processor.latency_frames() as u64;
    // Never truncate a caller's existing file, including an original or alias.
    let mut writer = crate::audio_io::wav_stream::FloatWavWriter::create_new(target, metadata)?;
    let mut raw = [0.0f32; PITCH_BLOCK * 2];
    let mut input = [0.0f32; PITCH_BLOCK * 2];
    let mut output = [0.0f32; PITCH_BLOCK * 2];
    let mut position = 0;
    let mut written = 0;
    while written < metadata.frame_count {
        if cancel.is_cancelled() {
            return Err(DomainError::SeparationCancelled);
        }
        reader.read_frames(position, &mut raw[..PITCH_BLOCK * channels])?;
        for i in 0..PITCH_BLOCK {
            input[i * 2] = raw[i * channels];
            input[i * 2 + 1] = raw[i * channels + channels - 1];
        }
        processor.process(&input, &mut output)?;
        let first = delay.saturating_sub(position).min(PITCH_BLOCK as u64) as usize;
        let count = (PITCH_BLOCK - first).min((metadata.frame_count - written) as usize);
        if channels == 2 {
            writer.write_samples(&output[first * 2..(first + count) * 2])?;
        } else {
            for index in 0..count {
                raw[index] = output[(first + index) * 2];
            }
            writer.write_samples(&raw[..count])?;
        }
        written += count as u64;
        position += PITCH_BLOCK as u64;
    }
    writer.finish().map(|_| ())
}
