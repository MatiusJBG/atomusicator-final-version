use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum InfrastructureError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid model manifest: {0}")]
    InvalidManifest(String),
    #[error("model profile `{0}` was not found")]
    ModelNotFound(String),
    #[error("model profile `{0}` has an invalid SHA-256 digest")]
    InvalidDigest(String),
    #[error("download failed: {0}")]
    Download(#[from] reqwest::Error),
    #[error("model hash mismatch for `{profile}`: expected {expected}, got {actual}")]
    HashMismatch {
        profile: String,
        expected: String,
        actual: String,
    },
    #[error("the workspace lock is already held")]
    WorkspaceLocked,
    #[error("invalid cache entry: {0}")]
    InvalidCache(String),
    #[error("ONNX Runtime error: {0}")]
    OnnxRuntime(String),
    #[error("domain data could not be represented: {0}")]
    Domain(String),
}

impl InfrastructureError {
    pub fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
