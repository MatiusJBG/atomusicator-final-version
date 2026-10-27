# Architecture and Migration Master Plan: Atomusicator (Rust + Flutter)

* **Target Repository:** `https://github.com/MatiusJBG/atomusicator-final-version`
* **Reference Repository (Legacy):** `MatiusJBG/atomusicator-jbg-version` (branch `feature/atomusicator-performance`, commit `6ac6221`)
* **Technology Stack:** Rust (Edition 2024) + Flutter (Desktop & Mobile) + `flutter_rust_bridge` v2 + Native ONNX Runtime (`ort`)
* **Architectural Paradigm:** Clean Architecture + Strict SOLID Principles + Tactical Domain-Driven Design (DDD)

---

## 1. Architectural Vision and Responsibility Distribution

In the legacy Python version, the application was designed as a modular monolith under Clean Architecture using `multiprocessing` in `spawn` mode (`ProcessSeparationService`) to isolate heavy computation from the `PySide6` user interface. By migrating to the **Rust + Flutter** stack, we completely eliminate the Python interpreter and the Global Interpreter Lock (GIL), allowing heavy concurrency to run on native shared-memory threads (*Zero-Copy*) within a single compiled binary.

### Golden Rule of the FFI Boundary (Flutter vs. Rust)

1. **The Core Lives in Rust (`rust_core`):** Contains 100% of the `domain`, `application` (use cases and ports), and `infrastructure` layers (ONNX inference on GPU/CPU, DSP, STFT/iSTFT, phase subtraction, real-time mixing, pitch shifting, session I/O, and caching).
2. **The Presentation Lives in Flutter (`flutter_ui`):** Exclusively replaces the legacy `src/atomusicator/ui/` layer built with `PySide6` and its `QObject` workers. Flutter never processes raw audio buffers or executes business rules; it only dispatches asynchronous commands to Rust via FFI and subscribes to state *Streams* (separation progress, RMS peak meters, and playhead position).

---

## 2. Repository Structure (`atomusicator-final-version`)

The project is organized as a **Hybrid Workspace** where Dart packages and modular Rust crates coexist, ensuring the compiler enforces architectural boundaries at build time:

```text
atomusicator-final-version/
├── .github/
│   └── workflows/                  # CI/CD: cargo test, clippy, flutter test, and Windows/Mobile builds
├── assets/
│   └── manifests/
│       └── model_license_manifest.json  # Migrated intact from the legacy Python repository
├── rust_core/                      # Rust Workspace (Native Engine + Clean Architecture)
│   ├── Cargo.toml                  # Workspace definition and shared dependencies
│   ├── crates/
│   │   ├── domain/                 # [Layer 1] Pure entities, Value Objects, and business errors (0 external deps)
│   │   ├── application/            # [Layer 2] Use cases, orchestrators, and segregated Traits (Ports)
│   │   ├── infrastructure/         # [Layer 3] Adapters: ONNX (ort), DSP, Audio I/O (cpal), Cache, FS
│   │   └── bootstrap_ffi/          # [Layer 4] Dependency Injection (Composition Root) and Flutter FFI API
├── lib/                            # Flutter Application (SOLID Presentation Layer)
│   ├── main.dart
│   ├── core/
│   │   ├── di/                     # Dependency Injection container (Riverpod providers)
│   │   └── theme/                  # Visual design system
│   ├── src/
│   │   ├── rust_bridge/            # Auto-generated bindings by flutter_rust_bridge
│   │   └── features/
│   │       ├── workbench/          # Equivalent to AudioWorkbench and track rows
│   │       ├── separation/         # Progress dialogs, stem selection, and cancellation
│   │       ├── mixer_pitch/        # Gain, mute, solo, pan, and semitone controls
│   │       └── export/             # Stem export configuration and batch rendering
├── test/                           # Flutter UI and widget tests
├── benchmarks/                     # Mathematical parity tests (Null Test) and GPU/CPU timing suites
├── packaging/                      # Inno Setup (.iss) scripts and cross-platform packaging
├── flutter_rust_bridge.yaml        # FFI code generator configuration
└── pubspec.yaml                    # Flutter dependencies and asset definitions
```

---

## 3. Strict Application of SOLID Principles

To ensure the application scales without accumulating technical debt, every layer in both Rust and Flutter must obey the following compile-time verifiable design rules:

