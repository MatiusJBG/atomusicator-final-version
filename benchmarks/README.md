# Native audio evidence

Run the Rust example in release mode with a local JSON corpus and a fresh output
directory. No Python is used. All four model hashes must already verify in the
local model cache; the runner never downloads models.

```powershell
cargo build --locked --release --manifest-path rust_core/Cargo.toml -p atomusicator-infrastructure --example audio_bench
./benchmarks/Measure-NativeRun.ps1 -Executable rust_core/target/release/examples/audio_bench.exe -Arguments @('local_audio/corpus.json','local_audio/run-001') -Report local_audio/resources-001.json
```

Set `ATOMUSICATOR_FFMPEG` to the verified packaged executable if needed. The corpus
contains `cases`, each with `id`, `source`, `sha256`, `license`, `attribution`, and
optional `references` mapping `lead_vocal`, `backing_vocals`, `drums`, `bass` or
`instrumental` to aligned source stems. Paths are relative to the manifest or
absolute. Keep audio and manifests in ignored `local_audio/`.

The default runner uses `BoundedSeparation` and fresh spool storage with checkpoint
reuse disabled. The optional whole-buffer comparison runner is now named
`audio_bench_reference` and requires `--features reference-pipeline`; it is
excluded from standard production builds.

SI-SDR is calculated independently per channel after mean removal. SDR is
`10 log10(reference energy / squared sample error)`, with no BSS Eval filtering.
Amplitude
error, RMS and peak are also reported because scale invariance can hide gain
errors. Silence and mathematically perfect estimates have explicit JSON states.
Reference duration/rate/channel mismatches fail rather than silently cropping.

Inspect an existing five-stem directory without rerunning inference:

```powershell
cargo build --release --manifest-path rust_core/Cargo.toml -p atomusicator-infrastructure --example audio_inspect
./rust_core/target/release/examples/audio_inspect.exe local_audio/source.flac local_audio/run-001/case/outputs local_audio/diagnostics.json
```

An optional last argument selects a baseline stem directory. Its comparisons are
algorithm differences, not ground-truth quality scores. The report includes
per-channel RMS, peaks, float headroom and steps at eight-second boundaries,
plus alignment and reconstruction checks. A value above full scale in float WAV
does not prove hard clipping. Keep user-provided audio and its reports local.

The explicit CPU baseline is recorded separately with six intra-op threads,
one inter-op thread, sequential execution and spinning disabled. That is a
conservative policy; it has not been established as an optimal thread count.

The 2026-10-27 baseline used Ryzen 9 8945HS / 16 logical CPUs / approximately
15.3 GiB RAM, ONNX Runtime 1.20.0, with inference artifact cache disabled. It is
one cold process, not a statistical benchmark. RSS samples concern the Rust
process only, excluding FFmpeg children and GPU memory. Provider registration
was automatic in this version: **the effective provider is unknown**. Its
timing must not be compared to an explicitly CPU-only run as a CPU speedup.

Anvil Crypt Loop is CC BY 4.0 procedural instrumental music. Bass and drums have
aligned references; its synthesized lead is not a vocal reference. Drum SI-SDR
around -30 dB and near-zero output energy expose poor separation on this case.
This is not evidence of acceptable extraction quality, representative vocal
quality, or native/Python equivalence. The exact-sum reconstruction invariant
alone does not verify allocation to the correct stem. No audio is redistributed.

## Prepare and diagnose user-supplied reference stems

Place exactly one `.flac` or `.wav` per canonical name in
`local_audio/ground_truth/`: `lead_vocal`, `backing_vocals`, `drums`, `bass`,
`instrumental`. Preparation preserves those originals, pads shorter tails at
sample zero, and applies one common gain to references and their summed mix if
needed to keep the prepared files below full scale. Its JSON records hashes,
padding, the raw summed peak and common gain. It creates `ground_truth/mix.wav`
only if absent, and rejects an existing mix unrelated to the reference sum.

```powershell
cargo build --locked --release --manifest-path rust_core/Cargo.toml -p atomusicator-infrastructure --example prepare_ground_truth --example audio_bench --example audio_inspect --example rhythm_probe
./rust_core/target/release/examples/prepare_ground_truth.exe local_audio/ground_truth local_audio/prepared-001
```

