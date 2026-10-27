use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::{fs, io::AsyncWriteExt};

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
        let result = self.download_to(profile, &temporary_path).await;
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path).await;
        }
        result?;
        if final_path.exists() {
            fs::remove_file(&final_path)
                .await
                .map_err(|error| InfrastructureError::io(&final_path, error))?;
        }
        fs::rename(&temporary_path, &final_path)
            .await
            .map_err(|error| InfrastructureError::io(&final_path, error))?;
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
            .send()
            .await?
            .error_for_status()?;
        let mut file = fs::File::create(path)
            .await
            .map_err(|error| InfrastructureError::io(path, error))?;
        let mut digest = Sha256::new();
        while let Some(chunk) = response.chunk().await? {
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

async fn hash_file(path: &Path) -> Result<String, InfrastructureError> {
    let bytes = fs::read(path)
        .await
        .map_err(|error| InfrastructureError::io(path, error))?;
    Ok(hex_lower(&Sha256::digest(bytes)))
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
