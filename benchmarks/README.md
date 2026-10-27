# Native audio evidence

Run the Rust example in release mode with a local JSON corpus and a fresh output
directory. No Python is used. All four model hashes must already verify in the
local model cache; the runner never downloads models.

```powershell
cargo build --release --manifest-path rust_core/Cargo.toml -p atomusicator-infrastructure --example audio_bench
./benchmarks/Measure-NativeRun.ps1 -Executable rust_core/target/release/examples/audio_bench.exe -Arguments @('local_audio/corpus.json','local_audio/run-001') -Report local_audio/resources-001.json
```

Set `ATOMUSICATOR_FFMPEG` to the verified packaged executable if needed. The corpus
contains `cases`, each with `id`, `source`, `sha256`, `license`, `attribution`, and
optional `references` mapping `lead_vocal`, `backing_vocals`, `drums`, `bass` or
`instrumental` to aligned source stems. Paths are relative to the manifest or
absolute. Keep audio and manifests in ignored `local_audio/`.

SI-SDR is calculated independently per channel after mean removal. Amplitude
error, RMS and peak are also reported because scale invariance can hide gain
errors. Silence and mathematically perfect estimates have explicit JSON states.
Reference duration/rate/channel mismatches fail rather than silently cropping.

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
