use std::path::Path;

use atomusicator_application::ports::{
    ModelRegistryError, ModelRegistryFuture, ModelRegistryPort, VerifiedModelSpec,
};

use crate::{
    errors::InfrastructureError,
    model_downloader::{ModelDownloader, ModelManifest, hash_file},
    storage::paths::WorkspacePathPolicy,
};

const EMBEDDED_MODEL_MANIFEST: &str =
    include_str!("../../../../../assets/manifests/model_license_manifest.json");

/// Connects the checked-in license manifest to verified models under the OS data directory.
pub struct ManifestModelManager {
    manifest: ModelManifest,
    path_policy: WorkspacePathPolicy,
    downloader: ModelDownloader,
}

impl ManifestModelManager {
    pub async fn ensure_profile_ready_cancellable(
        &self,
        profile_id: &str,
        cancel: &atomusicator_application::CancellationToken,
    ) -> Result<VerifiedModelSpec, InfrastructureError> {
        let pending = self.ensure_profile_ready(profile_id);
        tokio::pin!(pending);
        loop {
            if cancel.is_cancelled() {
                return Err(InfrastructureError::OnnxRuntime(
                    "model preparation cancelled".into(),
                ));
            }
            tokio::select! {
                result=&mut pending=>return result,
                _=tokio::time::sleep(std::time::Duration::from_millis(50))=>{}
            }
        }
    }
    pub fn from_embedded_manifest(
        path_policy: WorkspacePathPolicy,
    ) -> Result<Self, InfrastructureError> {
        Self::from_manifest_json(EMBEDDED_MODEL_MANIFEST, path_policy)
    }

    pub fn from_manifest_json(
        json: &str,
        path_policy: WorkspacePathPolicy,
    ) -> Result<Self, InfrastructureError> {
        let manifest = ModelManifest::parse(json)?;
        Ok(Self {
            manifest,
            path_policy,
            downloader: ModelDownloader::default(),
        })
    }

    pub fn with_downloader(
        json: &str,
        path_policy: WorkspacePathPolicy,
        downloader: ModelDownloader,
    ) -> Result<Self, InfrastructureError> {
        let manifest = ModelManifest::parse(json)?;
        Ok(Self {
            manifest,
            path_policy,
            downloader,
        })
    }

    pub fn manifest(&self) -> &ModelManifest {
        &self.manifest
    }

    pub async fn ensure_profile_ready(
        &self,
        profile_id: &str,
    ) -> Result<VerifiedModelSpec, InfrastructureError> {
        let profile = self.manifest.profile(profile_id)?;
        let expected_hash = profile.sha256.to_ascii_lowercase();

        for candidate in self.path_policy.model_candidates(&profile.filename)? {
            if !candidate.is_file() {
                continue;
            }
            if let Ok(actual_hash) = hash_file(&candidate).await
                && actual_hash.eq_ignore_ascii_case(&expected_hash)
            {
                return Ok(verified_spec(profile, candidate, expected_hash));
            }
        }

        let local_path = self
            .downloader
            .download_verified(profile, &self.path_policy.models_dir())
            .await?;
        Ok(verified_spec(profile, local_path, expected_hash))
    }

    pub async fn verify_model_file(path: &Path) -> Result<String, InfrastructureError> {
        hash_file(path).await
    }
}

impl ModelRegistryPort for ManifestModelManager {
    fn ensure_profile_ready<'a>(&'a self, profile_id: &'a str) -> ModelRegistryFuture<'a> {
        Box::pin(async move {
            match self.ensure_profile_ready(profile_id).await {
                Ok(spec) => Ok(spec),
                Err(InfrastructureError::ModelNotFound(id)) => {
                    Err(ModelRegistryError::ProfileNotFound(id))
                }
                Err(error) => Err(ModelRegistryError::Unavailable(error.to_string())),
            }
        })
    }
}

fn verified_spec(
    profile: &crate::model_downloader::ModelProfile,
    local_path: std::path::PathBuf,
    sha256_hash: String,
) -> VerifiedModelSpec {
    VerifiedModelSpec {
        profile_id: profile.profile_id.clone(),
        stage: profile.stage.clone(),
        local_path,
        sha256_hash,
        license: profile.license.clone(),
        attribution: profile.attribution.clone(),
    }
}
