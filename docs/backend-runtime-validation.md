# Backend runtime and validation

The desktop composition uses `native-v2-original-mix-bass`: drums consume the
accompaniment residual; bass consumes the original mix; other is recomputed by
subtraction. The original `native-v1` remains available for reproducible
comparisons and recovery of jobs created with that profile. A retry pins its
parent's quality descriptor in its first durable journal revision.

## Bounded memory and lifetime

MDX reuses PCM, spectrum, prediction, decoded-window and FFT scratch buffers.
The ORT adapter borrows an exclusive CPU slice only during synchronous `Run`;
the tensor view cannot escape the adapter. It validates dimensions and finite
samples before constructing the view, and checks finite outputs while copying
into the reusable destination. Small ORT bookkeeping allocations remain; this
is not a claim of zero allocator calls inside ONNX Runtime.

An offline-job scope releases idle ONNX sessions after completion, cancellation
or unwinding, while preserving live leases. Waveform preparation uses blocks of
65,536 frames and a multilevel index bounded to approximately 1 MiB per track.
The separate codec streamer uses a default 48,000-sample ring; decoder packet
storage is additional. Session verification hashes files in blocks rather than
decoding full songs.

The stage spool has 64-bit offsets. Its default **retained-cache quota** is
4 GiB, not a maximum active-session size. Set
`ATOMUSICATOR_CACHE_RETAINED_BYTES` to an unsigned byte count before launching
the application to change retention; zero retains no idle audio. Active leased
artifacts are protected. Invalid configuration fails explicitly.

## Large WAV files

Publication and native pitch export choose RIFF for ordinary files and RF64 when
the 32-bit container size would overflow. Both write the definitive header first
and preserve finite float32 PCM. FFmpeg WAV export requests `-rf64 auto`.
Native readers support contiguous RF64/BW64 float32 and PCM16/24/32, including
WAVEFORMATEXTENSIBLE subformats, with bounded buffers and 64-bit seeking.
Optional extended chunk-size tables are explicitly rejected; the application
does not produce them. The allocating legacy WAV convenience reader remains
a compatibility utility; production session playback uses block readers.