Build a local corpus manifest using the prepared `mix.path`, `mix.sha256` and
each prepared `references.<stem>.path` from `preparation.json`. Use absolute
paths or paths relative to the corpus manifest. Include the user's attribution
and local evaluation permission; do not invent a redistribution license.

After running `audio_bench`, inspect its stage observations and isolate the
rhythm models from upstream vocal subtraction:

```powershell
./rust_core/target/release/examples/audio_inspect.exe --stage-report local_audio/run-001/report.json local_audio/stage-diagnosis.json
./rust_core/target/release/examples/rhythm_probe.exe local_audio/run-001/case local_audio/probes-001
```

The probes choose the 15-second grid excerpt with maximum reference drum energy
and run each rhythm model on the mixture, its actual pipeline input and the
isolated reference. They are diagnostic excerpts, not full-song qualification.
Tensor observations are spectral units; normalized iSTFT windows include overlap
and padding. Compare whole-track PCM RMS at the restored output, stage spool and
final WAV to locate amplitude loss. The gate observations concern vocals only;
drums and bass bypass `SpectralBleedGate`.

To check the bass-routing hypothesis across the entire track, reuse validated
unchanged baseline stages and infer only the alternative bass input:

```powershell
cargo build --locked --release --manifest-path rust_core/Cargo.toml -p atomusicator-infrastructure --example bass_candidate
./rust_core/target/release/examples/bass_candidate.exe local_audio/run-001/case local_audio/bass-candidate-001
```

This recomputes `instrumental` after replacing bass and scores all five stems.
Its results are a quality counterfactual, not an independent full-run performance
measurement. Do not promote a routing change merely because bass improves:
check the residual stem and the other outputs as well.

Keep generated reports in ignored `benchmarks/results/` or `local_audio/`.
Resource reports sample the root and observed descendants, including limitations
and the observed sampling gaps; VRAM remains explicitly unmeasured.

## DirectML qualification on Windows x64

Production remains CPU-only. Prepare a separate, pinned diagnostic runtime:

```powershell
pwsh -File benchmarks/Prepare-DirectMlQualification.ps1
$env:ORT_LIB_LOCATION = "$PWD/build/toolchain/directml-qualification/sdk"
$env:ORT_PREFER_DYNAMIC_LINK = '1'
$env:CARGO_TARGET_DIR = "$PWD/build/directml-qualification-target"
$env:TEMP = "$PWD/build/interactive-pitch-temp"
$env:TMP = $env:TEMP
New-Item -ItemType Directory -Force $env:TEMP | Out-Null
cargo build --locked --manifest-path rust_core/Cargo.toml -p atomusicator-infrastructure --example provider_qualify
$bin = "$env:CARGO_TARGET_DIR/debug/examples"
Copy-Item "$env:ORT_LIB_LOCATION/lib/onnxruntime.dll", "$env:ORT_LIB_LOCATION/lib/DirectML.dll" $bin
& "$bin/provider_qualify.exe" "$PWD/local_audio/directml-check-001"
```

Use a fresh report directory per run. The preparation script verifies NuGet
SHA-256 pins and Microsoft signatures, and retains ONNX Runtime and DirectML
licenses/notices alongside its local SDK. It does not modify application runtime
DLLs or add packages to a release. Remove these process environment overrides
before returning to normal application builds (or open a fresh terminal).

The example explicitly ends ORT profiling, counts actual `*_kernel_time` events
by provider, and compares the CPU/DirectML inverse-STFT window samples for all
four verified local models. A CPU fallback does not qualify as GPU execution.
The executable fails after saving its report if node execution or window parity
fails. Timing includes cold initialization and comes from a debug diagnostic
build; it is not a full-pipeline performance qualification.

The first local run executed 112 DirectML kernel events per model. Maximum
normalized-window PCM errors were respectively 1.769513e-7 (main), 1.741573e-7
(kara), 3.358582e-8 (drums), and 3.427267e-7 (bass), below 1e-4.
This proves node execution and single-window parity on the default ORT adapter.
It does not identify that adapter independently, measure VRAM, prove full-song
SI-SDR parity or certify batching. All four model declarations have a dynamic
batch dimension, so batching remains a feasible experiment rather than a
static-shape exclusion. Private raw traces and reports stay in ignored directories.

