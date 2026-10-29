//! Read-only compatibility boundary for previous application data. Active data
//! always belongs to Atomusicator; originals are retained until user cleanup.
use super::{checksum::file_hash, paths::WorkspacePathPolicy};
use crate::{audio_tools::TemporaryDirectory, errors::InfrastructureError};
use atomusicator_application::CancellationToken;
use fs4::fs_std::FileExt;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

/// Historical on-disk/registry identifier, not a current product name.
pub const PREVIOUS_APP_DIRECTORY: &str = "Separador Vocal";

pub fn data_roots(base: &Path) -> Vec<PathBuf> {
    let flat = base.join(PREVIOUS_APP_DIRECTORY);
    [flat.join(PREVIOUS_APP_DIRECTORY), flat]
        .into_iter()
        .filter(|path| path.is_dir())
        .collect()
}

pub fn registry_keys() -> [String; 2] {
    [
        r"Software\Atomusicator\Atomusicator\projects".into(),
        format!(r"Software\{0}\{0}\projects", PREVIOUS_APP_DIRECTORY),
    ]
}

pub fn import_projects(policy: &WorkspacePathPolicy) -> Result<usize, InfrastructureError> {
    let target = policy.native_projects_dir();
    fs::create_dir_all(&target).map_err(failure)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(policy.app_data_root().join(".project-import.lock"))
        .map_err(failure)?;
    lock.lock_exclusive().map_err(failure)?;
    let mut imported = 0;
    for root in policy.legacy_project_dirs() {
        let entries = match fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) => {
                eprintln!("Historical projects cannot be inspected: {error}");
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("Historical project entry unavailable: {error}");
                    continue;
                }
            };
            let source = entry.path();
            if !source.join("session.json").is_file() {
                continue;
            }
            match import_project(&source, &target) {
                Ok(true) => imported += 1,
                Ok(false) => {}
                Err(error) => {
                    eprintln!("Historical session was retained without importing: {error}")
                }
            }
        }
    }
    Ok(imported)
}

fn import_project(source: &Path, target: &Path) -> Result<bool, InfrastructureError> {
    if fs::symlink_metadata(source)
        .map_err(failure)?
        .file_type()
        .is_symlink()
    {
        return Err(failure("linked historical session is not imported"));
    }
    let source = source.canonicalize().map_err(failure)?;
    let manifest = source.join("session.json");
    if fs::metadata(&manifest).map_err(failure)?.len() > 4 * 1024 * 1024 {
        return Err(failure(
            "historical session metadata exceeds the supported size",
        ));
    }
    let manifest_hash = file_hash(&manifest, &CancellationToken::default()).map_err(failure)?;
    let key = format!(
        "{:x}",
        Sha256::digest(format!("{}\0{manifest_hash}", source.display()).as_bytes())
    );
    let marker = serde_json::json!({"version":1,"source_key":key});
    let name = source
        .file_name()
        .ok_or_else(|| failure("invalid session directory"))?;
    let mut destination = target.join(name);
    if destination.exists() {
        if same_import(&destination, &marker) {
            return Ok(false);
        }
        destination = target.join(format!("{}-import-{}", name.to_string_lossy(), &key[..16]));
        if destination.exists() {
            return if same_import(&destination, &marker) {
                Ok(false)
            } else {
                Err(failure(
                    "historical session conflicts with an existing import; originals preserved",
                ))
            };
        }
    }
    // Verify the source before copying. In particular, reject stem paths that
    // escape the session and preserve the five-stem/checksum contract.
    crate::session_reader::read_session_files(&source, &CancellationToken::default())
        .map_err(failure)?;
    let stage = TemporaryDirectory::new(target).map_err(failure)?;
    copy_tree(&source, &stage.0, &source, 0)?;
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(stage.0.join("session.json")).map_err(failure)?)
            .map_err(failure)?;
    if metadata
        .pointer("/separation/status")
        .and_then(|status| status.as_str())
        .is_some_and(|status| status != "complete")
    {
        return Err(failure("historical session is not complete"));
    }
    if file_hash(&manifest, &CancellationToken::default()).map_err(failure)? != manifest_hash {
        return Err(failure(
            "session metadata changed during import; originals preserved",
        ));
    }
    metadata["separation"]["status"] = "complete".into();
    let stems = metadata
        .pointer_mut("/separation/stems")
        .ok_or_else(|| failure("missing stems"))?;
    let entries: Vec<&mut serde_json::Value> = match stems {
        serde_json::Value::Array(entries) => entries.iter_mut().collect(),
        serde_json::Value::Object(entries) => entries.values_mut().collect(),
        _ => return Err(failure("invalid stems")),
    };
    for entry in entries {
        let original = entry["path"]
            .as_str()
            .ok_or_else(|| failure("invalid stem path"))?;
        let resolved = source.join(original).canonicalize().map_err(failure)?;
        let relative = resolved.strip_prefix(&source).map_err(failure)?;
        entry["path"] = relative.to_string_lossy().replace('\\', "/").into();
    }
    fs::write(
        stage.0.join("session.json"),
        serde_json::to_vec_pretty(&metadata).map_err(failure)?,
    )
    .map_err(failure)?;
    crate::session_reader::read_session_files(&stage.0, &CancellationToken::default())
        .map_err(failure)?;
    fs::write(
        stage.0.join(".atomusicator-import.json"),
        serde_json::to_vec(&marker).map_err(failure)?,
    )
    .map_err(failure)?;
    fs::rename(&stage.0, &destination).map_err(failure)?;
    Ok(true)
}

