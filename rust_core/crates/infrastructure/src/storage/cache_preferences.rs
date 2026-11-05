//! Retained spool-cache policy. Active artifacts are protected by leases.
use crate::{audio_tools::TemporaryDirectory, storage::paths::WorkspacePathPolicy};
use std::{fs, io::Write, path::Path};

const DEFAULT_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub fn load(root: &Path) -> Result<u64, String> {
    match fs::read(root.join("retained-cache.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DEFAULT_BYTES),
        Err(e) => Err(e.to_string()),
    }
}
pub fn save(root: &Path, bytes: u64) -> Result<(), String> {
    if bytes > 1024 * 1024 * 1024 * 1024 {
        return Err("Retained cache must not exceed 1 TiB".into());
    }
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let stage = TemporaryDirectory::new(root).map_err(|e| e.to_string())?;
    let temporary = stage.0.join("retained-cache.json");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes.to_string().as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    fs::rename(temporary, root.join("retained-cache.json")).map_err(|e| e.to_string())
}
pub fn effective(policy: &WorkspacePathPolicy) -> Result<u64, String> {
    if std::env::var_os("ATOMUSICATOR_CACHE_RETAINED_BYTES").is_some() {
        super::float_spool::FloatSpoolStore::retention_from_env().map_err(|e| e.to_string())
    } else {
        load(policy.app_data_root())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_policy_round_trips_large_limits_and_rejects_invalid_values() {
        let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        assert_eq!(load(&root.0).unwrap(), DEFAULT_BYTES);
        save(&root.0, 64 * 1024 * 1024 * 1024).unwrap();
        assert_eq!(load(&root.0).unwrap(), 64 * 1024 * 1024 * 1024);
        assert!(save(&root.0, u64::MAX).is_err());
        assert_eq!(load(&root.0).unwrap(), 64 * 1024 * 1024 * 1024);
        fs::write(root.0.join("retained-cache.json"), b"broken").unwrap();
        assert!(load(&root.0).is_err());
    }
}
