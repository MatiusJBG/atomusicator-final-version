use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
};

use crate::errors::InfrastructureError;

#[derive(Debug, Clone, Deserialize)]
pub struct ModelManifest {
    pub manifest_version: u32,
    pub models: Vec<ModelProfile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelProfile {
    pub profile_id: String,
    pub stage: String,
    pub filename: String,
    pub sha256: String,
    pub download_url: String,
    pub license: String,
    pub license_status: String,
    pub attribution: Option<String>,
}

impl ModelManifest {
    pub fn parse(json: &str) -> Result<Self, InfrastructureError> {
        let manifest: Self = serde_json::from_str(json)
            .map_err(|error| InfrastructureError::InvalidManifest(error.to_string()))?;
        if manifest.manifest_version == 0 || manifest.models.is_empty() {
            return Err(InfrastructureError::InvalidManifest(
                "manifest must have a positive version and at least one model".into(),
            ));
        }
        let mut ids = std::collections::HashSet::new();
        for profile in &manifest.models {
            if !ids.insert(profile.profile_id.as_str()) {
                return Err(InfrastructureError::InvalidManifest(format!(
                    "duplicate profile id `{}`",
                    profile.profile_id
                )));
            }
            if profile.sha256.len() != 64
                || !profile.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(InfrastructureError::InvalidDigest(
                    profile.profile_id.clone(),
                ));
            }
            if !profile.download_url.starts_with("https://") {
                return Err(InfrastructureError::InvalidManifest(format!(
                    "profile `{}` must use HTTPS",
                    profile.profile_id
                )));
            }
            if Path::new(&profile.filename)
                .file_name()
                .and_then(|name| name.to_str())
                != Some(profile.filename.as_str())
                || profile.filename.contains('/')
                || profile.filename.contains('\\')
                || profile.filename.trim().is_empty()
            {
                return Err(InfrastructureError::InvalidManifest(format!(
                    "profile `{}` filename must be a single path component",
                    profile.profile_id
                )));
            }
        }
        Ok(manifest)
    }

    pub fn profile(&self, profile_id: &str) -> Result<&ModelProfile, InfrastructureError> {
        self.models
            .iter()
            .find(|model| model.profile_id == profile_id)
            .ok_or_else(|| InfrastructureError::ModelNotFound(profile_id.to_owned()))
    }
}

#[derive(Clone, Default)]
pub struct ModelDownloader {
    client: reqwest::Client,
}

impl ModelDownloader {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    pub async fn download_verified(
        &self,
        profile: &ModelProfile,
        model_dir: &Path,
    ) -> Result<PathBuf, InfrastructureError> {
        fs::create_dir_all(model_dir)
            .await
            .map_err(|error| InfrastructureError::io(model_dir, error))?;
        let final_path = model_dir.join(&profile.filename);
        if final_path.exists() {
            let actual = hash_file(&final_path).await?;
            if actual == profile.sha256.to_ascii_lowercase() {
                return Ok(final_path);
            }
        }

        let id = NEXT_DOWNLOAD_ID.fetch_add(1, Ordering::Relaxed);
        let temporary_path = model_dir.join(format!(
            ".{}.{}.{}.partial",
            profile.filename,
            std::process::id(),
            id
        ));
        let _cleanup = PartialDownload(temporary_path.clone());
        let result = self.download_to(profile, &temporary_path).await;
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path).await;
        }
        result?;
        publish_verified_model(&temporary_path, &final_path).await?;
        Ok(final_path)
    }

    async fn download_to(
        &self,
        profile: &ModelProfile,
        path: &Path,
    ) -> Result<(), InfrastructureError> {
        let mut response = self
            .client
            .get(&profile.download_url)
            .timeout(std::time::Duration::from_secs(600))
            .send()
            .await?
            .error_for_status()?;
        if response.url().scheme() != "https" {
            return Err(InfrastructureError::InvalidManifest(format!(
                "profile `{}` download redirected to a non-HTTPS URL",
                profile.profile_id
            )));
        }
        let mut file = fs::File::create(path)
            .await
            .map_err(|error| InfrastructureError::io(path, error))?;
        let mut digest = Sha256::new();
        let mut received = 0usize;
        while let Some(chunk) = response.chunk().await? {
            received = received.saturating_add(chunk.len());
            if received > 512 * 1024 * 1024 {
                return Err(InfrastructureError::InvalidManifest(
                    "model download exceeds 512 MiB".into(),
                ));
            }
            digest.update(&chunk);
            file.write_all(&chunk)
                .await
                .map_err(|error| InfrastructureError::io(path, error))?;
        }
        file.flush()
            .await
            .map_err(|error| InfrastructureError::io(path, error))?;
        file.sync_all()
            .await
            .map_err(|error| InfrastructureError::io(path, error))?;
        drop(file);
        let actual = hex_lower(&digest.finalize());
        let expected = profile.sha256.to_ascii_lowercase();
        if actual != expected {
            return Err(InfrastructureError::HashMismatch {
                profile: profile.profile_id.clone(),
                expected,
                actual,
            });
        }
        Ok(())
    }
}

static NEXT_DOWNLOAD_ID: AtomicU64 = AtomicU64::new(0);

/// Also removes incomplete artifacts when the async download future is cancelled.
struct PartialDownload(PathBuf);
impl Drop for PartialDownload {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub(crate) async fn hash_file(path: &Path) -> Result<String, InfrastructureError> {
    let mut file = fs::File::open(path)
        .await
        .map_err(|error| InfrastructureError::io(path, error))?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let bytes_read = file
            .read(&mut buffer)
            .await
            .map_err(|error| InfrastructureError::io(path, error))?;
        if bytes_read == 0 {
            break;
        }
        digest.update(&buffer[..bytes_read]);
    }
    Ok(hex_lower(&digest.finalize()))
}

async fn publish_verified_model(
    temporary_path: &Path,
    final_path: &Path,
) -> Result<(), InfrastructureError> {
    if !final_path.exists() {
        return fs::rename(temporary_path, final_path)
            .await
            .map_err(|error| InfrastructureError::io(final_path, error));
    }

    let backup_path = final_path.with_file_name(format!(
        ".{}.{}.{}.previous",
        final_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("model"),
        std::process::id(),
        NEXT_DOWNLOAD_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::rename(final_path, &backup_path)
        .await
        .map_err(|error| InfrastructureError::io(final_path, error))?;
    match fs::rename(temporary_path, final_path).await {
        Ok(()) => {
            let _ = fs::remove_file(backup_path).await;
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(&backup_path, final_path).await;
            Err(InfrastructureError::io(final_path, error))
        }
    }
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
