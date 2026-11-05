//! Final-size float WAV headers and bounded RF64 access (EBU Tech 3306).
use atomusicator_domain::{AudioAssetMetadata, AudioFormat, DomainError};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

#[derive(Clone, Copy)]
pub enum WaveContainer {
    Auto,
    Riff,
    Rf64,
}
fn export(e: impl std::fmt::Display) -> DomainError {
    DomainError::AudioExportFailure(e.to_string())
}
fn decode(e: impl std::fmt::Display) -> DomainError {
    DomainError::AudioDecodeFailure(e.to_string())
}
fn data_bytes(metadata: AudioAssetMetadata) -> Result<u64, DomainError> {
    metadata
        .frame_count
        .checked_mul(u64::from(metadata.format.channels) * 4)
        .filter(|n| *n <= i64::MAX as u64 - 128)
        .ok_or_else(|| export("WAV size overflow"))
}
pub fn float_header(
    metadata: AudioAssetMetadata,
    container: WaveContainer,
) -> Result<Vec<u8>, DomainError> {
    AudioFormat::new(metadata.format.sample_rate_hz, metadata.format.channels)?;
    let bytes = data_bytes(metadata)?;
    let rf64 = match container {
        WaveContainer::Auto => bytes > u64::from(u32::MAX) - 128,
        WaveContainer::Rf64 => true,
        WaveContainer::Riff => false,
    };
    if !rf64 && bytes > u64::from(u32::MAX) - 128 {
        return Err(export("RIFF capacity exceeded"));
    }
    let align = metadata
        .format
        .channels
        .checked_mul(4)
        .ok_or_else(|| export("WAV block alignment overflow"))?;
    let rate = metadata
        .format
        .sample_rate_hz
        .checked_mul(u32::from(align))
        .ok_or_else(|| export("WAV byte rate overflow"))?;
    let mut header = Vec::with_capacity(94);
    header.extend_from_slice(if rf64 { b"RF64" } else { b"RIFF" });
    header.extend_from_slice(&(if rf64 { u32::MAX } else { bytes as u32 + 50 }).to_le_bytes());
    header.extend_from_slice(b"WAVE");
    if rf64 {
        header.extend_from_slice(b"ds64");
        header.extend_from_slice(&28u32.to_le_bytes());
        header.extend_from_slice(&(bytes + 86).to_le_bytes());
        header.extend_from_slice(&bytes.to_le_bytes());
        header.extend_from_slice(&metadata.frame_count.to_le_bytes());
        header.extend_from_slice(&0u32.to_le_bytes());
    }
    header.extend_from_slice(b"fmt ");
    header.extend_from_slice(&18u32.to_le_bytes());
    header.extend_from_slice(&3u16.to_le_bytes());
    header.extend_from_slice(&metadata.format.channels.to_le_bytes());
    header.extend_from_slice(&metadata.format.sample_rate_hz.to_le_bytes());
    header.extend_from_slice(&rate.to_le_bytes());
    header.extend_from_slice(&align.to_le_bytes());
    header.extend_from_slice(&32u16.to_le_bytes());
    header.extend_from_slice(&0u16.to_le_bytes());
    header.extend_from_slice(b"fact");
    header.extend_from_slice(&4u32.to_le_bytes());
    header.extend_from_slice(
        &(if rf64 {
            u32::MAX
        } else {
            u32::try_from(metadata.frame_count).unwrap_or(u32::MAX)
        })
        .to_le_bytes(),
    );
    header.extend_from_slice(b"data");
    header.extend_from_slice(&(if rf64 { u32::MAX } else { bytes as u32 }).to_le_bytes());
    Ok(header)
}

