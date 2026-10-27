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

## Migration status

The domain, application ports, separation orchestration, migration persistence
and caching, STFT/iSTFT, phase subtraction, bleed-gate rebalance, ONNX session
pool, audio file I/O, waveform peaks, and a responsive workbench are present.
The Flutter workbench currently imports files into UI state only; it does not
invoke the Rust engine.

The FFI bindings have not been generated because the installed
`flutter_rust_bridge_codegen` currently fails on this Windows checkout while
resolving the Rust output path. The planned Mel-Band RoFormer / HTDemucs or SCNet
model profile also remains unconfigured until its model artifact, license,
download URL, and SHA-256 can be verified. Consequently, model inference, native
playback and pitch shifting, batch export, packaging, and release remain future
work.
