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

## Full-track native-v2 CPU / DirectML comparison

One user-supplied five-stem reference set, 11,521,125 stereo frames at 44.1 kHz
(261.25 seconds), identical prepared mix and references, current production
`native-v2-original-mix-bass` routing, same Release executable in isolated runtime
folders, ORT 1.20.0, six intra-op threads and fresh stage storage. Providers ran
sequentially. No other build or model benchmark ran concurrently.

| Observation | CPU | DirectML |
|---|---:|---:|
| Processing seconds | 695.4870 | 131.0282 |
| Processing RTF | 2.6622 | 0.5015 |
| Sampled process-tree peak RSS, bytes | 1,466,617,856 | 2,020,016,128 |
| Maximum observed sampler gap, ms | 1,548.63 | 780.90 |
| Maximum reconstruction sample error | 1.788139e-7 | 1.788139e-7 |

Processing includes decode, inference, diagnostic aggregation and WAV writing;
reference scoring is excluded. These are one run per provider, not repeated-run
medians or a general speed guarantee. RSS includes shared pages and excludes
unmeasured VRAM. Requested 100 ms polling did not achieve that maximum interval;
shorter peaks may be missed.

CPU scores, arithmetic means of the two channel dB values:

| Stem | SI-SDR, dB | SDR, dB |
|---|---:|---:|
| Lead vocal | 2.6025 | 3.4643 |
| Backing vocals | -2.2378 | 2.0199 |
| Drums | 3.6884 | 5.2342 |
| Bass | 7.5215 | 7.8522 |
| Instrumental | 2.5292 | 3.6017 |

This independently executes the complete production routing; it confirms the
previous bass counterfactual without reusing its unchanged stages. Across all
five final WAV pairs, maximum CPU/DirectML sample error is 8.493662e-7 and maximum
per-channel SI-SDR difference is 8.860079e-7 dB. All pass the 1e-4 PCM / 0.1 dB
parity gates. The comparator verifies exact input/reference/model/WAV hashes,
quality descriptors, node evidence and provider registration without fallback.
Backing-vocal isolation remains a quality concern with this reference set.

Detailed evidence remains local: `benchmarks/results/PROVIDER_PARITY_GROUND_TRUTH.json`,
`local_audio/ground-truth-v2-{cpu6,directml6}/report.json`, resource samples and
`local_audio/provider-full-runtime-manifest.json`. Adapter/VRAM admission and
runtime-error fallback have since been implemented and
tested; batching and extended device/soak qualification remain pending before
production enablement. This is one reference set and does
not replace independent corpus diversity or listening review.

### Discrete NVIDIA adapter cross-check

The same full reference set and native-v2 routing also passed on adapter 1,
NVIDIA GeForce RTX 4060 Laptop GPU, using the pinned DirectML runtime. Processing
took 53.388 seconds (RTF 0.2044), a single observation rather than a median.
The external sampler observed 417,841,152 bytes peak process-tree RSS; internal
DXGI sampling observed 1,906,438,144 bytes of current-process local-segment GPU
usage. These sampled observations can miss transient peaks and do not establish
global free VRAM or a duration-independent memory guarantee.

The final WAV comparator verified all five CPU/NVIDIA pairs, with maximum
absolute sample error 1.838431e-6 and maximum per-channel SI-SDR difference
4.656331e-7 dB. Four model traces also established actual DirectML kernel
execution. Hash-bound evidence is retained locally in
`benchmarks/results/PROVIDER_PARITY_NVIDIA_GROUND_TRUTH.json` and
`local_audio/closure-nvidia-{full,window}/`. This extends numerical adapter
coverage; it does not certify long-duration stability or listening quality.

### Duration and memory soak on the discrete adapter

Ten fresh-process runs completed with native-v2 and strict DirectML adapter 1
(two passes each). The input is the authorized reference mix repeated/trimmed
without a gain change. Each report verifies input identity, five final WAV
hashes, stereo reconstruction within 1e-5 and absence of CPU fallback.

| Input duration | Processing RTF, pass 0 / 1 | Sampled process-tree peak RSS, MiB, pass 0 / 1 |
|---|---:|---:|
| 15 seconds | 0.5945 / 0.4045 | 391.1 / 392.5 |
| 3 minutes | 0.2127 / 0.2027 | 399.6 / 397.7 |
| 10 minutes | 0.2061 / 0.2315 | 386.0 / 388.7 |
| 30 minutes | 0.2912 / 0.2052 | 373.4 / 378.9 |
| 60 minutes | 0.3019 / 0.2991 | 377.2 / 383.7 |

The largest reconstruction error was 2.384186e-7. The last 60-minute run's
internal sampler observed 1,906,438,144 bytes peak current-process DXGI local
segment usage, with maximum sampling gap 0.4383 seconds. These are sampled
measurements, not allocation-exact guarantees. A Windows VM remained running;
background load and OS caches were uncontrolled. Pass 1 is not a resident-model
warm run and pass 0 does not establish a cold OS cache. The RTF values therefore
are individual observations rather than isolated performance medians.

Evidence is retained under ignored `local_audio/duration-validation-v3-serial/`:
per-case reports, resource samples, JSONL stage diagnostics, input/executable
provenance and the ten-case completion marker. Generated case working audio was
removed only after successful report validation; original inputs remain local.
This validates the tested duration range with bounded observed RSS, not musical
corpus diversity, repeated in-process lifecycle behavior or physical devices.
