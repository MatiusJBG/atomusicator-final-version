//! Bounded multilevel min/max index. Its temporary files live exactly as long as their readers.
use crate::{audio_io::wav_codec::WavBlockReader, audio_tools::TemporaryDirectory};
use atomusicator_domain::{AudioAssetMetadata, DomainError};
use std::{
    fs::{self, File},
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

struct Level {
    frames_per_bucket: u64,
    buckets: u64,
}
pub struct WaveformIndex {
    directory: TemporaryDirectory,
    metadata: AudioAssetMetadata,
    levels: Vec<Level>,
}
pub struct PeakRange {
    pub start_frame: u64,
    pub frames_per_bucket: u64,
    pub peaks: Vec<f32>,
}
impl WaveformIndex {
    pub fn prepare(source: &Path, temporary_root: &Path) -> Result<Self, DomainError> {
        let mut reader = WavBlockReader::open(source)?;
        let metadata = reader.metadata();
        if metadata.frame_count == 0 {
            return Err(failure("empty waveform source"));
        }
        fs::create_dir_all(temporary_root).map_err(failure)?;
        let directory = TemporaryDirectory::new(temporary_root)?;
        // At most 65,536 base buckets, <1 MiB across all levels, regardless of duration.
        let width = metadata.frame_count.div_ceil(65536).max(256);
        let count = metadata.frame_count.div_ceil(width);
        let mut writer =
            BufWriter::new(File::create(directory.0.join("0.peaks")).map_err(failure)?);
        let channels = usize::from(metadata.format.channels);
        let mut buffer = vec![0.0; 65536 * channels];
        let mut pair = [f32::INFINITY, f32::NEG_INFINITY];
        let mut accumulated = 0;
        for start in (0..metadata.frame_count).step_by(65536) {
            let frames = reader.read_frames(start, &mut buffer)?;
            if frames != (metadata.frame_count - start).min(65536) as usize {
                return Err(failure("truncated waveform source"));
            }
            for frame in buffer[..frames * channels].chunks_exact(channels) {
                for sample in frame {
                    pair[0] = pair[0].min(*sample);
                    pair[1] = pair[1].max(*sample);
                }
                accumulated += 1;
                if accumulated == width {
                    write_pair(&mut writer, pair)?;
                    pair = [f32::INFINITY, f32::NEG_INFINITY];
                    accumulated = 0;
                }
            }
        }
        if accumulated != 0 {
            write_pair(&mut writer, pair)?;
        }
        writer.flush().map_err(failure)?;
        drop(writer);
        let mut levels = vec![Level {
            frames_per_bucket: width,
            buckets: count,
        }];
        while levels.last().unwrap().buckets > 1 {
            let previous = levels.last().unwrap();
            let mut reader = BufReader::new(
                File::open(directory.0.join(format!("{}.peaks", levels.len() - 1)))
                    .map_err(failure)?,
            );
            let mut writer = BufWriter::new(
                File::create(directory.0.join(format!("{}.peaks", levels.len())))
                    .map_err(failure)?,
            );
            for index in (0..previous.buckets).step_by(2) {
                let mut pair = read_pair(&mut reader)?;
                if index + 1 < previous.buckets {
                    let second = read_pair(&mut reader)?;
                    pair = [pair[0].min(second[0]), pair[1].max(second[1])];
                }
                write_pair(&mut writer, pair)?;
            }
            writer.flush().map_err(failure)?;
            levels.push(Level {
                frames_per_bucket: previous
                    .frames_per_bucket
                    .checked_mul(2)
                    .ok_or_else(|| failure("waveform level overflow"))?,
                buckets: previous.buckets.div_ceil(2),
            });
        }
        Ok(Self {
            directory,
            metadata,
            levels,
        })
    }
    pub fn metadata(&self) -> AudioAssetMetadata {
        self.metadata
    }
    pub fn range(
        &self,
        start: u64,
        end: u64,
        max_buckets: usize,
    ) -> Result<PeakRange, DomainError> {
        if start >= end || end > self.metadata.frame_count || !(1..=8192).contains(&max_buckets) {
            return Err(failure("invalid waveform range or resolution"));
        }
        let (index, level) = self
            .levels
            .iter()
            .enumerate()
            .find(|(_, l)| {
                end.div_ceil(l.frames_per_bucket) - start / l.frames_per_bucket
                    <= max_buckets as u64
            })
            .ok_or_else(|| failure("waveform level unavailable"))?;
        let first = start / level.frames_per_bucket;
        let count = end.div_ceil(level.frames_per_bucket) - first;
        let mut reader =
            File::open(self.directory.0.join(format!("{index}.peaks"))).map_err(failure)?;
        if reader.metadata().map_err(failure)?.len() != level.buckets * 8 {
            return Err(failure("truncated waveform index"));
        }
        reader.seek(SeekFrom::Start(first * 8)).map_err(failure)?;
        let mut reader = BufReader::new(reader);
        let mut peaks = Vec::with_capacity(count as usize * 2);
        for _ in 0..count {
            peaks.extend(read_pair(&mut reader)?);
        }
        Ok(PeakRange {
            start_frame: first * level.frames_per_bucket,
            frames_per_bucket: level.frames_per_bucket,
            peaks,
        })
    }
}
fn write_pair(writer: &mut impl Write, pair: [f32; 2]) -> Result<(), DomainError> {
    for sample in pair {
        writer.write_all(&sample.to_le_bytes()).map_err(failure)?;
    }
    Ok(())
}
fn read_pair(reader: &mut impl Read) -> Result<[f32; 2], DomainError> {
    let mut bytes = [0u8; 8];
    reader.read_exact(&mut bytes).map_err(failure)?;
    let pair = [
        f32::from_le_bytes(bytes[..4].try_into().unwrap()),
        f32::from_le_bytes(bytes[4..].try_into().unwrap()),
    ];
    if pair.iter().any(|s| !s.is_finite()) || pair[0] > pair[1] {
        return Err(failure("invalid waveform peaks"));
    }
    Ok(pair)
}
fn failure(error: impl std::fmt::Display) -> DomainError {
    DomainError::AudioDecodeFailure(error.to_string())
}