### Full-track provider parity

The corpus `profile` accepts `native-v1` (the historical default) or
`native-v2-original-mix-bass` (current production routing). Select the same
descriptor for both providers; never compare different algorithms as GPU parity.
`audio_bench` additionally accepts `[cpu|directml] [intra-threads]`, defaulting
to `cpu 2` for historical reproducibility. Each run requires a new output folder.

Use two isolated runtime folders containing the **same Release executable** and
their matching ONNX Runtime 1.20.0 DLLs. The CPU folder needs its CPU runtime and
shared-provider DLL; the DirectML folder needs the pinned DirectML runtime and
`DirectML.dll` from the preparation script. Retain each runtime's notices and
record the executable/DLL hashes. Do not replace application installation DLLs.

```powershell
./benchmarks/Measure-NativeRun.ps1 -Executable local_audio/bin-cpu/audio_bench.exe -Arguments @('local_audio/corpus-v2.json','local_audio/cpu-run','cpu','6') -Report local_audio/cpu-resources.json
./benchmarks/Measure-NativeRun.ps1 -Executable local_audio/bin-directml/audio_bench.exe -Arguments @('local_audio/corpus-v2.json','local_audio/directml-run','directml','6') -Report local_audio/directml-resources.json
cargo build --locked --release --manifest-path rust_core/Cargo.toml -p atomusicator-infrastructure --example audio_inspect
./rust_core/target/release/examples/audio_inspect.exe local_audio/mix.wav local_audio/directml-run/case-id/outputs local_audio/waveform-parity.json local_audio/cpu-run/case-id/outputs
pwsh -File benchmarks/Compare-ProviderReports.ps1 -CpuReport local_audio/cpu-run/report.json -AcceleratorReport local_audio/directml-run/report.json -WaveformComparison local_audio/waveform-parity.json -WindowQualification local_audio/window-probe/provider-qualification.json -Output benchmarks/results/PROVIDER_PARITY.json
```

Run providers sequentially without concurrent builds or other benchmark jobs.
The comparator validates input/reference/model identities, quality descriptors,
alignment, actual-node window evidence, provider registration without fallback,
and the exact WAV hashes recorded by the inspector. It requires maximum PCM
error <=1e-4, per-channel SI-SDR difference <=0.1 dB and reconstruction <=1e-5.
Undefined scores fail for separate review. A failed numerical comparison saves
its report and exits with an error. Reports do not certify independent corpus
diversity, VRAM admission, batching, device reconnection or a complete release.

The inspector is an allocating diagnostic utility; its memory use is not the
bounded production pipeline's RSS measurement. The fixture tests exercise these
evidence gates without models or audio and run in backend CI:

```powershell
pwsh -File benchmarks/tests/provider-parity.ps1
```

## Optional batch qualification

The window qualifier accepts an explicit maximum batch size. Use a fresh empty
report directory and the isolated pinned DirectML runtime folder described above:

```powershell
cargo build --locked --release --manifest-path rust_core/Cargo.toml -p atomusicator-infrastructure --example provider_qualify
# Copy that executable into the prepared DirectML runtime folder first.
./local_audio/bin-directml/provider_qualify.exe local_audio/batch-window-probe 1 --batch-scaling=2
```

The example measures batches 1 and 2 using two warmup rounds and five timed
rounds, each processing four synthetic windows. It refuses CPU fallback, checks
every output window against CPU PCM and records post-trial DXGI memory usage.
The measurements include tensor copies, session lookup and profiling; they are
ONNX runner throughput, not full-pipeline RTF or musical-corpus certification.
Per-batch checkpoints and per-trial progress survive a later native failure.
`--batch-scaling=1` measures only the baseline; omitting the option retains the
ordinary single-window node/parity qualification.

On the tested RTX 4060 Laptop runtime, batch 2 was slower and used more GPU
memory. Batch 4 caused a native ONNX Runtime access violation in an experimental
run. `--batch-scaling=4` explicitly exercises that unqualified path and may
terminate the probe process; preserve its process report and Windows event log.
Neither batch size is enabled in production. See the recorded evidence and
limitations in `BASELINE_BACKEND.md`.
