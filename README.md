# Atomusicator

Atomusicator is being migrated from the legacy Python desktop application to a
Flutter presentation layer and a modular Rust audio engine. The current branch
contains the architecture foundation and an early workbench; it is not yet a
working five-stem separation release.

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

## Migration status

The domain, application ports, separation orchestration, migration persistence
and caching, STFT/iSTFT, phase subtraction, bleed-gate rebalance, ONNX session
pool, audio file I/O, waveform peaks, and a responsive workbench are present.
The Flutter workbench supports audio selection, drag-and-drop, and source
management. It loads the native bridge from the directory containing the
application executable, so launching the app from another working directory
does not change native library lookup. The current bridge reports engine
metadata only; it does not expose separation or playback operations. The
planned model profile also remains unconfigured until its artifact, license,
download URL, and SHA-256 can be verified. Therefore stem separation, native
playback, pitch shifting, and batch export are not available yet.

## Windows packaging

On a Windows development machine with Flutter, Rust, the Visual Studio C++
build tools, and Inno Setup 6 installed, run this from any working directory:

```powershell
& "<repository>\packaging\build_windows.ps1"
```

The script resolves all project paths from its own location, builds the Flutter
release and Rust FFI library, places the bridge and ONNX Runtime DLLs beside
`atomusicator.exe`, then creates `packaging\dist\Atomusicator-Setup-0.6.0.exe`.
To build the app folder without compiling the installer, pass
`-SkipInstaller`. The installer output directory is generated locally and is
not part of the source changes.
