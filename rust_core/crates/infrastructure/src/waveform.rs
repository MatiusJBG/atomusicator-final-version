use std::{num::NonZeroUsize, sync::Mutex};

use atomusicator_application::ports::WaveformCachePort;
use atomusicator_domain::AudioBufferF32;
use lru::LruCache;
use sha2::{Digest, Sha256};

type PeakList = Vec<(f32, f32)>;
type PeakEntries = LruCache<[u8; 32], PeakList>;

pub fn wav_peaks(
    path: &std::path::Path,
    max_buckets: usize,
) -> Result<(atomusicator_domain::AudioAssetMetadata, Vec<f32>), atomusicator_domain::DomainError> {
    let mut reader = crate::audio_io::wav_codec::WavBlockReader::open(path)?;
    let metadata = reader.metadata();
    let width = metadata
        .frame_count
        .div_ceil(max_buckets.max(1) as u64)
        .max(1);
    let count = metadata.frame_count.div_ceil(width) as usize;
    let mut peaks = vec![(f32::INFINITY, f32::NEG_INFINITY); count];
    let channels = usize::from(metadata.format.channels);
    let mut buffer = vec![0.0; 65536 * channels];
    for start in (0..metadata.frame_count).step_by(65536) {
        let frames = reader.read_frames(start, &mut buffer)?;
        if frames == 0 {
            return Err(atomusicator_domain::DomainError::AudioDecodeFailure(
                "truncated stem waveform".into(),
            ));
        }
        for (frame, samples) in buffer[..frames * channels].chunks(channels).enumerate() {
            let bucket = ((start + frame as u64) / width) as usize;
            for sample in samples {
                peaks[bucket].0 = peaks[bucket].0.min(*sample);
                peaks[bucket].1 = peaks[bucket].1.max(*sample);
            }
        }
    }
    Ok((
        metadata,
        peaks
            .into_iter()
            .flat_map(|(min, max)| [min, max])
            .collect(),
    ))
}

/// Bounded waveform peak cache keyed by the exact PCM content and bucket size.
pub struct WaveformPeakCache {
    entries: Mutex<PeakEntries>,
}

impl WaveformPeakCache {
    pub fn new(capacity: usize) -> Option<Self> {
        Some(Self {
            entries: Mutex::new(LruCache::new(NonZeroUsize::new(capacity)?)),
        })
    }

    fn key(audio: &AudioBufferF32, bucket_frames: usize) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(audio.format().sample_rate_hz.to_le_bytes());
        hasher.update(audio.format().channels.to_le_bytes());
        hasher.update((bucket_frames as u64).to_le_bytes());
        for sample in audio.samples() {
            hasher.update(sample.to_le_bytes());
        }
        hasher.finalize().into()
    }

    fn compute(audio: &AudioBufferF32, bucket_frames: usize) -> Vec<(f32, f32)> {
        if bucket_frames == 0 || audio.frames() == 0 {
            return Vec::new();
        }

        let channels = usize::from(audio.format().channels);
        audio
            .samples()
            .chunks(bucket_frames.saturating_mul(channels))
            .map(|bucket| {
                bucket
                    .iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), sample| {
                        (min.min(*sample), max.max(*sample))
                    })
            })
            .collect()
    }
}

impl WaveformCachePort for WaveformPeakCache {
    fn peaks(&self, audio: &AudioBufferF32, bucket_frames: usize) -> Vec<(f32, f32)> {
        let key = Self::key(audio, bucket_frames);
        if let Ok(mut entries) = self.entries.lock()
            && let Some(peaks) = entries.get(&key)
        {
            return peaks.clone();
        }

        let peaks = Self::compute(audio, bucket_frames);
        if let Ok(mut entries) = self.entries.lock() {
            entries.put(key, peaks.clone());
        }
        peaks
    }
}
