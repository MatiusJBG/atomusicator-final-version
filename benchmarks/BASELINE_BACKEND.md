# Backend baseline and qualification

## Evidence status

This document records verification status, not an assertion that the backend is
qualified for release. Private input audio, reference stems, measurements and
implementation plans remain outside version control.

The initial audit at `84bb04f` found (historical findings, not current status):

- The Cargo workspace contains four crates with local path dependencies.
- The model manifest is embedded by `include_str!` in
  `infrastructure/src/models/manifest_manager.rs`; loading that manifest does
  not depend on the runtime working directory.
- The previous product directory literal remains intentionally confined to
  `storage/legacy_import.rs` for compatible data discovery.
- `audio_bench` reported `CPUExecutionProvider` as a literal. This is
  not evidence of per-node provider assignment or observed GPU fallback.
- Session creation held the pool mutex. Moving it outside the
  mutex requires capacity reservations and concurrent admission tests.
- No release optimization profile existed in the workspace manifest.

## Required baseline before DSP or inference optimization

Record the source commit, toolchain, runtime and model versions/hashes, input
hash, power profile, CPU/GPU/driver, thread budget and available memory for
every measurement. Preserve raw JSON and process logs locally.

Measure at least five repetitions for short and medium cases. Report cold
session construction separately from repeated inference within the same
process and pool: restarting a process is not a warm session measurement.
Report median, p95, dispersion and whole-process-tree memory with the sampling
interval and measurement limitations.

Required durations: 15 seconds, 3 minutes, 10 minutes, 30 minutes and 60 minutes.
Long synthetic cases can validate bounds and cancellation, but cannot establish
musical isolation quality.

## Musical qualification

Qualification requires at least five authorized songs with independent reference
stems for lead vocals, backing vocals, drums, bass and other. A commercial mix
alone does not supply these references. Do not treat reconstructed mixture error
as isolation quality or label generated model outputs as original reference stems.

Report alignment, SI-SDR, RMS error and peak level for each stem, including
intermediate pipeline outputs when diagnosing attenuation. The reported Anvil
drum/bass issue remains a hypothesis until reproduced against its original
references and configuration.

## Acceptance

Preserve finite-value validation, hash checks, resource admission, cancellation
and transactional publication. Reject unrelated time or memory regressions above
5%. Accept substantial optimization complexity only with a measured median gain
of at least 10–15% and unchanged musical quality within declared tolerances.

The Windows CI workflow checks formatting, all-target Clippy and workspace tests.
Opt-in hardware/model tests, acoustic tests, GPU parity and clean-machine installer
qualification require separate evidence; a green CI run does not certify them.

## Reusable MDX workspace measurement

Five independent Release processes per implementation, virtual five-minute
source through the actual main-profile FFT/OLA kernel, counting sink and zero
neural prediction (no ONNX inference or musical-quality claim):

| Measurement | Before | Reused buffers |
|---|---:|---:|
| Median kernel time | 2.795860 s | 2.355453 s |
| p95 (nearest rank, n=5) | 2.951130 s | 2.492066 s |
| Maximum sampled process-tree RSS | 48,832,512 B | 49,754,112 B |

Median kernel time improved 15.75%; sampled RSS increased 1.89%. Both use the
same Release profile. Original raw logs remain in ignored `local_audio/`.
Separate tests compare reusable FFT operations against the original allocating
implementation, and borrowed ORT input against owned tensors for all four pinned
models. The real-model bounded/frozen pipeline comparison passes at <=1e-6.
These kernel results do not establish the full separation speedup.

## CPU thread matrix (Ryzen 9 8945HS, 16 logical CPUs)

Release build, ORT 1.20.0 CPU, one resident model at a time, inter-op=1,
sequential execution, spinning disabled. Each cell is the median of five
synthetic spectral-tensor runs in seconds; initialization is measured separately.
The temporary Linux VM was shut down before these measurements.

| Intra-op threads | Main | Kara | Drums | Bass |
|---:|---:|---:|---:|---:|
| 1 | 8.6451 | 6.3066 | 6.2717 | 6.2282 |
| 2 | 6.0504 | 4.0942 | 4.0129 | 4.2253 |
| 4 | 3.8227 | 2.5864 | 2.8477 | 2.7821 |
| 6 | 3.0769 | 2.1103 | 2.4191 | 2.4298 |
| 8 | 2.7207 | 1.8570 | 2.1839 | 2.3542 |
| 12 | 2.4225 | 1.6585 | 1.9826 | 2.0821 |

Twelve versus six threads reduces these medians by 14.3–21.4%. This supports
an opt-in throughput profile on this host, not a universal optimal thread count
or an end-to-end speedup claim. Warm outputs were bit identical within each
configuration; cross-configuration hashes are not a musical parity test.
Raw outputs and process-tree resource samples: ignored `local_audio/cpu-matrix-*`.
