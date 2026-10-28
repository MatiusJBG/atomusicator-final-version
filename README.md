# Atomusicator

Atomusicator is being migrated from the legacy Python desktop application to a
Flutter presentation layer and a modular Rust audio engine. The current branch
connects the workbench to native five-stem inference, playback, pitch rendering
and stem export. Migration parity and the remaining work are tracked in
[the migration audit](docs/migration-audit.md).

## Repository layout

- `rust_core/crates/domain`: audio value objects, session entities, and invariants.
- `rust_core/crates/application`: use cases and infrastructure-independent ports.
- `rust_core/crates/infrastructure`: persistence, model manifest validation,
  verified downloads, content-addressed audio caching, ONNX session pooling,
  audio file I/O, DSP, and waveform peak caching.
- `rust_core/crates/bootstrap_ffi`: Flutter-facing Rust API and composition root.
- `lib`: Flutter workbench, theme, and Riverpod state.
- `assets/manifests`: migrated model license manifest.

## Development checks

```powershell
cargo fmt --manifest-path rust_core/Cargo.toml --all -- --check
cargo clippy --manifest-path rust_core/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --manifest-path rust_core/Cargo.toml --workspace
flutter pub get
flutter analyze
flutter test
```

The Rust workspace uses Rust 2024. The Flutter app requires Dart 3.7 or newer.

## Regenerate Flutter-Rust bindings

The Flutter-Rust-Bridge runtime is pinned to 2.8.0, so use the matching code
generator version from the repository root:

```powershell
cargo install flutter_rust_bridge_codegen --version 2.8.0 --locked
flutter_rust_bridge_codegen generate
```

The generator configuration uses repository-relative paths and lets FRB place
its Rust output under the configured crate. Keep `rust_output` unset in
`flutter_rust_bridge.yaml`; an explicit path is resolved against the Flutter
root and can fail on Windows when FRB compares it with its canonical Rust
crate path. The bridge exposes only first-party API types, so it keeps FRB's
optional full-dependency/`ffigen` mode disabled and does not require LLVM for
code generation.
Check `flutter_rust_bridge_codegen --version` before regeneration: newer
generators may silently rewrite runtime dependencies. With Rust edition 2024,
2.8.0 can warn that its optional auto-upgrade step cannot parse the workspace;
generation still completes. Keep the Rust and Dart runtimes pinned to 2.8.0.

## Migration status

The workbench imports WAV/MP3/FLAC/M4A, selects a source, separates five stems
using the four reviewed legacy MDX profiles, and reopens native or Python
sessions. Models are SHA-256 verified; missing models require download consent.
Existing models under the nested `Separador Vocal/Separador Vocal/models`
directory are reused without moving them.

Playback exposes synchronized play/pause/stop/seek, track gain, mute/solo, pan,
master gain and a real master meter. Waveforms contain native audio peaks and
share a timeline with 100–400% zoom. Offline pitch uses FFmpeg Rubber Band,
preserves tempo, optionally preserves formants, and never overwrites original
stems. Export supports selected WAV/FLAC/MP3 stems, 44.1/48 kHz, mono/stereo,
16/24-bit PCM WAV, 32-bit float WAV, 16/24-bit FLAC, 320 kbps MP3 and optional
loudness normalization. Float WAV preserves stem headroom and relative levels
without normalization. The export dialog reports measured peaks above 0 dBFS
and explains that per-file loudness normalization can change stem balances.

Separation, pitch and export run as cancellable background jobs. Results are
published as complete batches. Decode, MDX overlap-add, residual processing and
playback use bounded blocks backed by files. One playback producer shares clock,
seek generation and filtered resampling across all five stems. Multilevel peak
indexes supply waveform detail as the timeline zooms.

Typed job states and a durable journal offer resume/discard after interruption.
Compatible verified stages are reused; damaged final bundles can be rebuilt from
the original. Owned decode/publication staging is cleaned without deleting
originals or confirmed sessions. Pitch and stage caches retain active readers.
Native preferences adopt historical QSettings projects without Python or Qt.

CPU uses conservative resource admission and serial inference; accelerators stay
unqualified. Independent musical-quality evaluation, a full long-duration model
campaign, physical device reconnection and clean-Windows qualification remain
open. See the audit for measured evidence and precise differences from Python.
Reconstruction alone does not establish extraction quality.

## Windows packaging

On a Windows development machine with PowerShell 7, Flutter, Rust, the Visual Studio C++
build tools, and Inno Setup 6 installed, run this from any working directory:

```powershell
& "<repository>\packaging\build_windows.ps1"
```

The script first prepares hash-verified FFmpeg/FFprobe with Rubber Band (no
Python build dependency), then resolves all project paths from its own location, builds the Flutter
release and Rust FFI library, places the bridge, CPU ONNX Runtime and app-local MSVC DLLs beside
`atomusicator.exe`, then creates `packaging\dist\Atomusicator-Setup-0.6.0.exe`.
To build the app folder without compiling the installer, pass
`-SkipInstaller`. The installer output directory is generated locally and is
not part of the source changes.
The release includes a SHA-256 file manifest. Validate it and exercise startup with
`packaging/validate_windows_release.ps1 -ReleaseDirectory '<release-folder>' -Launch`.
`packaging/test_windows_installer.ps1` accepts `-ReleaseDirectory` and `-InnoCompiler`
to test an isolated current-user installation, installer version upgrade and
uninstall with user-data fixtures. These host checks do not replace clean-Windows testing.

An existing verified runtime can be reused with
`-FfmpegRuntimeDirectory '<directory containing bin, LICENSE and README.txt>'`.
Otherwise the preparer downloads the pinned archive. Its source, hashes and
GPL-3.0-or-later distribution notices are retained in `audio-tools` beside the
installed app. Models and user audio are not bundled.

To test the complete app folder, run `packaging/build_windows.ps1 -SkipInstaller`
and open `build/windows/x64/runner/Release/atomusicator.exe`. A bare
`flutter run -d windows` does not copy the native libraries or audio tools.
For development, place the built bridge, ONNX DLLs and `audio-tools` beside the
debug executable, or test the prepared release folder.
