# Atomusicator identity and managed storage

Atomusicator is an audio workbench for five-stem extraction, synchronized mixing,
live pitch and export. Vocal tracks are two of its stem types; the former
application's name does not define the product or its active storage namespace.

## Current directories

| Data | Windows location |
| --- | --- |
| Managed projects | `%LOCALAPPDATA%\Atomusicator\projects` |
| Verified models | `%LOCALAPPDATA%\Atomusicator\models` |
| Native settings | `%LOCALAPPDATA%\Atomusicator\settings.json` |
| Job recovery | `%LOCALAPPDATA%\Atomusicator\jobs` |
| Processing cache | `%LOCALAPPDATA%\Atomusicator\cache` |
| Live pitch settings | `%LOCALAPPDATA%\Atomusicator\live-pitch` |

The displayed desktop title and Windows executable product description are
Atomusicator. Package identifiers, executable filenames and model/stem identifiers
keep their existing machine-readable spelling to preserve contracts and sessions.

## Historical data compatibility

The previous filesystem/registry identifier is isolated in the Rust
`storage::legacy_import` adapter. It is used only to discover existing data, not
as a default directory or visible product name.

On initialization, compatible completed session folders are copied into the
Atomusicator project directory. Each import:

- Validates the five stems, matching audio formats and available checksums.
- Copies regular files through an unpublished temporary directory and verifies
  copied file hashes. Linked/escaping files and incomplete or invalid sessions
  are not published; original data is retained.
- Converts absolute stem references into bundle-relative paths and validates
  the copied bundle before publication.
- Records a versioned import fingerprint for idempotent restarts. Name collisions
  use a deterministic import suffix and never overwrite a native project.
- Redirects old session bookmarks to the imported bundle and carries compatible
  live pitch settings into the new session identity.
- Redirects resumed jobs away from historical managed destinations and records
  the imported output location when reopening an already published session.

Historical model files are hash-verified and copied into the native models
directory when the model is first requested; subsequent requests use that copy.
This reuses downloaded weights without a new network download. Imports require
enough disk space for the new copies and temporary staging.

Old managed directories are no longer accepted as active workspace destinations.
An explicitly configured custom directory outside those managed historical
locations is preserved. Export destinations remain selected by the user.

Original historical folders are retained as a recovery source. They are not
renamed or deleted automatically. Skipped imports are reported in native
diagnostics and can be retried after fixing the source or storage problem.
Importing a source audio file still retains its original location; creating a
separation saves the Atomusicator session and its generated stems.

## Verification on the Windows development host

The namespace/import changes passed 129 Rust tests and 39 Flutter/native tests,
including conflict handling, absolute stem rebasing, old bookmark resolution,
per-session pitch preservation and no-download model reuse. Static analysis and
Clippy completed without findings. Debug and Release builds were prepared.

A two-second authorized FLAC excerpt completed CPU separation into five stems
under the Atomusicator project root, and the four verified models were reused
from their native-directory copies. Windows product metadata reports
Atomusicator. Android/iOS/macOS display-name configuration was checked as valid
XML; these platforms were not built or qualified in this Windows check.
