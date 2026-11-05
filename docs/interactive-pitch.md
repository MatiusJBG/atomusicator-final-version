# Interactive pitch validation

The 0.7.0 candidate adjusts all five stems during playback without rendering a
song or starting a pitch job. The Flutter control immediately displays the latest
integer semitone setting (−12 through +12); one in-flight bridge call coalesces
rapid changes. Reset selects zero. There is no formant control.

## Processing and session behavior

- Five independent stereo Signalsmith processors preserve tempo and a common
  timeline. WAV access, resampling and DSP coordination run outside CPAL's
  callback. That callback consumes a bounded queue and performs mixing.
- The profile follows Signalsmith `presetDefault`: a block of about `0.12 *
  sample_rate` and a hop of about `0.03 * sample_rate`, and it never uses a
  block shorter than 3,072 samples. The minimum FFT stays 8,192 points (larger
  only if the block exceeds that). Transpose uses an 8 kHz tonality limit, as a
  fraction of the sample rate and strictly below Nyquist, so highs are not fully
  shifted. A private pool uses at most two DSP workers. The processed queue
  targets one observed device callback plus 256 frames, within a 16,384-frame
  capacity. Unsupported larger buffers fail preparation instead of growing RAM.
- Formant compensation is not a feature. Every stem is transposed with
  `setFormantFactor(1, false)`, so the shifter does not estimate a fundamental
  or try to keep vocal timbre. The same setting is used for monitoring and
  export. Older session files may still contain `preserve_formants`; that field
  is ignored and is not written again.
- Playback preparation probes the actual output buffer while muted. Device
  drivers can ignore a requested buffer size. Play/seek may prepare buffers;
  adjusting pitch does not reopen the stream or show a loading job.
- Zero pitch crossfades to the sample-exact delayed original. Seek recreates DSP
  history with real preceding samples. All stems change semitones together.
- Source stems and waveform paths remain unchanged. Session, epoch and revision
  reject stale commands. A bounded autosave worker persists the latest setting
  under native local application data; incompatible/corrupt settings are ignored.
- Export freezes the displayed pitch at click time and uses the same DSP engine
  with exact frame-count compensation. Later control changes cannot alter that
  export. `export.json` records settings and engine identity. Export still runs as
  a cancellable background job and publishes a complete batch atomically.
- The old offline pitch API remains available for compatibility; Flutter's live
  controls do not invoke it. Existing rendered batches keep their lifetime leases.

## Measured evidence

Measurements below use a Windows development host and release DSP build. They
are specific to this configuration, not a universal latency guarantee.

| Check | Result |
| --- | --- |
| Sustained 440 Hz tones, 44.1/48 kHz, ±1/±7/±12 | All combinations within 5 cents; duration preserved |
| Five stereo stems, 256-frame blocks, formants on | Processing p95 3.45 ms; mean real-time factor 0.267; maximum 5.92 ms |
| Native muted device, 48 kHz, 1,056-frame callbacks, queue target 1,312 | 16 changes: p50 81.5 ms, p95/max 95.1 ms; zero underrun frames, capture overflows or driver errors |
| Concurrent pitch-render export load, same device | 12 exports during 16 changes; p50 81.0 ms, p95/max 91.0 ms; zero underruns/overflows/driver errors. Encoding and inference are outside this measurement |
| Native latency scope | Own synthetic samples before output gain, using driver-estimated presentation timestamps; no microphone/system-loopback capture |

The last measurement includes the output queue, device scheduling estimate and
a 512-sample detection window. It excludes Flutter event delivery and physical
speaker/headphone latency. An acknowledged revision means the callback consumed
its frames; it does not prove acoustic presentation.

The table above was measured with the previous 3,072/512 profile and formant
compensation on every stem. That profile is retired: it shifted the key but
sounded distorted on a full mix. The current profile is the longer rate-scaled
window and the 8 kHz tonality limit, with formant compensation disabled.

Release `five_stem_dsp_timing` on this host with the current profile at 48 kHz:
p95 1.39 ms, max 2.16 ms, mean real-time factor 0.048, algorithm delay
5,760 frames (120 ms), output delay 2,880 frames (60 ms). That fits under one
256-frame period (5.33 ms). The device-latency rows above were not remeasured.
A 440 Hz tone within 5 cents still does not prove mix quality; listening on a
real song is the gate. The longer analysis window adds latency and remains a
live control, not an offline render.

## Reproduce

After the toolchain setup in the README, run from the repository root:

```powershell
cargo test --locked --manifest-path rust_core/Cargo.toml --workspace
cargo test --locked --manifest-path rust_core/Cargo.toml -p atomusicator-infrastructure --release --test live_pitch five_stem_dsp_timing -- --ignored --nocapture
cargo run --locked --manifest-path rust_core/Cargo.toml -p atomusicator-infrastructure --release --example live_pitch_device_timing
cargo run --locked --manifest-path rust_core/Cargo.toml -p atomusicator-infrastructure --release --example live_pitch_device_timing -- --export-load
# Existing local five-stem session; output stays muted:
cargo run --locked --manifest-path rust_core/Cargo.toml -p atomusicator-bootstrap-ffi --release --example desktop_playback_check -- C:/path/to/local/session
```

Run timing measurements serially with builds and competing benchmarks stopped.
The device probe requires an F32 default output device. Other supported output
formats are covered by the application, not that particular probe.

## Remaining release qualification

The runnable 0.7.0 package passed 38-file integrity verification and startup with
a reduced PATH, and its installer compiled. The isolated installer test could
not complete: this execution context denied writes to the Start Menu and HKCU
uninstall registry key. Setup rolled its isolated files back. This is not a
clean-Windows installation pass; repeat the documented installer qualification
from a normal user session on the target machine.

The implementation is available for local evaluation. It is not yet fully
qualified for release: independent vocal references and gain-matched listening,
Flutter-to-acoustic timing, concurrent inference/export timing, a full 60-minute
physical playback soak, additional device configurations, clean Windows and
physical device reconnect still need evidence. Synthetic frequency accuracy does
not establish mix quality or absence of transient/stereo artifacts.
The new MIT licenses and provenance ship with the app; broader dependency
compliance remains tracked in [distribution-compliance.md](distribution-compliance.md).

Plans, test audio and detailed machine logs remain local and ignored by Git.