The implementation follows the size-override rules of
[EBU Tech 3306](https://tech.ebu.ch/docs/tech/tech3306v1_1.pdf).
Tests include a sparse 32 GiB file, sample positions beyond `u32::MAX` frames,
small RF64 readback, malformed sizes and non-sentinel size precedence.

## Verification scope

### CPU profiles

Set `ATOMUSICATOR_CPU_PROFILE` before starting the application: `eco` (up to two
inference threads), `balanced` (default, up to six), or `throughput` (up to twelve).
Unknown values fail explicitly. Every profile reserves two logical CPUs when
possible, and remains valid on single-core hosts. The generated Flutter bridge
exposes `computeSettings()` and `setComputeProfile(profile: ...)`. The latter
rejects changes during any active audio job and persists the selection atomically
in `Atomusicator/compute-profile.json` under the OS local application-data folder.
Missing preferences default to balanced; invalid preferences fail explicitly.
A startup environment setting overrides the saved preference for that launch.
The visual settings control remains a frontend integration task.

While the mixer is playing, the next inference window halves the requested
thread budget (rounded up), uses at most six threads
and reserves four logical CPUs when possible. A running inference is allowed to
finish; this is not instantaneous preemption. Sessions are cached by canonical
model path **and actual thread count**, so a thread-budget change cannot reuse
an incompatible ORT session. All session variants share the same capacity limit.
Admission reserves the largest supported profile's upper bound, and published metadata records the
actual thread counts observed in initialized sessions. This policy reduces CPU
contention; physical playback/underrun qualification remains necessary.

Example from PowerShell, before `flutter run -d windows`:

```powershell
$env:ATOMUSICATOR_CPU_PROFILE = 'throughput'
```

The host-specific matrix and its measurement limits are in
`benchmarks/BASELINE_BACKEND.md`.

### Accelerator configuration and admission

The startup preference `%LOCALAPPDATA%/Atomusicator/execution.json` accepts
`{"mode":"cpu","device_id":0}`, with modes `cpu`, `directml` or `auto`.
CPU remains the default. `ATOMUSICATOR_EXECUTION_PROVIDER` and
`ATOMUSICATOR_GPU_DEVICE_ID` override that file for the launch; restart the app
after changing execution mode. Unknown values and out-of-range indices fail
explicitly. Auto selects a physical adapter, preferring dedicated video memory.
It still falls back to CPU if admission or provider registration fails. GPU modes
remain experimental pending duration/device qualification. Accelerated jobs use
one resident-model slot, matching the measured memory configuration.

DirectML requires an observable DXGI budget with 2.5 GiB headroom and at least
2 GiB available system RAM. Adapters with less than 2.5 GiB dedicated memory
reserve an additional 2.5 GiB of system RAM (4.5 GiB total), because shared GPU
memory consumes that same RAM. These are conservative estimates, not hard OS
reservations. Other processes can change availability after the observation.

DXGI records the adapter index/name, dedicated/shared capacity, budget and the
calling process's usage. After-window samples and the optional 100 ms sampler
report observed GPU-process peaks; neither measures allocation-exact peaks nor
global GPU usage. The default adapter on the tested host is the Radeon 780M
(index 0); RTX 4060 Laptop is index 1. Adapter ordinals are machine-specific.

If accelerated `Run` fails, its session lease is released and the model is
quarantined to CPU for that job. The cache key distinguishes forced-CPU sessions,
so retry cannot reuse the failed accelerated session. Idle sessions are released
and CPU memory admission is checked before one CPU retry. CPU and permanent
argument/model errors are not retried. Both failure messages remain available
if retry fails. No partially produced block is published.

### Per-job diagnostics

Each offline job writes `jobs/<job-id>.diagnostics.jsonl` beside its journal.
Tracing is scoped to the worker thread; no global subscriber or audio callback
logging is installed. The file contains stage wall times, session registration
and fallback reasons, outcome, and sampled current-process RSS. RSS excludes
FFmpeg child processes and VRAM; external process-tree measurements remain
separate. GPU samples have their own explicit scope. Panics still close a valid
JSON summary, and telemetry does not swallow the original operation failure.

`ATOMUSICATOR_DIAGNOSTICS=false` disables files; `ATOMUSICATOR_LOG_LEVEL` selects
TRACE/DEBUG/INFO/WARN/ERROR (default INFO). Use INFO to retain stage and memory
summaries. Invalid settings fail explicitly. CPU jobs do not probe or wake GPU
adapters merely for telemetry. Phase names distinguish nested `mdx_total` from
read/normalization, FFT, inference, overlap-add, spool writes and finalization;
do not add parent totals to their child durations.

The benchmark uses a separate `<output>.diagnostics/` directory and optionally
accepts a final adapter index after its CPU-thread argument.

### Platform checks

Windows full workspace after the CPU settings bridge: 150 tests passed, 12
opt-in tests omitted, with formatting and all-target Clippy clean. A separate
real-model ONNX run passed all 13 tests, including thread-variant replacement
and four-model tensor parity. Flutter's compiled-bridge smoke test also passed,
including settings retrieval, invalid-profile rejection and existing pitch/export.

The earlier RF64 revision passed 142 tests with 11 omitted on Linux x64 in an
isolated Arch/QEMU overlay, with formatting and all-target Clippy clean. The
subsequent CPU/bridge changes have not yet been rerun there. Both platforms used
Rust 1.99.1 and ONNX Runtime 1.20.0; Linux used the
[official CPU runtime archive](https://github.com/microsoft/onnxruntime/releases/tag/v1.20.0).

Additional opt-in checks compare borrowed and owned ORT tensors for all four
verified models, and bounded/reference real-model outputs at <=1e-6. The kernel
measurement and its limited scope are recorded in `benchmarks/BASELINE_BACKEND.md`.

The Linux VM is not clean-Windows installer qualification, physical-device
reconnection evidence, GPU parity qualification or the long model soak. Those
require their own results. Private audio and detailed host reports remain in
ignored local directories.
