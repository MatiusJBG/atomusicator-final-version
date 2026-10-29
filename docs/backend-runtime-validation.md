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

Windows full workspace: 143 tests passed, 11 opt-in tests omitted. Linux x64 in
an isolated Arch/QEMU overlay: 142 passed, 11 omitted, with formatting and
all-target Clippy clean. The count differs because the Windows ACL test is
platform-specific. Both used Rust 1.99.1 and ONNX Runtime 1.20.0; Linux used the
[official CPU runtime archive](https://github.com/microsoft/onnxruntime/releases/tag/v1.20.0).

Additional opt-in checks compare borrowed and owned ORT tensors for all four
verified models, and bounded/reference real-model outputs at <=1e-6. The kernel
measurement and its limited scope are recorded in `benchmarks/BASELINE_BACKEND.md`.

The Linux VM is not clean-Windows installer qualification, physical-device
reconnection evidence, GPU parity qualification or the long model soak. Those
require their own results. Private audio and detailed host reports remain in
ignored local directories.