### S — Single Responsibility Principle (SRP)
Every `struct` in Rust and `class` in Dart must have a single reason to change:
* **In the Separation Pipeline:** Instead of a monolithic class that downloads models, computes the STFT, runs ONNX sessions, and subtracts phases, the engine is decomposed into atomic components:
  1. `ModelDownloader`: Exclusively downloads and verifies SHA-256 hashes against the manifest.
  2. `StftProcessor`: Exclusively transforms `f32` PCM buffers between the time and frequency domains using Hann windowing and *Overlap-Add*.
  3. `OnnxSessionRunner`: Exclusively executes tensor inference over `ort::Session`.
  4. `PhaseSubtractor`: Exclusively runs SIMD vectorized polarity inversion operations ($I_{\text{total}} = M - V_{\text{total}}$, $S_{\text{backing}} = V_{\text{total}} - S_{\text{lead}}$, $S_{\text{inst}} = I_{\text{total}} - (S_{\text{drums}} + S_{\text{bass}})$).
  5. `SpectralBleedGate`: Exclusively cleans diffuse reverb tails from the backing vocals stem.

### O — Open/Closed Principle (OCP)
The engine must be open for extension (new models, new GPU execution providers, or new export formats) but closed for modification in its core use cases:
* **Separation Graph Strategy (`SeparationTopology`):** The `SeparateStemsUseCase` does not hardcode specific model architectures like `RoFormer` or `HTDemucs`. Instead, it executes nodes implementing the `SeparationStage` trait. Swapping `HTDemucs` for `SCNet` or adding a sixth stem only requires registering a new `SeparationStage` implementation and updating `model_license_manifest.json` without modifying the use case orchestrator.
* **Hardware Execution Providers:** Selecting between `DirectML` (universal Windows GPU), `CUDA` (NVIDIA), `CoreML` (Apple), and `CPU` is encapsulated via the *Strategy* pattern during session pool initialization.

### L — Liskov Substitution Principle (LSP)
Any implementation of a contract (`trait`) must be substitutable for another without altering program correctness or introducing unexpected side effects:
* **Transparent Polymorphism in Playback:** In the legacy architecture, small tracks load into RAM as PCM buffers while tracks larger than $256\text{ MiB}$ use `StreamingMixer` with $1024$-frame read-ahead blocks and an $8$-block queue. In Rust, both implement the `AudioTrackSource` trait:
  * `InMemoryTrackSource` (for tracks $\le 256\text{ MiB}$).
  * `BufferedStreamTrackSource` (for tracks $> 256\text{ MiB}$ backed by a lock-free ring buffer).
  * `RealtimeMixer` consumes `&mut dyn AudioTrackSource` inside the real-time audio callback without knowing whether samples originate from RAM or disk; both guarantee constant $O(1)$ retrieval time and zero I/O blocking.

### I — Interface Segregation Principle (ISP)
No use case or UI widget should depend on methods it does not use. Legacy contracts from `src/atomusicator/application/ports/` are split into cohesive, fine-grained `traits` inside the `application` crate:

```rust
// rust_core/crates/application/src/ports/mod.rs

pub trait AudioDecoderPort: Send + Sync {
    fn probe_metadata(&self, path: &Path) -> Result<AudioAssetMetadata, DomainError>;
    fn decode_to_f32(&self, path: &Path) -> Result<AudioBufferF32, DomainError>;
}

pub trait StemInferencePort: Send + Sync {
    fn infer_vocal_bus(&self, mix: &AudioBufferF32, cancel: &CancellationToken) -> Result<AudioBufferF32, DomainError>;
    fn infer_lead_vocal(&self, vocal_bus: &AudioBufferF32, cancel: &CancellationToken) -> Result<AudioBufferF32, DomainError>;
    fn infer_rhythm_section(&self, inst_bus: &AudioBufferF32, cancel: &CancellationToken) -> Result<RhythmStems, DomainError>;
}

pub trait ArtifactCachePort: Send + Sync {
    fn get_checkpoint(&self, key: &CacheKey) -> Option<AudioBufferF32>;
    fn put_checkpoint(&self, key: &CacheKey, buffer: &AudioBufferF32) -> Result<(), DomainError>;
}

pub trait RealtimePlaybackPort: Send + Sync {
    fn load_session(&self, stems: Vec<StemTrack>) -> Result<(), DomainError>;
    fn set_track_state(&self, stem_id: StemType, gain_db: f32, pan: f32, mute: bool, solo: bool);
    fn set_pitch_semitones(&self, semitones: f32, preserve_formants: bool);
}
```

