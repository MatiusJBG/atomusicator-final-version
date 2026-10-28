# Embedded pitch processing

Atomusicator's interactive pitch engine uses the following separately licensed
third-party code. The proprietary first-party notice does not replace these terms.

| Component | Pinned source | License |
| --- | --- | --- |
| Signalsmith Stretch 1.3.2 | `88061fd4eda56a2353c5d65c74aa3f4ccf76f9da` | MIT; copyright 2022 Geraint Luff / Signalsmith Audio Ltd. |
| Signalsmith Linear | `7bb1c8f36794d6980080ce817c91f8da8fe56a66` | MIT; copyright 2025 Signalsmith Audio |

Original notices and full license texts are preserved under
`rust_core/crates/infrastructure/native/vendor/`. `PROVENANCE.json` records
the upstream repositories and SHA-256 of each unmodified vendored file.
Both license texts and provenance are copied into `licenses/signalsmith/`
in the runnable Windows application and installer.

The C ABI adapter is first-party code; vendored upstream headers are unchanged.
The build generates a derived header exposing Linear's existing FFT minimum
size separately from the audio-window length. The two-line transformation is
recorded in `rust_core/crates/infrastructure/build.rs` and guarded against
upstream API changes. This permits a shorter audio window with finer spectral
mapping; the upstream MIT notice remains applicable to the generated header.
The build uses the bundled FFT implementation, without FFTW, IPP or another
optional external FFT backend. No additional pitch-engine DLL, Python runtime,
LLVM installation or commercial Rubber Band license is required by this engine.

This document covers the new embedded pitch components. The wider distribution
inventory and remaining release-compliance work remain documented in
[distribution-compliance.md](docs/distribution-compliance.md).
