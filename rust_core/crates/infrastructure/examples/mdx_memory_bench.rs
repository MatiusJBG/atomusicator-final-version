//! Duration stress for the real MDX window/FFT/OLA kernel, with a counting sink.
//! This deliberately excludes ONNX models and does not measure extraction quality.
use atomusicator_application::{
    CancellationToken,
    ports::{AudioBlockSource, AudioBlockWriter},
};
use atomusicator_domain::{AudioAssetMetadata, AudioFormat, DomainError};
use atomusicator_infrastructure::{mdx::MdxConfig, mdx_bounded::run_bounded_mdx};
use std::{sync::Arc, time::Instant};
struct VirtualSource {
    metadata: AudioAssetMetadata,
}
impl AudioBlockSource for VirtualSource {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn metadata(&self) -> AudioAssetMetadata {
        self.metadata
    }
    fn peak(&self) -> f32 {
        0.25
    }
    fn content_hash(&self) -> &str {
        "virtual-antiphase-square-wave-not-a-cached-artifact"
    }
    fn read_frames(&self, start: u64, output: &mut [f32]) -> Result<usize, DomainError> {
        output.fill(0.0);
        let frames = self
            .metadata
            .frame_count
            .saturating_sub(start)
            .min((output.len() / 2) as u64) as usize;
        for (index, frame) in output[..frames * 2]
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .enumerate()
        {
            let sample = if (start + index as u64) % 64 < 32 {
                0.25
            } else {
                -0.25
            };
            *frame = [sample, -sample];
        }
        Ok(frames)
    }
}
struct CountingSink {
    frames: u64,
    max_write_frames: usize,
}
impl AudioBlockWriter for CountingSink {
    fn write(&mut self, samples: &[f32]) -> Result<(), DomainError> {
        if !samples.len().is_multiple_of(2) || samples.iter().any(|s| !s.is_finite() || *s != 0.0) {
            return Err(DomainError::DspFailure(
                "zero-prediction stress produced invalid output".into(),
            ));
        }
        self.frames += samples.len() as u64 / 2;
        self.max_write_frames = self.max_write_frames.max(samples.len() / 2);
        Ok(())
    }
    fn finish(self: Box<Self>) -> Result<Arc<dyn AudioBlockSource>, DomainError> {
        Err(DomainError::DspFailure(
            "counting sink does not publish audio".into(),
        ))
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let minutes: u64 = std::env::args()
        .nth(1)
        .ok_or("usage: mdx_memory_bench <5|30|60-minutes>")?
        .parse()?;
    if ![5, 30, 60].contains(&minutes) {
        return Err("choose 5, 30 or 60 minutes".into());
    }
    let source = VirtualSource {
        metadata: AudioAssetMetadata {
            format: AudioFormat::new(44100, 2)?,
            frame_count: minutes * 60 * 44100,
        },
    };
    let mut sink = CountingSink {
        frames: 0,
        max_write_frames: 0,
    };
    let mut windows = 0u64;
    let started = Instant::now();
    run_bounded_mdx(
        MdxConfig::for_profile("uvr-mdxnet-main-v1")?,
        &source,
        &mut sink,
        &CancellationToken::default(),
        |tensor| {
            windows += 1;
            Ok(vec![0.0; tensor.len()])
        },
    )?;
    if sink.frames != source.metadata.frame_count {
        return Err("lost or duplicated output frames".into());
    }
    println!(
        "{}",
        serde_json::json!({"minutes":minutes,"frames":sink.frames,"windows":windows,"max_write_frames":sink.max_write_frames,
        "elapsed_seconds":started.elapsed().as_secs_f64(),"profile":"native-v1/main FFT and overlap kernel","models_executed":false,
        "scope":"duration stress of actual FFT/OLA; virtual source and counting sink; not full application or musical-quality qualification"})
    );
    Ok(())
}