pub struct FloatWavWriter {
    file: BufWriter<File>,
    hash: Sha256,
    remaining: u64,
    channels: usize,
    bytes: Vec<u8>,
}
impl FloatWavWriter {
    pub fn create(path: &Path, metadata: AudioAssetMetadata) -> Result<Self, DomainError> {
        Self::open(path, metadata, false)
    }
    pub fn create_new(path: &Path, metadata: AudioAssetMetadata) -> Result<Self, DomainError> {
        Self::open(path, metadata, true)
    }
    fn open(path: &Path, metadata: AudioAssetMetadata, new: bool) -> Result<Self, DomainError> {
        let header = float_header(metadata, WaveContainer::Auto)?;
        let mut options = OpenOptions::new();
        options.write(true);
        if new {
            options.create_new(true);
        } else {
            options.create(true).truncate(true);
        }
        let mut file = BufWriter::new(options.open(path).map_err(export)?);
        file.write_all(&header).map_err(export)?;
        let mut hash = Sha256::new();
        hash.update(header);
        Ok(Self {
            file,
            hash,
            remaining: data_bytes(metadata)? / 4,
            channels: metadata.format.channels as usize,
            bytes: Vec::new(),
        })
    }
    pub fn write_samples(&mut self, samples: &[f32]) -> Result<(), DomainError> {
        if !samples.len().is_multiple_of(self.channels) || samples.len() as u64 > self.remaining {
            return Err(export("invalid WAV frame count"));
        }
        self.bytes.clear();
        for sample in samples {
            if !sample.is_finite() {
                return Err(DomainError::NonFiniteSample);
            }
            self.bytes.extend_from_slice(&sample.to_le_bytes());
        }
        self.file.write_all(&self.bytes).map_err(export)?;
        self.hash.update(&self.bytes);
        self.remaining -= samples.len() as u64;
        Ok(())
    }
    pub fn finish(mut self) -> Result<String, DomainError> {
        if self.remaining != 0 {
            return Err(export("incomplete WAV output"));
        }
        self.file.flush().map_err(export)?;
        self.file.get_ref().sync_all().map_err(export)?;
        Ok(format!("{:x}", self.hash.finalize()))
    }
}