### D — Dependency Inversion Principle (DIP)
High-level modules (`domain` and `application`) must never depend on low-level modules (`infrastructure`):
* The `domain` crate has **zero external infrastructure dependencies** in its `Cargo.toml`.
* The `application` crate depends exclusively on `domain`.
* The `infrastructure` crate depends on `application` and `domain`, implementing port traits using `ort`, `cpal`, `symphonia`, and `rubato`.
* The `bootstrap_ffi` crate acts as the Composition Root (equivalent to the legacy `src/atomusicator/bootstrap/services.py`): it instantiates concrete adapters from `infrastructure` and injects them via smart pointers (`Arc<dyn PortTrait>`) into `application` use cases.

---

## 4. Migration Equivalence Table: Python to Rust + Flutter

This mapping defines how every module from `MatiusJBG/atomusicator-jbg-version` translates into the new `atomusicator-final-version` repository:

| Legacy Python Component (`atomusicator-jbg-version`) | New Rust / Flutter Component (`atomusicator-final-version`) | Direct Technical Improvement |
| :--- | :--- | :--- |
| `domain/`: `AudioAsset`, `AudioSession`, `Stem`, `SeparationResult`, `SeparationJob` | Crate `domain`: Immutable `struct`s and `enum StemType { LeadVocal, BackingVocals, Drums, Bass, Instrumental }` | Compile-time type safety and sample-accurate alignment validation. |
| `ProcessSeparationService` (`multiprocessing` `spawn` + local pipe) | `tokio::task::spawn_blocking` + `rayon` ThreadPool in `infrastructure` | Eliminates IPC pipe serialization overhead; `f32` audio tensors are shared in RAM via **Zero-Copy**. |
| `AudioSeparatorEngine` (`audio-separator` 0.47.0 + `PyTorch` + `UVR/Kuielab`) | `OrtSeparationEngine` (`ort` with DirectML/CUDA/CoreML) + "3 Inferences + 3 Subtractions" Pipeline | Eliminates PyTorch; enables universal GPU acceleration on Windows and replaces 4 serial MDX stages with 3 high-fidelity inferences (RoFormer + HTDemucs) plus exact phase subtractions. |
| `SessionPool` and `StageArtifactCache` (SHA-256 + LRU quota) | `OrtSessionPool` (`Mutex<LruCache>`) and `ContentAddressedCache` (`sha2` + `memmap2`) | Preserves SHA-256 verification and LRU eviction while reading cached checkpoints from disk via memory-mapped files (`mmap`) nearly instantaneously. |
| `RealtimeMixer` and `StreamingMixer` (`sounddevice` + $1024$-frame blocks) | `CpalRealtimeMixer` (`cpal` + `rtrb` Lock-Free Ring Buffer) | Sub-$5\text{ ms}$ latency over WASAPI/ASIO/CoreAudio with zero garbage collection or interpreter pauses inside the audio callback. |
| `RenderPitch` and `PitchProcessor` (FFmpeg + offline Rubber Band filter) | **Real-time** pitch processing inside the mixer + accelerated offline export | Users can adjust semitones ($-12$ to $+12$) and formants while hearing changes live without waiting for an offline disk render. |
| `SessionStore` and `SessionWorkspace` (`%LOCALAPPDATA%\Atomusicator`) | `LocalSessionRepository` (`serde_json` + `directories` + `fs4` File Locks) | 100% backward-compatible with existing `%LOCALAPPDATA%\Atomusicator` folders, `session.json` metadata, and session locks. |
| `ui/`: `MainWindow`, `AudioWorkbench`, and `PySide6` workers | Flutter (`Riverpod` Notifiers + GPU-accelerated `CustomPainter` via Impeller/Skia) | $120\text{ FPS}$ waveform rendering and a unified codebase ready for Windows, macOS, Linux, Android, and iOS. |

---

## 5. Tooling and Library Installation

### Step 1: Repository Initialization and Base Directory Structure

Run these commands in your terminal to initialize the hybrid workspace in `atomusicator-final-version`:

```bash
# 1. Clone the target remote repository
git clone https://github.com/MatiusJBG/atomusicator-final-version.git
cd atomusicator-final-version

# 2. Install the Flutter-Rust FFI code generator
cargo install flutter_rust_bridge_codegen

# 3. Initialize the Flutter project at the root for Desktop and Mobile platforms
flutter create --org com.matiusjbg --project-name atomusicator --platforms=windows,macos,linux,android,ios .

# 4. Create the modular Rust Workspace following Clean Architecture
mkdir -p rust_core/crates/domain/src
mkdir -p rust_core/crates/application/src
mkdir -p rust_core/crates/infrastructure/src
mkdir -p rust_core/crates/bootstrap_ffi/src
```

### Step 2: Rust Workspace Dependencies (`rust_core/Cargo.toml`)

Create `rust_core/Cargo.toml` to define shared workspace dependencies selected for production audio and AI workloads:

```toml
[workspace]
resolver = "2"
members = [
    "crates/domain",
    "crates/application",
    "crates/infrastructure",
    "crates/bootstrap_ffi",
]

[workspace.package]
version = "0.6.0"
edition = "2024"
authors = ["Matias Benjamin Morales Aigaje"]
license = "MIT"

[workspace.dependencies]
# --- AI Inference Engine and Tensors (Replaces PyTorch and audio-separator) ---
# 'ort' provides official Rust bindings for ONNX Runtime 1.20+ with DirectML (Windows GPU), CUDA, and CoreML
ort = { version = "2.0.0-rc.9", features = ["directml", "cuda", "coreml", "half"] }
ndarray = { version = "0.16", features = ["rayon"] }

# --- Digital Signal Processing (DSP: STFT, iSTFT, Resampling) ---
realfft = "3.4"            # Fast Fourier Transform optimized for real-valued audio signals
apodize = "1.0"            # Exact Hann window generation for Overlap-Add reconstruction
rubato = "0.16"            # High-precision linear-phase asynchronous sample rate conversion

# --- Audio Decoding, Encoding, and Real-Time I/O ---
symphonia = { version = "0.5", features = ["all"] } # Pure Rust decoder (WAV, FLAC, MP3, OGG, AAC)
hound = "3.5"              # Fast WAV writer supporting 16/24-bit PCM and 32-bit Float
cpal = "0.15"              # Cross-platform low-latency real-time audio I/O (WASAPI/ASIO/CoreAudio)
rtrb = "0.3"               # Wait-free/Lock-free SPSC ring buffer for StreamingMixer (>256 MiB)

# --- Concurrency, Caching, Networking, and Local Persistence ---
tokio = { version = "1.43", features = ["rt-multi-thread", "sync", "fs", "macros"] }
rayon = "1.10"             # Data parallelism on CPU for SIMD phase subtractions
tokio-util = { version = "0.7", features = ["rt"] } # CancellationToken for instant inference abortion
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"         # Serialization for session.json and model_license_manifest.json
sha2 = "0.10"              # SHA-256 verification for ONNX model weights and StageArtifactCache
lru = "0.12"               # LRU quota management for SessionPool and StageArtifactCache
memmap2 = "0.9"            # Memory-mapped file I/O for fast model loading and cache reads
reqwest = { version = "0.12", features = ["stream", "rustls-tls"] } # Secure HTTPS model downloader
directories = "6.0"        # OS-standard path resolution for %LOCALAPPDATA%\Atomusicator
fs4 = "0.12"               # OS-level file locking for transactional SessionWorkspace operations

# --- FFI Bridge, Error Handling, and Diagnostics ---
flutter_rust_bridge = "=2.8.0"
thiserror = "2.0"          # Strongly-typed Domain and Infrastructure error definitions
tracing = "0.1"            # Structured logging and telemetry for GUI/headless execution
```

### Step 3: Flutter Dependencies (`pubspec.yaml`)

Configure `pubspec.yaml` with state management, dependency injection, and desktop integration packages:

```yaml
name: atomusicator
description: "High-performance desktop and mobile application for 5-stem audio separation and real-time mixing."
publish_to: 'none'
version: 0.6.0+1

environment:
  sdk: ^3.7.0

dependencies:
  flutter:
    sdk: flutter

  # --- FFI Bridge to the Rust Core ---
  flutter_rust_bridge: 2.8.0
  ffi: ^2.1.3

  # --- Reactive State Management and Dependency Injection (DIP) ---
  flutter_riverpod: ^2.6.1
  riverpod_annotation: ^2.6.1

  # --- Immutable UI Models and Contracts ---
  freezed_annotation: ^2.4.4
  json_annotation: ^4.9.0

  # --- Desktop User Interface and File Utilities ---
  window_manager: ^0.4.3       # Native window management on Windows/macOS/Linux
  file_picker: ^9.0.0          # Local audio file selection and export directory dialogs
  desktop_drop: ^0.5.0         # Drag-and-Drop support for importing tracks or dragging stems to a DAW
  lucide_icons_flutter: ^1.2.0 # Clean iconography for audio console controls

dev_dependencies:
  flutter_test:
    sdk: flutter
  flutter_lints: ^5.0.0
  build_runner: ^2.4.15
  freezed: ^2.5.8
  riverpod_generator: ^2.6.4
```

---

## 6. Detailed Design of the 5-Stem Pipeline in Rust

To resolve the bottlenecks identified in the legacy Python version (which ran 4 serial stages with CPU-bound `Kuielab` MDX models, exhibited weak synthesized drum separation, and averaged $27.11\text{ s}$ for a $15.48\text{ s}$ clip), the `SeparateFiveStemsUseCase` in the `application` crate executes the following topology over 32-bit floating-point buffers (`AudioBufferF32`):

1. **Stage 1 — Master Vocal Isolation (Inference 1 + Subtraction 1):**
   * `VocalBus = stem_inference_port.infer_vocal_bus(&original_mix)` using **Mel-Band RoFormer** in `FP16` precision on GPU (`DirectML` / `CUDA`).
   * `InstrumentalMaster = PhaseSubtractor::subtract(&original_mix, &VocalBus)`. Because it is computed via exact phase subtraction, it preserves 100% of the original instrumental transients and high-frequency air.
2. **Stage 2 — Vocal Branch (Inference 2 + Subtraction 2 + Bleed Gate):**
   * **VAD Gate (Optimization):** Before invoking the GPU on each $8$-second chunk of `VocalBus`, the CPU computes its RMS energy. If $\text{RMS} < -65\text{ dBFS}$, it writes zeros directly to `LeadVocal` and `BackingVocals`, bypassing ONNX Runtime completely.
   * For active vocal chunks: `LeadVocal = stem_inference_port.infer_lead_vocal(&VocalBus)` (using a **RoFormer Karaoke** model with $[L, R, \text{Mid}, \text{Side}]$ spatial feature channels).
   * `RawBackingVocals = PhaseSubtractor::subtract(&VocalBus, &LeadVocal)`.
   * `SpectralBleedGate::rebalance(&mut LeadVocal, &mut RawBackingVocals)` returns any isolated diffuse reverb tail from the lead singer back to `LeadVocal`, leaving the **Backing Vocals** stem clean while preserving the exact mathematical sum $S_{\text{lead}} + S_{\text{backing}} = V_{\text{total}}$.
3. **Stage 3 — Instrumental Branch (Inference 3 + Subtraction 3):**
   * `RhythmStems { drums, bass } = stem_inference_port.infer_rhythm_section(&InstrumentalMaster)` using a single hybrid model (**HTDemucs v4** or **SCNet** with its `vocals` and `other` output heads pruned from the ONNX graph). This directly resolves the weakness on both synthesized and acoustic percussion.
   * `InstrumentalStem = PhaseSubtractor::subtract_two(&InstrumentalMaster, &drums, &bass)`.
4. **Domain Invariant Verification (`SeparationResult`):**
   * Mirroring the legacy domain validation, `SeparationResult::try_new(...)` in the `domain` crate verifies that all 5 stems (`LeadVocal`, `BackingVocals`, `Drums`, `Bass`, `Instrumental`) are present, share the exact same frame count, sample rate, and channel layout, and satisfy the reconstruction null test where the maximum sample deviation against the original mix is $\le 10^{-5}$.

---

## 7. Step-by-Step Execution Plan (Migration Roadmap)

Follow this strict 5-phase sequence using `feature/*` branches merged into `develop` via Pull Requests:

