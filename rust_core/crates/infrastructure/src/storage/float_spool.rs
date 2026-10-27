//! Header + raw little-endian float32. File positions and lengths are 64 bit, without RIFF limits.
use atomusicator_application::{
    CancellationToken,
    ports::{AudioBlockSource, AudioBlockStore, AudioBlockWriter},
};
use atomusicator_domain::{AudioAssetMetadata, AudioFormat, DomainError};
use fs4::fs_std::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Header {
    version: u32,
    metadata: AudioAssetMetadata,
    peak: f32,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
struct CacheEntry {
    version: u32,
    key: String,
    manifest: String,
    content_sha256: String,
    metadata: AudioAssetMetadata,
    stage: String,
    signature: String,
    parents: Vec<String>,
}
pub struct FloatSpool {
    data: PathBuf,
    header: Header,
}
pub struct FloatSpoolStore {
    directory: PathBuf,
}
impl FloatSpoolStore {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}
fn cache_key(stage: &str, signature: &str, parents: &[Arc<dyn AudioBlockSource>]) -> String {
    let mut hash = Sha256::new();
    for value in [stage, signature]
        .into_iter()
        .chain(parents.iter().map(|p| p.content_hash()))
    {
        hash.update((value.len() as u64).to_le_bytes());
        hash.update(value.as_bytes());
    }
    for parent in parents {
        let metadata = parent.metadata();
        hash.update(metadata.format.sample_rate_hz.to_le_bytes());
        hash.update(metadata.format.channels.to_le_bytes());
        hash.update(metadata.frame_count.to_le_bytes());
    }
    format!("{:x}", hash.finalize())
}
impl AudioBlockStore for FloatSpoolStore {
    fn cached(
        &self,
        stage: &str,
        signature: &str,
        parents: &[Arc<dyn AudioBlockSource>],
        cancel: &CancellationToken,
    ) -> Result<Option<Arc<dyn AudioBlockSource>>, DomainError> {
        let key = cache_key(stage, signature, parents);
        let path = self.directory.join(format!("{key}.entry"));
        if !path.exists() {
            return Ok(None);
        }
        let checked = (|| {
            if fs::metadata(&path).map_err(failure)?.len() > 65536 {
                return Err(failure("oversized block cache index"));
            }
            let entry: CacheEntry =
                serde_json::from_slice(&fs::read(&path).map_err(failure)?).map_err(failure)?;
            if entry.version != 1
                || entry.key != key
                || entry.stage != stage
                || entry.signature != signature
                || entry.parents
                    != parents
                        .iter()
                        .map(|p| p.content_hash().to_owned())
                        .collect::<Vec<_>>()
                || entry.manifest.contains(['/', '\\'])
                || !entry.manifest.ends_with(".json")
            {
                return Err(failure("invalid block cache index"));
            }
            let spool = FloatSpool::open(&self.directory.join(entry.manifest), cancel)?;
            if spool.content_hash() != entry.content_sha256 || spool.metadata() != entry.metadata {
                return Err(failure("block cache identity mismatch"));
            }
            Ok(Arc::new(spool) as Arc<dyn AudioBlockSource>)
        })();
        match checked {
            Ok(source) => Ok(Some(source)),
            Err(DomainError::SeparationCancelled) => Err(DomainError::SeparationCancelled),
            Err(_) => {
                let _ = fs::rename(
                    &path,
                    path.with_extension(format!("invalid-{}", crate::audio_tools::unique_id())),
                );
                Ok(None)
            }
        }
    }
    fn remember(
        &self,
        stage: &str,
        signature: &str,
        parents: &[Arc<dyn AudioBlockSource>],
        artifact: &Arc<dyn AudioBlockSource>,
    ) -> Result<(), DomainError> {
        let spool = artifact
            .as_any()
            .downcast_ref::<FloatSpool>()
            .ok_or_else(|| failure("foreign block artifact"))?;
        let key = cache_key(stage, signature, parents);
        let path = self.directory.join(format!("{key}.entry"));
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.with_extension("lock"))
            .map_err(failure)?;
        lock.lock_exclusive().map_err(failure)?;
        if path.is_file() {
            return Ok(());
        }
        let manifest = spool.data.with_extension("json");
        if manifest.parent() != Some(self.directory.as_path()) {
            return Err(failure("block artifact is outside its store"));
        }
        let entry = CacheEntry {
            version: 1,
            key,
            manifest: manifest
                .file_name()
                .ok_or_else(|| failure("missing spool header"))?
                .to_string_lossy()
                .into_owned(),
            content_sha256: spool.content_hash().into(),
            metadata: spool.metadata(),
            stage: stage.into(),
            signature: signature.into(),
            parents: parents.iter().map(|p| p.content_hash().into()).collect(),
        };
        let pending = path.with_extension(format!("pending-{}", crate::audio_tools::unique_id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)
            .map_err(failure)?;
        file.write_all(&serde_json::to_vec(&entry).map_err(failure)?)
            .map_err(failure)?;
        file.sync_all().map_err(failure)?;
        drop(file);
        fs::rename(pending, path).map_err(failure)
    }
    fn create(
        &self,
        stage: &str,
        metadata: AudioAssetMetadata,
    ) -> Result<Box<dyn AudioBlockWriter>, DomainError> {
        if stage.is_empty()
            || !stage
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(failure("invalid spool stage"));
        }
        fs::create_dir_all(&self.directory).map_err(failure)?;
        let bytes = metadata
            .frame_count
            .checked_mul(u64::from(metadata.format.channels) * 4)
            .ok_or_else(|| failure("spool size overflow"))?;
        if fs4::available_space(&self.directory).map_err(failure)? < bytes {
            return Err(failure("insufficient disk space for audio spool"));
        }
        let pending = self.directory.join(format!(
            "{stage}-{}.pending",
            crate::audio_tools::unique_id()
        ));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)
            .map_err(failure)?;
        Ok(Box::new(SpoolWriter {
            pending,
            file: Some(BufWriter::new(file)),
            expected: metadata,
            samples: 0,
            peak: 0.0,
            digest: Sha256::new(),
            failed: false,
        }))
    }
}
impl FloatSpool {
    pub fn open(manifest: &Path, cancel: &CancellationToken) -> Result<Self, DomainError> {
        if fs::metadata(manifest).map_err(failure)?.len() > 65536 {
            return Err(failure("invalid spool header size"));
        }
        let header: Header =
            serde_json::from_slice(&fs::read(manifest).map_err(failure)?).map_err(failure)?;
        let data = manifest.with_extension("f32");
        if header.version != 1 || !header.peak.is_finite() || header.peak < 0.0 {
            return Err(failure("invalid spool header"));
        }
        let checked = Self::inspect_raw(data, header.metadata.format, cancel)?;
        if checked.header.metadata != header.metadata
            || checked.header.sha256 != header.sha256
            || checked.header.peak != header.peak
        {
            return Err(failure("audio spool integrity mismatch"));
        }
        Ok(checked)
    }
    /// Imports an already decoded raw file using bounded buffers. The caller owns its directory.
    pub fn inspect_raw(
        data: PathBuf,
        format: AudioFormat,
        cancel: &CancellationToken,
    ) -> Result<Self, DomainError> {
        let bytes = fs::metadata(&data).map_err(failure)?.len();
        let frame_bytes = u64::from(format.channels) * 4;
        if !bytes.is_multiple_of(frame_bytes) {
            return Err(failure("raw spool has an incomplete frame"));
        }
        let mut file = File::open(&data).map_err(failure)?;
        let mut digest = Sha256::new();
        let mut peak = 0.0f32;
        let mut buffer = [0u8; 65536];
        let mut remaining = bytes;
        while remaining > 0 {
            if cancel.is_cancelled() {
                return Err(DomainError::SeparationCancelled);
            }
            let count = remaining.min(buffer.len() as u64) as usize;
            file.read_exact(&mut buffer[..count]).map_err(failure)?;
            digest.update(&buffer[..count]);
            for sample in buffer[..count].as_chunks::<4>().0 {
                let value = f32::from_le_bytes(*sample);
                if !value.is_finite() {
                    return Err(DomainError::NonFiniteSample);
                }
                peak = peak.max(value.abs());
            }
            remaining -= count as u64;
        }
        Ok(Self {
            data,
            header: Header {
                version: 1,
                metadata: AudioAssetMetadata {
                    format,
                    frame_count: bytes / frame_bytes,
                },
                peak,
                sha256: format!("{:x}", digest.finalize()),
            },
        })
    }
}
impl AudioBlockSource for FloatSpool {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn metadata(&self) -> AudioAssetMetadata {
        self.header.metadata
    }
    fn peak(&self) -> f32 {
        self.header.peak
    }
    fn content_hash(&self) -> &str {
        &self.header.sha256
    }
    fn read_frames(&self, start: u64, destination: &mut [f32]) -> Result<usize, DomainError> {
        let channels = usize::from(self.header.metadata.format.channels);
        if !destination.len().is_multiple_of(channels) {
            return Err(DomainError::IncompleteInterleavedFrame);
        }
        destination.fill(0.0);
        if start >= self.header.metadata.frame_count {
            return Ok(0);
        }
        let frames = (self.header.metadata.frame_count - start)
            .min((destination.len() / channels) as u64) as usize;
        let offset = start
            .checked_mul(channels as u64 * 4)
            .ok_or_else(|| failure("spool offset overflow"))?;
        let mut file = File::open(&self.data).map_err(failure)?;
        file.seek(SeekFrom::Start(offset)).map_err(failure)?;
        let mut bytes = vec![0u8; frames * channels * 4];
        file.read_exact(&mut bytes).map_err(failure)?;
        for (target, sample) in destination.iter_mut().zip(bytes.as_chunks::<4>().0.iter()) {
            *target = f32::from_le_bytes(*sample);
            if !target.is_finite() {
                return Err(DomainError::NonFiniteSample);
            }
        }
        Ok(frames)
    }
}
struct SpoolWriter {
    pending: PathBuf,
    file: Option<BufWriter<File>>,
    expected: AudioAssetMetadata,
    samples: u64,
    peak: f32,
    digest: Sha256,
    failed: bool,
}
impl AudioBlockWriter for SpoolWriter {
    fn write(&mut self, samples: &[f32]) -> Result<(), DomainError> {
        let expected = self
            .expected
            .frame_count
            .checked_mul(u64::from(self.expected.format.channels))
            .ok_or_else(|| failure("spool size overflow"))?;
        if self.failed
            || !samples
                .len()
                .is_multiple_of(usize::from(self.expected.format.channels))
            || samples.iter().any(|v| !v.is_finite())
            || self
                .samples
                .checked_add(samples.len() as u64)
                .is_none_or(|count| count > expected)
        {
            self.failed = true;
            return Err(failure("invalid or oversized spool block"));
        }
        let mut bytes = Vec::with_capacity(samples.len() * 4);
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
            self.peak = self.peak.max(sample.abs());
        }
        let result = self
            .file
            .as_mut()
            .ok_or_else(|| failure("spool already finished"))?
            .write_all(&bytes);
        if let Err(error) = result {
            self.failed = true;
            return Err(failure(error));
        }
        self.digest.update(&bytes);
        self.samples += samples.len() as u64;
        Ok(())
    }
    fn finish(mut self: Box<Self>) -> Result<Arc<dyn AudioBlockSource>, DomainError> {
        let expected = self.expected.frame_count * u64::from(self.expected.format.channels);
        if self.failed || self.samples != expected {
            return Err(failure("incomplete audio spool"));
        }
        let mut file = self
            .file
            .take()
            .ok_or_else(|| failure("spool already finished"))?;
        file.flush().map_err(failure)?;
        file.get_ref().sync_all().map_err(failure)?;
        drop(file);
        let data = self.pending.with_extension("f32");
        fs::rename(&self.pending, &data).map_err(failure)?;
        let header = Header {
            version: 1,
            metadata: self.expected,
            peak: self.peak,
            sha256: format!("{:x}", self.digest.clone().finalize()),
        };
        let manifest = data.with_extension("json");
        let pending_header = data.with_extension("header-pending");
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&pending_header)
            .map_err(failure)?;
        file.write_all(&serde_json::to_vec(&header).map_err(failure)?)
            .map_err(failure)?;
        file.sync_all().map_err(failure)?;
        drop(file);
        fs::rename(pending_header, manifest).map_err(failure)?;
        Ok(Arc::new(FloatSpool { data, header }))
    }
}
impl Drop for SpoolWriter {
    fn drop(&mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.pending);
    }
}
fn failure(error: impl std::fmt::Display) -> DomainError {
    DomainError::WorkspaceFailure(error.to_string())
}