pub(crate) struct Rf64Reader {
    reader: BufReader<File>,
    pub metadata: AudioAssetMetadata,
    data_offset: u64,
    bits: u16,
    float: bool,
    bytes: Vec<u8>,
}
impl Rf64Reader {
    pub fn open(path: &Path) -> Result<Self, DomainError> {
        let mut reader = BufReader::new(File::open(path).map_err(decode)?);
        let file_len = reader.get_ref().metadata().map_err(decode)?.len();
        let mut header = [0u8; 12];
        reader.read_exact(&mut header).map_err(decode)?;
        if !matches!(&header[..4], b"RF64" | b"BW64") || &header[8..] != b"WAVE" {
            return Err(decode("invalid RF64 header"));
        }
        let mut sizes: Option<(u64, u64, u64)> = None;
        let mut format: Option<(AudioFormat, u16, bool)> = None;
        let mut fact: Option<u32> = None;
        for index in 0..1024 {
            let mut chunk = [0u8; 8];
            reader.read_exact(&mut chunk).map_err(decode)?;
            let size = u32::from_le_bytes(chunk[4..].try_into().unwrap());
            let offset = reader.stream_position().map_err(decode)?;
            if index == 0 && &chunk[..4] != b"ds64" {
                return Err(decode("RF64 requires ds64 as first chunk"));
            }
            if &chunk[..4] == b"data" {
                let (riff, data, frames) = sizes.ok_or_else(|| decode("missing RF64 sizes"))?;
                let riff32 = u32::from_le_bytes(header[4..8].try_into().unwrap());
                let riff = if riff32 == u32::MAX {
                    riff
                } else {
                    u64::from(riff32)
                };
                let frames = match fact {
                    Some(value) if value != u32::MAX => u64::from(value),
                    _ => frames,
                };
                let (format, bits, float) = format.ok_or_else(|| decode("missing RF64 format"))?;
                let align = u64::from(format.channels) * u64::from(bits) / 8;
                let data = if size == u32::MAX {
                    data
                } else {
                    u64::from(size)
                };
                let end = offset
                    .checked_add(data)
                    .ok_or_else(|| decode("RF64 data overflow"))?;
                let riff_end = riff
                    .checked_add(8)
                    .ok_or_else(|| decode("RF64 RIFF overflow"))?;
                if !data.is_multiple_of(align)
                    || end > file_len
                    || end > riff_end
                    || riff_end > file_len
                    || (frames != 0 && frames != data / align)
                {
                    return Err(decode("RF64 size/sample count mismatch or truncated audio"));
                }
                return Ok(Self {
                    reader,
                    metadata: AudioAssetMetadata {
                        format,
                        frame_count: data / align,
                    },
                    data_offset: offset,
                    bits,
                    float,
                    bytes: Vec::new(),
                });
            }
            let end = offset
                .checked_add(u64::from(size) + u64::from(size % 2))
                .filter(|n| *n <= file_len)
                .ok_or_else(|| decode("invalid RF64 chunk size"))?;
            match &chunk[..4] {
                b"fact" => {
                    if size < 4 {
                        return Err(decode("invalid RF64 fact chunk"));
                    }
                    let mut value = [0u8; 4];
                    reader.read_exact(&mut value).map_err(decode)?;
                    fact = Some(u32::from_le_bytes(value));
                }
                b"ds64" => {
                    if sizes.is_some() || !(28..=65536).contains(&size) {
                        return Err(decode("invalid duplicate or oversized ds64 chunk"));
                    }
                    let mut values = [0u8; 28];
                    reader.read_exact(&mut values).map_err(decode)?;
                    if u32::from_le_bytes(values[24..].try_into().unwrap()) != 0 {
                        return Err(decode("RF64 extended chunk tables are not supported"));
                    }
                    sizes = Some((
                        u64::from_le_bytes(values[..8].try_into().unwrap()),
                        u64::from_le_bytes(values[8..16].try_into().unwrap()),
                        u64::from_le_bytes(values[16..24].try_into().unwrap()),
                    ));
                }
                b"fmt " => {
                    if format.is_some() || !(16..=65536).contains(&size) {
                        return Err(decode("invalid RF64 format chunk"));
                    }
                    let mut fmt = vec![0u8; size as usize];
                    reader.read_exact(&mut fmt).map_err(decode)?;
                    let mut tag = u16::from_le_bytes(fmt[..2].try_into().unwrap());
                    let channels = u16::from_le_bytes(fmt[2..4].try_into().unwrap());
                    let rate = u32::from_le_bytes(fmt[4..8].try_into().unwrap());
                    let bits = u16::from_le_bytes(fmt[14..16].try_into().unwrap());
                    if tag == 0xfffe {
                        if fmt.len() < 40
                            || fmt[26..40]
                                != [0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71]
                        {
                            return Err(decode("unsupported RF64 extensible subformat"));
                        }
                        tag = u16::from_le_bytes(fmt[24..26].try_into().unwrap());
                    }
                    if !matches!((tag, bits), (3, 32) | (1, 16 | 24 | 32)) {
                        return Err(decode("unsupported RF64 sample format"));
                    }
                    let audio_format = AudioFormat::new(rate, channels)?;
                    let align = u32::from(channels) * u32::from(bits) / 8;
                    if u32::from(u16::from_le_bytes(fmt[12..14].try_into().unwrap())) != align
                        || rate.checked_mul(align)
                            != Some(u32::from_le_bytes(fmt[8..12].try_into().unwrap()))
                    {
                        return Err(decode("invalid RF64 block alignment/byte rate"));
                    }
                    format = Some((audio_format, bits, tag == 3));
                }
                _ => {}
            }
            reader.seek(SeekFrom::Start(end)).map_err(decode)?;
        }
        Err(decode("RF64 metadata chunk limit exceeded"))
    }
    pub fn read_frames(
        &mut self,
        start: u64,
        destination: &mut [f32],
    ) -> Result<usize, DomainError> {
        let channels = self.metadata.format.channels as usize;
        if !destination.len().is_multiple_of(channels) {
            return Err(DomainError::IncompleteInterleavedFrame);
        }
        destination.fill(0.0);
        let frames = self
            .metadata
            .frame_count
            .saturating_sub(start)
            .min((destination.len() / channels) as u64) as usize;
        if frames == 0 {
            return Ok(0);
        }
        let width = self.bits as usize / 8;
        let position = start
            .checked_mul((channels * width) as u64)
            .and_then(|v| self.data_offset.checked_add(v))
            .ok_or_else(|| decode("RF64 seek overflow"))?;
        self.reader
            .seek(SeekFrom::Start(position))
            .map_err(decode)?;
        self.bytes.resize(frames * channels * width, 0);
        self.reader.read_exact(&mut self.bytes).map_err(decode)?;
        for (target, bytes) in destination.iter_mut().zip(self.bytes.chunks_exact(width)) {
            *target = match (self.float, self.bits) {
                (true, 32) => f32::from_le_bytes(bytes.try_into().unwrap()),
                (false, 16) => f32::from(i16::from_le_bytes(bytes.try_into().unwrap())) / 32768.0,
                (false, 24) => {
                    i32::from_le_bytes([
                        bytes[0],
                        bytes[1],
                        bytes[2],
                        if bytes[2] & 0x80 != 0 { 255 } else { 0 },
                    ]) as f32
                        / 8388608.0
                }
                (false, 32) => i32::from_le_bytes(bytes.try_into().unwrap()) as f32 / 2147483648.0,
                _ => return Err(decode("unsupported RF64 decoder")),
            };
            if !target.is_finite() {
                return Err(DomainError::NonFiniteSample);
            }
        }
        Ok(frames)
    }
}
