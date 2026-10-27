use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};

use atomusicator_application::ports::ArtifactCachePort;
use atomusicator_domain::{AudioBufferF32, AudioFormat, DomainError};
use lru::LruCache;
use memmap2::MmapOptions;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactIdentity {
    pub input_sha256: String,
    pub model_sha256: String,
    pub runtime_signature: String,
}

impl ArtifactIdentity {
    pub fn content_key(&self) -> Result<String, DomainError> {
        for value in [&self.input_sha256, &self.model_sha256] {
            if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(DomainError::CacheFailure(
                    "input and model hashes must be SHA-256 values".into(),
                ));
            }
        }
        let mut digest = Sha256::new();
        digest.update(self.input_sha256.to_ascii_lowercase());
        digest.update([0]);
        digest.update(self.model_sha256.to_ascii_lowercase());
        digest.update([0]);
        digest.update(self.runtime_signature.as_bytes());
        Ok(hex_lower(&digest.finalize()))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheHeader {
    sample_rate_hz: u32,
    channels: u16,
    sample_count: usize,
    data_sha256: String,
}

pub struct ContentAddressedCache {
    root: PathBuf,
    memory: Mutex<LruCache<String, AudioBufferF32>>,
    disk_quota_bytes: u64,
}

impl ContentAddressedCache {
    pub fn new(
        root: impl Into<PathBuf>,
        memory_entries: usize,
        disk_quota_bytes: u64,
    ) -> Result<Self, DomainError> {
        let capacity = NonZeroUsize::new(memory_entries).ok_or_else(|| {
            DomainError::CacheFailure("memory cache capacity must be positive".into())
        })?;
        let root = root.into();
        fs::create_dir_all(&root).map_err(|error| cache_io(&root, error))?;
        Ok(Self {
            root,
            memory: Mutex::new(LruCache::new(capacity)),
            disk_quota_bytes,
        })
    }

    fn path_for(&self, key: &str) -> PathBuf {
        let digest = Sha256::digest(key.as_bytes());
        self.root.join(format!("{}.checkpoint", hex_lower(&digest)))
    }

    fn load_from_disk(&self, path: &Path) -> Result<Option<AudioBufferF32>, DomainError> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(cache_io(path, error)),
        };
        let map =
            unsafe { MmapOptions::new().map(&file) }.map_err(|error| cache_io(path, error))?;
        if map.len() < 4 {
            return Ok(None);
        }
        let header_length = u32::from_be_bytes(map[0..4].try_into().unwrap()) as usize;
        let header_end = match 4usize.checked_add(header_length) {
            Some(end) if end <= map.len() => end,
            _ => return Ok(None),
        };
        let header: CacheHeader = match serde_json::from_slice(&map[4..header_end]) {
            Ok(header) => header,
            Err(_) => return Ok(None),
        };
        let data = &map[header_end..];
        if data.len() != header.sample_count.saturating_mul(4)
            || hex_lower(&Sha256::digest(data)) != header.data_sha256
        {
            return Ok(None);
        }
        let format = AudioFormat::new(header.sample_rate_hz, header.channels)
            .map_err(|error| DomainError::CacheFailure(error.to_string()))?;
        let samples = data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| f32::from_le_bytes(*bytes))
            .collect();
        AudioBufferF32::new(format, samples).map(Some)
    }

    fn evict_disk_files(&self, protected: &Path) -> Result<(), DomainError> {
        let mut entries = Vec::new();
        let mut total = 0u64;
        for item in fs::read_dir(&self.root).map_err(|error| cache_io(&self.root, error))? {
            let item = item.map_err(|error| cache_io(&self.root, error))?;
            let item_path = item.path();
            if !item
                .file_type()
                .map_err(|error| cache_io(&item_path, error))?
                .is_file()
            {
                continue;
            }
            let metadata = item
                .metadata()
                .map_err(|error| cache_io(&item_path, error))?;
            total = total.saturating_add(metadata.len());
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            entries.push((modified, metadata.len(), item_path));
        }
        entries.sort_by_key(|entry| entry.0);
        for (_, size, path) in entries {
            if total <= self.disk_quota_bytes {
                break;
            }
            if path == protected {
                continue;
            }
            fs::remove_file(&path).map_err(|error| cache_io(&path, error))?;
            total = total.saturating_sub(size);
        }
        Ok(())
    }
}

impl ArtifactCachePort for ContentAddressedCache {
    fn get_checkpoint(&self, key: &str) -> Result<Option<AudioBufferF32>, DomainError> {
        if let Some(buffer) = self
            .memory
            .lock()
            .map_err(|_| DomainError::CacheFailure("memory cache lock was poisoned".into()))?
            .get(key)
            .cloned()
        {
            return Ok(Some(buffer));
        }
        let path = self.path_for(key);
        match self.load_from_disk(&path)? {
            Some(buffer) => {
                self.memory
                    .lock()
                    .map_err(|_| {
                        DomainError::CacheFailure("memory cache lock was poisoned".into())
                    })?
                    .put(key.to_owned(), buffer.clone());
                Ok(Some(buffer))
            }
            None => {
                if path.exists() {
                    let _ = fs::remove_file(&path);
                }
                Ok(None)
            }
        }
    }

    fn put_checkpoint(&self, key: &str, buffer: &AudioBufferF32) -> Result<(), DomainError> {
        let path = self.path_for(key);
        let temporary_path = path.with_extension(format!("{}.tmp", std::process::id()));
        let data: Vec<u8> = buffer
            .samples()
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        let header = CacheHeader {
            sample_rate_hz: buffer.format().sample_rate_hz,
            channels: buffer.format().channels,
            sample_count: buffer.samples().len(),
            data_sha256: hex_lower(&Sha256::digest(&data)),
        };
        let header_bytes = serde_json::to_vec(&header)
            .map_err(|error| DomainError::CacheFailure(error.to_string()))?;
        let header_length = u32::try_from(header_bytes.len())
            .map_err(|_| DomainError::CacheFailure("cache header is too large".into()))?;
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary_path)
            .map_err(|error| cache_io(&temporary_path, error))?;
        file.write_all(&header_length.to_be_bytes())
            .and_then(|_| file.write_all(&header_bytes))
            .and_then(|_| file.write_all(&data))
            .and_then(|_| file.sync_all())
            .map_err(|error| cache_io(&temporary_path, error))?;
        drop(file);
        if path.exists() {
            fs::remove_file(&path).map_err(|error| cache_io(&path, error))?;
        }
        fs::rename(&temporary_path, &path).map_err(|error| cache_io(&path, error))?;
        self.evict_disk_files(&path)?;
        self.memory
            .lock()
            .map_err(|_| DomainError::CacheFailure("memory cache lock was poisoned".into()))?
            .put(key.to_owned(), buffer.clone());
        Ok(())
    }
}

fn cache_io(path: &Path, error: io::Error) -> DomainError {
    DomainError::CacheFailure(format!("{}: {error}", path.display()))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
