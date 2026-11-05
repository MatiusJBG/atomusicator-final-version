# Frontend/backend improvement status

Branch: `feature/backend-validation-and-optimization`.

## Implemented

- Source preview accumulation runs in a Dart worker isolate with bounded chunks
  and `BytesBuilder`. FFmpeg decoding is cancelled when the provider is disposed.
  Managed stem waveforms continue to use the native waveform index/cache.
- Static mixer waveforms and the moving cursor have independent repaint
  boundaries. Mixer layout and track controls use granular state selectors.
- Native job changes wake a condition variable. A bridge wait call delivers
  transitions to a Dart stream; its two-second watchdog bounds abandoned calls.
  Unchanged watchdog responses do not rebuild the UI. There is no global job
  status timer. The 180 ms transport timer exists only while playback is active,
  and stops on pause, stop, end-of-playback or controller disposal.
- Compute profiles Ahorro/Equilibrado/Máxima velocidad use native persistence,
  backend validation and requested/effective thread counts. Changes are blocked
  during jobs; playback-aware thread admission remains owned by the engine.
- Execution provider preferences CPU/DirectML/Auto use the native preferences.
  They apply after restart, not by mutating a live ONNX pool. Environment
  overrides retain priority. Diagnostics distinguish registered providers,
  effective assignments and fallback reasons; unconfirmed assignments are
  labelled accordingly. Selection is not GPU qualification.
- Retained spool-cache limits persist natively and are used by the existing
  bounded pipeline. The environment override remains authoritative. UI supports
  limits beyond 4 GiB, actual cache usage and native lease-aware pruning while
  idle. This is a retention quota, not a cap on active job working storage.
  Cleanup does not delete projects or arbitrary orphan directories.
- Cancellation immediately disables repeat requests and remains busy until the
  engine confirms the terminal transaction state. It does not publish partial
  stems. Resource admission failures preserve native requested/available detail.
- Explicit project closing stops and unloads the mixer, invalidates pending pitch
  work and releases idle ONNX sessions. Project files remain on disk.
- Large WAV exports display the RF64 compatibility notice based on selected
  rate/channels/bit depth and duration. The native exporter already uses RF64 auto.
- Windows frontend CI checks formatting, analysis and widget/controller tests.
  Native integration additionally needs the built DLL and pinned FFmpeg tools.
- Bindings are generated with bridge 2.8.0, matching the pinned Dart/Rust runtime;
  no bridge dependency upgrade is included.

## Verification scope

Tests cover independent waveform/cursor invalidation, profiles and errors,
provider persistence calls and CPU fallback display, cache cleanup and busy
protection, native job event delivery, listener disposal and transactional
cancellation. The native bridge smoke test checks waveform, pitch/export,
event-based job completion, runtime settings reads and project unloading.

`flutter build windows` builds Flutter only. After changing FFI, build the Rust
bridge too and package its DLL beside the executable using
`packaging/build_windows.ps1 -SkipInstaller` (optionally `-PrepareDebug`). Never
bypass the bridge content-hash check to run mismatched artifacts.

## Remaining qualification

A 60-minute DevTools trace, Repaint Rainbow capture, GPU qualification on the
distributed runtime and physical audio-device measurements remain manual release
acceptance work. Functional tests do not certify 16 ms frames, zero repaints per
second or 95 ms latency. Source preview uses the allowed isolate fallback; the
session-only waveform API is not applied to arbitrary imported files.
