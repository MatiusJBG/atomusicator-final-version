//! Bounded, cancellable file hashing shared by artifact and tool adapters.
use atomusicator_application::CancellationToken;
use atomusicator_domain::DomainError;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

pub(crate) fn file_hash(path: &Path, cancel: &CancellationToken) -> Result<String, DomainError> {
    let failure = |error: std::io::Error| DomainError::AudioExportFailure(error.to_string());
    let mut file = File::open(path).map_err(failure)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        if cancel.is_cancelled() {
            return Err(DomainError::SeparationCancelled);
        }
        let count = file.read(&mut buffer).map_err(failure)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
