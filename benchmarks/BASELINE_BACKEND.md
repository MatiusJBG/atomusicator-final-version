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
