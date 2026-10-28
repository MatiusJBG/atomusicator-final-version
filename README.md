# Atomusicator

A Windows audio workbench with a Flutter interface and a Rust engine for
five-stem separation, synchronized mixing, pitch processing and export.
The application runs without Python or the legacy repository.

**Release status:** Windows x64 CPU candidate, version 0.6.0. Independent vocal
quality/listening, full 30–60 minute model runs, clean-Windows installation and
physical device reconnection remain qualification gates. The current installer
also needs the complete third-party notice/source bundle before publication.
See [migration evidence](docs/migration-audit.md),
[distribution requirements](docs/distribution-compliance.md) and the
[manual release procedure](docs/manual-release.md).

## Contents

- [Use the application](#use-the-application)
- [Set up a development machine](#set-up-a-development-machine)
- [Clone, build and run](#clone-build-and-run)
- [Development and verification](#development-and-verification)
- [Architecture](#architecture)
- [Troubleshooting](#troubleshooting)
- [Licensing and releases](#licensing-and-releases)

## Use the application

The qualified distribution target is **Windows x64 with CPU inference**. Other
platform folders exist in the Flutter project but are not qualified releases.
GPU acceleration remains unqualified.

When a release is published, download its installer from
[GitHub Releases](https://github.com/MatiusJBG/atomusicator-final-version/releases)
and verify its SHA-256 against the accompanying checksum file. Until then,
build the application using the steps below. Do not download an unrelated
binary simply because it has a similar name.

1. Launch Atomusicator. Import **WAV, MP3, FLAC or M4A**, drop files anywhere in
   the workbench, or copy files in Windows Explorer and press **Ctrl+V** in the
   workbench. The first file in the latest import becomes the selected source.
2. Select the source and choose **Separate stems**. Missing models require
   explicit download consent and are verified against the model manifest.
   Internet is needed for first-time model downloads; verified cached models
   can be reused offline. Weights are not included in the installer.
3. Mix lead vocals, backing vocals, drums, bass and instrumental using play,
   pause, stop, seek, gain, mute, solo, pan and master volume. Waveforms share a
   synchronized timeline with 100–400% zoom.
4. Apply pitch changes with optional formant preservation. Original stems are
   retained. Separation, pitch and export are cancellable background jobs.
5. Export selected stems as WAV, FLAC or MP3. WAV supports 16/24-bit PCM and
   32-bit float; FLAC supports 16/24-bit; MP3 uses 320 kbps. Choose 44.1/48 kHz,
   mono/stereo and optional loudness normalization. Float WAV preserves
   headroom; normalization of each stem separately can change their balance.
6. Reopen saved sessions. After an interrupted job, follow the recovery controls
   to resume or discard it. Keep original audio available for rebuilding stages.

Native data defaults under `%LOCALAPPDATA%\Atomusicator`; project preferences
can adopt an existing legacy project location. The application reuses compatible
verified legacy models without moving them. No Python installation is required.

## Set up a development machine

Use Windows x64 and **PowerShell 7** (`pwsh`) for the commands below. Install:

| Tool | Requirement / purpose |
| --- | --- |
| Git | Clone and review source changes |
| Flutter SDK | Stable Windows SDK; this machine was checked with Flutter 3.47.0 / Dart 3.13.0. Package constraint: Dart 3.7 or newer |
| Rust | MSVC host toolchain, edition 2024 support; this machine was checked with rustc 1.99.1. Older toolchains have not been qualified against the lockfile |
| Visual Studio | Desktop development with C++, MSVC x64 tools, CMake tools and a supported Windows SDK |
| Inno Setup 6 | Required only to create the installer; not needed for `-SkipInstaller` |

Install through the official [Flutter Windows setup guide](https://docs.flutter.dev/platform-integration/windows/setup),
[Rust installer](https://www.rust-lang.org/tools/install),
[PowerShell installation guide](https://learn.microsoft.com/en-us/powershell/scripting/install/installing-powershell-on-windows)
and [Inno Setup download](https://jrsoftware.org/isdl.php).
Add Flutter's `bin` and Cargo's `bin` to your user PATH and reopen PowerShell.
Visual Studio Code alone does not provide the native C++ toolchain.

Check the machine before building:

```powershell
pwsh --version
git --version
flutter --version
flutter config --enable-windows-desktop
flutter doctor -v
flutter devices
rustup show active-toolchain
rustc --version
cargo --version
```

`flutter doctor -v` must report a working Windows/Visual Studio toolchain, and
`flutter devices` must list Windows. Android/mobile toolchain warnings do not
block the qualified Windows build. Use the Rust MSVC target, normally
`stable-x86_64-pc-windows-msvc`, rather than a GNU host toolchain.

## Clone, build and run

Access to this proprietary repository does not grant a distribution license;
obtain permission from the owner before use or redistribution.

```powershell
git clone https://github.com/MatiusJBG/atomusicator-final-version.git
Set-Location atomusicator-final-version
git switch feature/complete-python-migration
flutter pub get
cargo fetch --locked --manifest-path rust_core/Cargo.toml
./packaging/build_windows.ps1 -SkipInstaller
./build/windows/x64/runner/Release/atomusicator.exe
```

All subsequent commands assume the repository root. If a final release uses a
different branch, check out its published tag instead of assuming the feature
branch is stable. Keep `pubspec.lock` and `rust_core/Cargo.lock` in source control.
Do not run dependency upgrades as part of normal setup.

The packaging script downloads the pinned, SHA-256-verified FFmpeg/FFprobe build
with Rubber Band and MP3 support, builds Flutter and the Rust FFI DLL, and copies
CPU ONNX Runtime, the MSVC runtime and audio tools beside the application.
The first build needs internet access for package registries/native dependencies
and can take substantially longer than subsequent builds. Free disk space must
cover SDKs, the Cargo build cache, model downloads, decoded audio and output
stems; final minimum disk/RAM recommendations await qualification measurements.

To create the installer as well:

```powershell
./packaging/build_windows.ps1
```

If Inno Setup is installed in a custom location:

```powershell
./packaging/build_windows.ps1 -InnoCompiler 'C:/Tools/Inno Setup 6/ISCC.exe'
```

Outputs:

| Output | Location |
| --- | --- |
| Complete runnable app | `build/windows/x64/runner/Release/` |
| App executable | `build/windows/x64/runner/Release/atomusicator.exe` |
| Windows installer | `packaging/dist/Atomusicator-Setup-0.6.0.exe` |
| Runtime checksum inventory | `build/windows/x64/runner/Release/release-manifest.json` |

Keep the **whole release folder** together when running or creating a portable
ZIP. Copying only the EXE loses required DLLs, Flutter assets and audio tools.
Build outputs are ignored by Git and should be uploaded as release assets only
after qualification and license review. Models and user recordings stay local.

To reuse an already downloaded verified audio runtime, pass
`-FfmpegRuntimeDirectory 'C:/path/to/runtime'`; that folder must contain `bin`,
`LICENSE` and `README.txt` matching `packaging/ffmpeg-runtime.json`.

## Development and verification

### Normal checks

```powershell
cargo fmt --manifest-path rust_core/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path rust_core/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --locked --manifest-path rust_core/Cargo.toml --workspace
flutter analyze
flutter test
```

The default tests do not prove perceptual extraction quality or clean-machine
compatibility. Some native/model tests are opt-in; report skipped tests as
skipped rather than counting them as passes.

### Flutter tests with the compiled native bridge

First build the complete release folder as above, then:

```powershell
$releaseDirectory = (Resolve-Path build/windows/x64/runner/Release).Path
$env:ATOMUSICATOR_NATIVE_LIBRARY = Join-Path $releaseDirectory 'atomusicator_bootstrap_ffi.dll'
$env:ATOMUSICATOR_FFMPEG = Join-Path $releaseDirectory 'audio-tools/bin/ffmpeg.exe'
flutter test
./packaging/validate_windows_release.ps1 -ReleaseDirectory $releaseDirectory -Launch
```

The release validator verifies the file manifest, required runtime files,
absence of Python and startup with a restricted PATH. It does not replace a
fresh Windows installation or the outstanding licensing audit.

For the isolated host installer/upgrade/uninstall test, supply the actual Inno
compiler path:

```powershell
./packaging/test_windows_installer.ps1 -ReleaseDirectory $releaseDirectory -InnoCompiler 'C:/Tools/Inno Setup 6/ISCC.exe'
```

This uses an isolated installation identity and user-data fixtures. It is not
a clean OS test or proof of upgrading every historical production installer.

### Iterating with Flutter

The simplest complete local run is the prepared release executable. A bare
`flutter run -d windows` does not place all native dependencies beside its debug
executable. If using debug/hot reload, prepare a release first, build the debug
runner, and copy its runtime dependencies:

```powershell
cargo build --locked --manifest-path rust_core/Cargo.toml -p atomusicator-bootstrap-ffi
flutter build windows --debug
$releaseDirectory = (Resolve-Path build/windows/x64/runner/Release).Path
$debugDirectory = (Resolve-Path build/windows/x64/runner/Debug).Path
Get-ChildItem -LiteralPath $releaseDirectory -Filter '*.dll' |
    Where-Object Name -Match '^(atomusicator_bootstrap_ffi|onnxruntime.*|msvcp140|vcruntime140.*|concrt140)\.dll$' |
    Copy-Item -Destination $debugDirectory -Force
Copy-Item -LiteralPath "$releaseDirectory/audio-tools" -Destination $debugDirectory -Recurse -Force
flutter run -d windows
```

The example uses the prepared release Rust DLL as the engine baseline. To test
Rust edits in debug, locate Cargo's `target_directory` with
`cargo metadata --manifest-path rust_core/Cargo.toml --no-deps --format-version 1`,
then copy `debug/atomusicator_bootstrap_ffi.dll` and its matching ONNX DLLs from
that directory into the Flutter debug directory. Recopy after Rust changes;
Flutter hot reload does not rebuild the Rust engine. Stop the app before
replacing a DLL. When native APIs change, regenerate bindings as below.

### Regenerate Flutter–Rust bindings

Generated bindings are checked in; ordinary clones do not need regeneration.
For API changes, use the generator matching both pinned FRB runtimes:

```powershell
cargo install flutter_rust_bridge_codegen --version 2.8.0 --locked
flutter_rust_bridge_codegen --version
flutter_rust_bridge_codegen generate
```

Keep the Rust/Dart runtimes and generator at **2.8.0**. Leave `rust_output`
unset in `flutter_rust_bridge.yaml` to avoid Windows canonical-path conflicts.
The bridge uses first-party API types, with optional `ffigen` disabled; LLVM is
not required. FRB 2.8.0 can warn about its optional auto-upgrade parser on Rust
2024; inspect the generated diff and do not accept dependency rewrites.

## Architecture

| Directory | Responsibility |
| --- | --- |
| `lib/` | Flutter workbench, theme, Riverpod state and bridge integration |
| `rust_core/crates/domain/` | Audio/session value types and invariants |
| `rust_core/crates/application/` | Use cases and infrastructure-independent ports |
| `rust_core/crates/infrastructure/` | Audio I/O, DSP, ONNX, caching, persistence and playback |
| `rust_core/crates/bootstrap_ffi/` | Composition root and Flutter-facing native API |
| `assets/manifests/` | Model hashes, provenance, licensing and download policy |
| `test/` | Flutter/widget/native-bridge tests |
| `packaging/` | Verified audio-tool preparation, Windows builds and installer validation |
| `docs/` | Public technical evidence and distribution instructions |

Separation uses four verified legacy MDX profiles and file-backed, bounded
processing stages. Playback shares a clock across all stems. Durable job state,
leases and transactional publication protect recovery and existing sessions.
See the audit for exact compatibility limits and measured evidence.

## Troubleshooting

| Symptom | Check / action |
| --- | --- |
| PowerShell rejects `#requires -Version 7.0` | Open `pwsh`, not Windows PowerShell 5.1 |
| Flutter cannot find Visual Studio or Windows | Install Desktop development with C++, Windows SDK and CMake tools; rerun `flutter doctor -v` |
| Missing native DLL / audio tools | Run `packaging/build_windows.ps1 -SkipInstaller`; launch the complete release folder |
| Model download denied / model unavailable | Grant download consent, check network and available disk; do not bypass SHA-256 checks |
| FLAC invisible in the picker | Rebuild/use the updated app; it uses an explicit WAV/MP3/FLAC/M4A filter |
| Ctrl+V imports nothing | Copy actual files in Explorer, focus the workbench and paste; copying a filename as text is a different clipboard format |
| Build cannot replace files | Close running copies of the app from that build directory and retry |
| Installer compiler missing | Install Inno Setup 6, pass `-InnoCompiler`, or use `-SkipInstaller` |
| Audio device disconnects | Record the displayed error and re-open playback as needed; automatic physical-device recovery remains under qualification |

## Licensing and releases

First-party material is proprietary; see [LICENSE](LICENSE). Third-party
licenses remain applicable independently. Consult the
[distribution inventory](docs/distribution-compliance.md) before shipping any
binary, model pack or offline dependency package. Existing Cargo metadata
formerly declared MIT; previously granted rights require separate review.

Installers and large dependency packages belong in
[GitHub Releases](https://github.com/MatiusJBG/atomusicator-final-version/releases).
The [manual release guide](docs/manual-release.md) explains branch pushes,
qualification, tagging, assets and checksums. No publication is automated.