fn same_import(directory: &Path, expected: &serde_json::Value) -> bool {
    read_marker(directory).is_some_and(|marker| marker == *expected)
}

fn read_marker(directory: &Path) -> Option<serde_json::Value> {
    let mut bytes = Vec::new();
    fs::File::open(directory.join(".atomusicator-import.json"))
        .ok()?
        .take(4097)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 4096 {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

/// Resolve historical bookmarks to the imported bundle without making the
/// historical tree an active workspace again.
pub fn resolve_session(
    policy: &WorkspacePathPolicy,
    source: &Path,
) -> Result<PathBuf, InfrastructureError> {
    if !policy.is_historical_projects_path(source) {
        return Ok(source.to_path_buf());
    }
    import_projects(policy)?;
    let canonical = source.canonicalize().map_err(failure)?;
    for entry in fs::read_dir(policy.native_projects_dir()).map_err(failure)? {
        let directory = entry.map_err(failure)?.path();
        if historical_source(policy, &directory).as_deref() == Some(canonical.as_path()) {
            return Ok(directory);
        }
    }
    Err(failure(
        "session could not be imported into Atomusicator; original retained",
    ))
}

/// Read-only origin lookup also lets the composition layer carry per-session
/// settings forward when a copied bundle is opened from the new project list.
pub fn historical_source(policy: &WorkspacePathPolicy, destination: &Path) -> Option<PathBuf> {
    let marker = read_marker(destination)?;
    if marker["version"] != 1 {
        return None;
    }
    let expected = marker["source_key"].as_str()?;
    for root in policy.legacy_project_dirs() {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let source = entry.path();
            let manifest = source.join("session.json");
            let Ok(metadata) = fs::metadata(&manifest) else {
                continue;
            };
            if metadata.len() > 4 * 1024 * 1024 {
                continue;
            }
            let Ok(source) = source.canonicalize() else {
                continue;
            };
            let Ok(hash) = file_hash(&manifest, &Default::default()) else {
                continue;
            };
            let key = format!(
                "{:x}",
                Sha256::digest(format!("{}\0{hash}", source.display()).as_bytes())
            );
            if key == expected {
                return Some(source);
            }
        }
    }
    None
}

fn copy_tree(
    source: &Path,
    target: &Path,
    boundary: &Path,
    depth: usize,
) -> Result<(), InfrastructureError> {
    if depth > 64 {
        return Err(failure(
            "historical session exceeds the supported directory depth",
        ));
    }
    for entry in fs::read_dir(source).map_err(failure)? {
        let entry = entry.map_err(failure)?;
        let kind = entry.file_type().map_err(failure)?;
        if kind.is_symlink()
            || !entry
                .path()
                .canonicalize()
                .map_err(failure)?
                .starts_with(boundary)
        {
            return Err(failure(
                "linked or escaping historical files are not imported",
            ));
        }
        let destination = target.join(entry.file_name());
        if kind.is_dir() {
            fs::create_dir(&destination).map_err(failure)?;
            copy_tree(&entry.path(), &destination, boundary, depth + 1)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &destination).map_err(failure)?;
            let cancel = CancellationToken::default();
            if file_hash(&entry.path(), &cancel).map_err(failure)?
                != file_hash(&destination, &cancel).map_err(failure)?
            {
                return Err(failure(
                    "historical file changed while importing; originals preserved",
                ));
            }
        } else {
            return Err(failure("unsupported historical file type"));
        }
    }
    Ok(())
}

pub async fn import_verified_model(
    source: &Path,
    target: &Path,
    expected: &str,
) -> Result<PathBuf, InfrastructureError> {
    let root = target
        .parent()
        .ok_or_else(|| failure("invalid native model path"))?;
    fs::create_dir_all(root).map_err(failure)?;
    let stage = TemporaryDirectory::new(root).map_err(failure)?;
    let temporary = stage.0.join("model.onnx");
    tokio::fs::copy(source, &temporary).await.map_err(failure)?;
    if !crate::model_downloader::hash_file(&temporary)
        .await?
        .eq_ignore_ascii_case(expected)
    {
        return Err(failure("historical model changed while importing"));
    }
    fs::rename(&temporary, target).map_err(failure)?;
    Ok(target.to_path_buf())
}

fn failure(error: impl std::fmt::Display) -> InfrastructureError {
    InfrastructureError::Domain(format!("Atomusicator data import: {error}"))
}
