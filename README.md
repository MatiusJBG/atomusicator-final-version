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
16/24-bit WAV/FLAC, 320 kbps MP3 and optional loudness normalization.

Separation, pitch and export run as cancellable background jobs. Results are
published as complete batches. The current connected playback path holds stems
in memory; bounded streaming, adaptive GPU/resource policy, partial recovery,
historical QSettings preferences and musical quality benchmarks remain pending.
See the audit for the precise differences from Python. No quality or performance
equivalence is implied by a successful reconstruction test.

## Windows packaging

On a Windows development machine with Flutter, Rust, the Visual Studio C++
build tools, and Inno Setup 6 installed, run this from any working directory:

```powershell
& "<repository>\packaging\build_windows.ps1"
```

The script first prepares hash-verified FFmpeg/FFprobe with Rubber Band (no
Python build dependency), then resolves all project paths from its own location, builds the Flutter
release and Rust FFI library, places the bridge and ONNX Runtime DLLs beside
`atomusicator.exe`, then creates `packaging\dist\Atomusicator-Setup-0.6.0.exe`.
To build the app folder without compiling the installer, pass
`-SkipInstaller`. The installer output directory is generated locally and is
not part of the source changes.

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
