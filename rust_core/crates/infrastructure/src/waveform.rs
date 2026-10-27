use std::{num::NonZeroUsize, sync::Mutex};

use atomusicator_application::ports::WaveformCachePort;
use atomusicator_domain::AudioBufferF32;
use lru::LruCache;
use sha2::{Digest, Sha256};

type PeakList = Vec<(f32, f32)>;
type PeakEntries = LruCache<[u8; 32], PeakList>;

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