### Phase 1: Domain Foundation, SOLID Ports, and FFI Bridge (Week 1)
* **Goal:** Compile the complete skeleton of all 4 Rust crates connected to the Flutter window before introducing heavy DSP logic.
* **Tasks:**
  1. Migrate entities from `src/atomusicator/domain/` (`AudioAsset`, `AudioSession`, `Stem`, `SeparationResult`, `SeparationJob`) into `rust_core/crates/domain`.
  2. Define all segregated port `traits` (ISP) in `rust_core/crates/application/src/ports/`.
  3. Configure `flutter_rust_bridge.yaml` pointing to `rust_core/crates/bootstrap_ffi/src/api.rs` and generate Dart bindings under `lib/src/rust_bridge/`.
  4. Implement architectural boundary tests in Cargo (equivalent to `tests/test_architecture.py`) verifying that `domain` and `application` compile without infrastructure dependencies.

### Phase 2: Persistence, Model Manifest, and SHA-256 Caching (Week 2)
* **Goal:** Enable the new application to read/write sessions compatible with `%LOCALAPPDATA%\Atomusicator` and manage ONNX models safely.
* **Tasks:**
  1. Copy `model_license_manifest.json` from the legacy repository into `assets/manifests/` and update model profiles for the 3-inference pipeline.
  2. Implement `LocalSessionWorkspace` in `infrastructure` with OS file locks (`fs4`), temporary staging directories, and atomic publication of `session.json` and the `outputs` folder.
  3. Implement `StageArtifactCache` using `sha2` and `lru` to validate input audio hashes, model hashes, and runtime signatures before reusing cached stage checkpoints.

### Phase 3: DSP Engine and 5-Stem GPU/CPU Separation (Weeks 3 & 4)
* **Goal:** Build `OrtSeparationEngine` and surpass the legacy Python benchmark metrics.
* **Tasks:**
  1. Implement `StftProcessor` (Hann window, 50% *Overlap-Add* in `f32`) and SIMD `PhaseSubtractor` in Rust.
  2. Implement `OrtSessionPool` configuring execution providers in priority order: `DirectML` (Windows GPU) $\rightarrow$ `CUDA` $\rightarrow$ `CoreML` $\rightarrow$ `CPU`.
  3. Wire the **3 Inferences + 3 Subtractions** pipeline with instant cancellation via `CancellationToken` and real-time progress reporting via `StreamSink` to Flutter.
  4. **Parity and Quality Benchmark:** Run a benchmark suite equivalent to `benchmarks/run_pipeline.py` and `verify_runtime.py`, verifying that the sum of all 5 stems cancels to zero (Null Test $<-100\text{ dB}$) against the original master audio.

### Phase 4: Lock-Free Playback, Streaming ($>256\text{ MiB}$), and Live Pitch Shifting (Week 5)
* **Goal:** Replace `sounddevice` and offline FFmpeg pitch renders with a real-time native audio engine.
* **Tasks:**
  1. Implement `CpalRealtimeMixer` with $60\text{ Hz}$ RMS peak telemetry streamed to Flutter and a master bus peak limiter.
  2. Implement `InMemoryTrackSource` and `BufferedStreamTrackSource` (using the lock-free `rtrb` ring buffer with $1024$-frame read-ahead blocks for sessions exceeding $256\text{ MiB}$, ensuring zero file I/O inside the audio callback).
  3. Integrate real-time pitch shifting ($-12$ to $+12$ semitones with formant preservation) directly into the playback DSP chain as well as the offline `ExportStemsUseCase` (using atomic temporary file writes renamed upon batch completion).

### Phase 5: Flutter UI (`AudioWorkbench`), Packaging, and Release (Week 6)
* **Goal:** Build the modern desktop/mobile interface, compile the native installer, and publish the GitHub Release.
* **Tasks:**
  1. Build `AudioWorkbench` in Flutter, including track rows with GPU-accelerated waveforms via `CustomPainter` (backed by the Rust waveform cache) and controls for gain, mute, solo, pan, and pitch.
  2. Adapt the Inno Setup script (`packaging/Atomusicator.iss`) and PowerShell automation (`packaging/build_windows.ps1` and `smoke_test_windows.ps1`) to bundle the `flutter build windows --release` executable alongside the `onnxruntime` and `DirectML` DLLs, keeping third-party license notices up to date.