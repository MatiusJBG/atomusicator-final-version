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
