# Resource usage and performance study — Atomusicator 0.6.0 (Windows x64 CPU candidate)

> Actualización de identidad (2026-10-29): las cifras de almacenamiento pertenecen a la medición histórica. Los proyectos y modelos activos usan Atomusicator; los datos anteriores solo se conservan como origen de importación. Véase atomusicator-storage.md.

Date: 2026-10-28. Branch: `feature/complete-python-migration`.
Scope: RAM, storage, CPU and GPU consumption; engine/thread verification; a
prioritised, **quality-preserving** optimisation portfolio.
Nature of this document: analysis and measurement. No product code, dependency,
model or packaging change was made while producing it, and no repository commit
was created from it.

Evidence comes from three places, kept separate throughout:

1. **Prior campaign** (2026-10-27) already stored in this repository:
   `local_audio/closure-evidence/*.json`, `benchmarks/results/*.json`.
2. **Re-run for this study** (2026-10-28): fresh runs of the same harness,
   raw artifacts under `report/measurements/`, summarised in
   §4 and §7.
3. **Static audit** of `rust_core/**`, `lib/**` and `packaging/**` with
   `file:line` evidence, in
   [`report/parts/rust-engine.md`](../report/parts/rust-engine.md) and
   [`report/parts/dart-storage.md`](../report/parts/dart-storage.md).

Numbers are quoted with their unit and provenance. Where a number is derived
rather than measured it is labelled *derived*; where it is an extrapolation it
is labelled *estimate*. Single runs are single runs: this study does not claim
statistical confidence intervals.

---

## 1. Executive summary

**What the application is.** A Windows audio workbench: Flutter UI over a
Rust engine (`atomusicator_bootstrap_ffi.dll` + `onnxruntime.dll` + FFmpeg
audio tools) that separates a stereo mix into five stems with four pinned ONNX
models, plays the five stems in sync, applies pitch with Rubber Band through
FFmpeg, and exports WAV/FLAC/MP3. There is no Python anywhere in the shipped
path.

**Headline numbers (measured unless marked).**

| Dimension | Result | Source |
|---|---|---|
| End-to-end separation, 3 min 45.4 s mix, 6 intra-op threads | **418.1 s wall** (1.85 s per audio second, i.e. 0.54× real time), **1597.4 CPU-s**, peak RSS **1.460 GB** | `applecats-bounded-{result,resources}.json`; re-run in §7 |
| Short clip (24 s) at 6 threads | 62.7 s per pass, peak RSS 1.496 GB (2026-10-27); 82.7 s per pass, 1.427 GB in the 2026-10-28 re-run | `threads-6-*.json`; `report/measurements/threads/` |
| Thread scaling (24 s clip) | 2026-10-27: 1→2→4→6 threads = 183.3 → 112.9 → 70.7 → 62.7 s (1.62×, 2.59×, 2.93×), efficiency 81 / 65 / **49 %**. 2026-10-28 re-run: 210.9 → 140.0 → 101.0 → 82.7 s (1.51×, 2.09×, 2.55×), efficiency 75 / 52 / **42 %** (see §4.2 drift note) | `threads-{1,2,4,6}-result.json`, `report/measurements/threads/` |
| CPU efficiency | **3.82 of 6 threads busy** on the full track (2.90 at 4 threads); CPU-seconds rise **+28 %** (2026-10-27) / **+56 %** (re-run) from 1 to 6 threads | derived |
| Fixed cost per pass | **≈16.5–20.4 s per pipeline pass**, measured directly: a **2 s** input takes **22.0 s** at 6 threads (≈4 ONNX session constructions per pass) | `threads-6-2s.*`; §4.4 |
| Memory vs duration | Peak RSS flat across 9.4× duration (1.460 GB at 225 s vs 1.496 GB at 24 s); FFT/OLA kernel flat at **40.9–42.0 MB** across 5/30/60 min | `kernel-{5,30,60}-min-resources.json` |
| Storage per separation | 9 full-length intermediate spools + decoded spool + 5 published float32 WAVs; **≈11.4 GB (10.65 GiB) of spools and 18.2 GB of required free space for a 60-minute source** | *derived* from code + manifest |
| Installed footprint | Release folder **489.3 MiB** (FFmpeg/FFprobe alone 434.4 MiB), installer **130.6 MiB**, app-owned data **2.44 GiB** after the local campaign | measured (§5) |
| GPU | **No GPU execution path.** Bundled ONNX Runtime is CPU-only; provider order is hardcoded to CPU. On this host the RTX 4060 Laptop sits at **0 MiB VRAM / 0 % utilisation** during all work | measured + code |
| Engine verification | Default Rust suite: **115 passed, 0 failed, 8 environment-gated (ignored)**; prior campaign: **bit-identical stems across 1/2/4/6 threads**, max reconstruction error **2.98e-7** against the `1e-5` contract; physical playback check passed play/pause/seek/EOF | re-run + `threads-*-comparison.json`, `final-physical-playback-check.json` |
| Cache payoff | Re-opening a completed session from verified stage artifacts: **5.77 s instead of 418 s** (not a 72× inference speed-up; it skips inference entirely) | `applecats-final-cache-reopen.json` |

**The five things that would actually make it faster without touching audio.**

1. **Fill the idle inference threads.** Six ORT threads deliver 2.55–2.93×
   while averaging only 3.7 busy cores. Running two independent windows (or two
   stages of independent work) with 3 threads each exploits the same budget far
   better (*estimate*: +25–45 % throughput). Output is bit-identical because the
   overlap-add order is preserved. Verify before believing (§9, E1).
2. **Stop paying for per-window and per-model repetition.** The KARA stage
   re-pads a 5.92 s window grid on every 8 s VAD chunk: **30.3 M network frames
   for 15.4 M frames of audio**, i.e. +19 % of all inference frames before
   weighting for the larger tensors. Removing it changes output, so it belongs
   to a *new profile* (`context-candidate-v1`), not to `native-v1`.
3. **Close the read amplification on cache hits.** A cache-hit job still moves
   ≈2.37 GB to open a 3.75-minute session (spool re-verification 683 MB, publish
   read/write/hash 1 137 MB, waveform rescan 379 MB, model verification
   179 MB). Verify-while-consuming and hash-during-publish cut most of it
   (*estimate*: reopen 5.77 s → 2–2.5 s) with no audio change.
4. **Parallelise the five FFmpeg children of pitch and export.** Today they run
   strictly one after another (`studio.rs:434-442`, `508-525`) with
   `-threads 2 -filter_threads 1`; each child is single-filter-threaded, so a
   five-track pitch job uses ~1 core on an 8-core machine. Output per file is
   unchanged because tracks are independent.
5. **Qualify a GPU provider — deliberately, not accidentally.** The RTX 4060 is
   present and idle. DirectML/CUDA paths are compiled but never wired, the
   effective provider is never observed, and `session.json` writes
   `"provider":"CPUExecutionProvider"` as a literal string. Wiring a provider
   without fixing observability would make a silent CPU fallback
   indistinguishable from real acceleration. Qualify with per-stem numeric
   comparison and an explicit "GPU unavailable → CPU" contract (§8).

**Deliberately not recommended:** optimising the FFT/OLA kernel or the
residual/combine passes (2.1 % and ~0.1 % of wall time respectively), forcing
more threads without measurement (4→6 gained only 1.13×), or any FP16 /
quantised / block-renormalisation shortcut (all change the neural input or
precision and are excluded by the project's quality contract).

---

## 2. Machine, versions and method

| Item | Value | How obtained |
|---|---|---|
| CPU | AMD Ryzen 9 8945HS, 8 physical / 16 logical | project docs (`implementation-plans/native-migration.md:44-47`); `Win32_Processor` query is denied in this sandbox |
| RAM | ~15.3 GiB visible | project docs |
| GPU | NVIDIA GeForce RTX 4060 Laptop, 8188 MiB, driver 617.14 | `nvidia-smi` (`report/measurements/gpu/nvidia-smi-query.out.log`) |
| OS | Windows x64 (session time zone America/Guayaquil) | runtime |
| ONNX Runtime | 1.20.0, CPU build (`git-commit-id=c4fb724`, `CPUINFO_SUPPORTED`) | benchmark JSON `onnx_runtime` field |
| Toolchain | rustc 1.99.1, Flutter 3.47.0 / Dart 3.13.0, MSVC host | `rustc --version`, `.dart_tool/version` |
| Models | 4 pinned ONNX weights, 178.95 MB total (Main 66.76, KARA-2 52.79, drums 29.70, bass 29.70) | local model cache + manifest hashes in every result JSON |
| Input | AppleCats "Algo Que Decirte", 225.5 s, 44.1 kHz stereo FLAC; 24 s excerpt; synthetic kernel stress | `local_audio/closure-evidence/README.md` |

**Reproduction commands** (from the repository root, PowerShell 7):

```powershell
# engine correctness
$env:PATH = (Resolve-Path 'build/windows/x64/runner/Release').Path + ';' + $env:PATH
$env:ATOMUSICATOR_FFMPEG = (Resolve-Path 'build/windows/x64/runner/Release/audio-tools/bin/ffmpeg.exe').Path
cargo test --locked --manifest-path rust_core/Cargo.toml --workspace

# end-to-end (real models, same API the Flutter UI calls)
cargo run --release --locked --manifest-path rust_core/Cargo.toml `
  -p atomusicator-bootstrap-ffi --example desktop_bench -- `
  'local_audio/closure-evidence/applecats-thread-sample.flac' 'local_audio/bench-run'

# thread scaling (1..6 intra-op threads, two passes, fresh stage cache per pass)
rust_core/target/release/examples/cpu_thread_bench.exe `
  'local_audio/closure-evidence/applecats-thread-sample.flac' 'local_audio/bench-t6' 6

# duration-independent memory (real FFT/OLA kernel, no models)
rust_core/target/release/examples/mdx_memory_bench.exe 60
```

**Limits of this study — read before quoting numbers.** Laptop, uncontrolled
power profile and thermal state, no CPU-frequency or affinity pinning; one cold
and at most two warm runs per configuration; the Flutter process RSS was not
instrumented separately from the native process in the stored artifacts; model
weights and user audio stay local and are not redistributed. The 24 s excerpt
is *content*, not a scaled-down song, so per-second costs must not be compared
across the two inputs without the fixed-cost fit in §4.4. On this host the
standard `cargo test` initially failed for *environmental* reasons (see §7.1),
not product reasons.

**Concurrent-workload contamination of the 2026-10-28 campaign.** While the
benchmarks ran (12:45–13:10) another process was actively editing and building
this same working tree: file mtimes show continuous changes from 11:23 to 13:49
(including `Cargo.lock` at 12:48, `pubspec.lock` at 12:47 and new live-pitch
sources at 13:40–13:48). Cargo/Flutter builds saturate the same 16 logical CPUs.
The re-run numbers are therefore a **contaminated upper bound** and must not be
read as the machine's clean performance; the 2026-10-27 campaign in
`local_audio/closure-evidence/` was collected without that contention and is the
better absolute baseline. Ratios within one campaign are still indicative, but
the 4-thread point's 8.7 % pass-to-pass spread is an example of the noise.
**Compare ratios within a campaign, never absolute seconds across campaigns.**
The end-to-end (`desktop_bench`) and physical-playback re-runs could not be
repeated at all because the engine resolves the real per-user profile and the
file sandbox blocks those paths (§7.5); those two rows are the 2026-10-27 values.

---

## 3. Execution model: who runs what

The engine is a composition root (`bootstrap_ffi`) over three Rust crates
(`domain` → `application` → `infrastructure`). The realtime-critical path and
the offline-compute path are cleanly separated; that separation is one of the
project's strengths and should not be disturbed by the optimisations below.

| Thread group | Created by | Count | Lifetime / notes |
|---|---|---|---|
| Flutter UI + embedder threads (platform, raster, IO) | Flutter engine | not measured | UI only; no PCM crosses the bridge |
| Dart→Rust bridge call | calling thread | 1 at a time | passes paths, peaks and DTOs only (`studio.rs:1`) |
| `atomusicator-job` worker | `jobs.rs:149-151` | **1 per job, 1 job max** (`jobs.rs:113-119`) | separation, pitch and export are mutually exclusive; playback is paused when a job starts (`studio.rs:139`) |
| Tokio runtime | `studio.rs:61-65` | 2 workers | model download/IO only; idle during inference |
| ONNX intra-op pool | `onnx.rs:45-47,116-121` | `clamp(logical−2, 1, 6)` = **6** on this host, **one pool per session** | inter-op 1, sequential execution, intra/inter spinning disabled; pool capacity 2 ⇒ up to 12 ORT worker threads exist, but `Run` is serialised per session by `Mutex<Session>` (`onnx.rs:243-245`) |
| `atomusicator-cpal-control` | `playback.rs:124-127` | 1 | owns stream lifecycle; producer is joined here, never in the callback |
| CPAL/WASAPI callback | driver (`playback.rs:821`, `:938`) | 1 (driver-owned, realtime priority) | mixes from an SPSC ring buffer (8 192 `AlignedFrame`s ≈ 459 KB); no I/O, no locks, no allocation |
| `atomusicator-aligned-reader` | `aligned.rs:75-77` | 1 per open stream generation | reads 5 WAVs in 64-tap/1 024-phase sinc blocks and pushes aligned frames |
| FFmpeg child | `audio_tools.rs:297-313` | 1 at a time today | `-threads 2 -filter_threads 1`; pitch/export loops are serial |

**Admission control** (`application/src/resource_admission.rs`,
`infrastructure/src/resource_probe.rs`): a shared pool of 4 GiB and 6 threads,
with a permanent 1.5 GiB "resident" reservation and 1.5 GiB + 6 threads charged
per separation, so exactly one separation is admitted at a time. Under memory
pressure (available physical RAM − 512 MiB < 3 GiB) the pool trims idle sessions
to capacity 1 and re-probes, requiring ≥2 GiB (`resource_probe.rs:7-12,50-70`;
this compact path is uncommitted work in the current tree). Disk admission asks
`frames × 120 B` free on stage cache, projects and work directories
(`studio.rs:316-319`).

**Separation stage chain** (`bounded_separation.rs:202-283`) — strictly serial,
each stage materialising one full-length spool:

```
mix → vocal bus (Main) → KARA on 8 s VAD chunks → lead/backing/active-vocals
    → instrumental-master = mix − active → drums → drum-free = master − drums
    → bass → instrumental = master − drums − bass → validate reconstruction (≤1e-5)
```

Nine stem-length artifacts (`vocal-bus`, `lead`, `backing`, `active-vocals`,
`instrumental-master`, `drums`, `drum-free`, `bass`, `instrumental`) are the
unit of resume and of cache identity. This is why memory is bounded — and why
storage I/O is the second-largest cost (§5).

---

## 4. CPU: measured cost, scaling and where the time goes

### 4.1 What the work actually is

All inference is CPU ONNX Runtime with FP32 MDX-Net models. Window geometry
comes from `domain/src/quality.rs:40-44` with hop 1024:

| Stage (model) | n_fft | tensor (dim_f × dim_t) | window | grid step |
|---|---|---|---|---|
| vocal bus (UVR MDXNet Main) | 7 680 | 256 × 256 | 261 120 frames (5.92 s) | 169 728 (3.85 s) |
| KARA-2 (lead vocal) | 5 120 | 2 048 × 256 | 261 120 frames (5.92 s) | 169 728 (3.85 s) |
| drums (kuielab) | 4 096 | 2 048 × 512 | 523 264 frames (11.86 s) | 340 122 (7.71 s) |
| bass (kuielab) | 16 384 | 2 048 × 512 | 523 264 frames (11.86 s) | 340 122 (7.71 s) |

For the 225.5 s track (9 944 550 frames) the chain executes **60 Main windows,
116 KARA windows (29 VAD chunks × 4), 31 drum windows and 31 bass windows =
78.4 M network frames for 9.94 M frames of audio** — the network sees ~7.9× the
duration. Most of that excess is by design (35 % overlap and centred padding);
the outlier is KARA: each 8 s VAD chunk restarts the window grid, so
`4 × 261 120 = 1 044 480` network frames are computed for 352 800 audio frames
(2.96×). A continuous KARA pass would need ~15.4 M frames instead of 30.3 M
(+14.85 M frames, ≈19 % of all frames; weighted by tensor size the KARA share is
higher — *estimate*, because time per model is not instrumented).

### 4.2 Thread scaling (24 s excerpt, two passes each, fresh stage cache)

Two independent campaigns, same harness and inputs. Compare *within* a
campaign; the absolute level drifted by ≈32 % between days under concurrent
development load (§4.2.1).

| Intra-op threads | 2026-10-27 mean pass (s) | Speed-up | Eff. | 2026-10-28 mean pass (s) | Speed-up | Eff. | Peak RSS (MB, 10-28) | Peak threads (10-28) | CPU-s, 2 passes (10-27 / 10-28) |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 183.3 | 1.00× | 100 % | 210.9 | 1.00× | 100 % | 1 415.7 | 21 | 364.4 / 394.2 |
| 2 | 112.9 | 1.62× | 81 % | 140.0 | 1.51× | 75 % | 1 410.0 | 23 | 385.9 / 477.8 |
| 3 | — | — | — | 103.6 | 2.04× | 68 % | 1 415.8 | 23 | — / 484.2 |
| 4 | 70.7 | 2.59× | 65 % | 101.0 | 2.09× | 52 % | 1 416.6 | 24 | 412.3 / 547.7 |
| 6 | 62.7 | 2.93× | 49 % | 82.7 | 2.55× | **42 %** | 1 426.6 | 30 | 467.2 / 614.9 |

Both campaigns tell the same story:

* **Diminishing returns with a growing overhead.** 4→6 threads buys only
  1.13× (10-27) / 1.22× (10-28) while CPU-seconds inflate: +28 % (10-27) and
  **+56 %** (10-28) from 1 to 6 threads. More threads is not a free lever.
* **The 4-thread "sweet spot" is unproven.** In the re-run, 3 and 4 threads are
  within noise of each other (103.6 vs 101.0 s; single-pass spread up to 8.7 %),
  so the earlier claim that 4 threads is the best wall-clock-per-CPU-second
  trade does not reproduce. What *does* reproduce is that 6 threads is fastest
  in wall time and least efficient per core.
* **Flat memory.** Peak RSS 1.41–1.43 GB at every thread count (10-28), against
  1.486–1.496 GB (10-27) — again duration/thread independent.
* **Thread pools are per session.** Peak OS-thread count rises 21 → 30 as the
  intra-op pool grows, consistent with one ONNX pool per resident session
  (capacity 2) plus the job, control and reader threads.

#### 4.2.1 Campaign-to-campaign drift (do not mix absolute numbers)

The identical harness is **≈32 % slower at 6 threads and consumes ≈32 % more
CPU-seconds** in the 2026-10-28 campaign (82.7 vs 62.7 s per pass; 614.9 vs
467.2 CPU-s for two passes), while the single-thread case is only 8–15 %
slower. The most likely cause is **concurrent work on the same machine**: this
working tree was being edited and built by another process throughout the
campaign (lockfiles rewritten at 12:47–12:48, source changes every few minutes,
new live-pitch sources at 13:40–13:48), and a Rust/Flutter build saturates all
16 logical CPUs. That alone can explain both the slower passes and the
CPU-second inflation, and it makes the re-run an upper bound rather than a
regression.

Two other candidates cannot be ruled out and are worth one controlled
experiment each: (a) the older campaign resolved ONNX Runtime through a
now-missing `ort.pyke.io` download cache, while the re-run uses the packaged
`onnxruntime.dll` — same version string, possibly a different MLAS/threading
build (if true, the *shipped* runtime would scale threads worse than the
development one, which matters); (b) sustained-load power/thermal state.
The 5/30/60-minute FFT/OLA kernel is *faster* in the fresh campaign (3.03 s vs
3.63 s for 5 min), which argues against a globally throttled CPU.

For a clean re-run: no builds, no tests, no editor indexing, fixed power plan,
one process at a time, and a fresh `git stash`-free tree.

**What this study changed in the repository: nothing tracked.** The only
additions are this document (untracked until you commit it) and the untracked
`report/` evidence folder. The pre-existing uncommitted work in `lib/` and
`rust_core/` (live-pitch feature, desktop UI shell, memory-admission compact
path) belongs to your own in-progress development and was neither edited nor
reverted here; the benchmark evidence in §7 was collected with those sources
present.

Per-window cost is not uniform across models (a KARA window is 2.1 M tensor
values, a Main window 0.26 M, drums/bass 4.2 M). Normalising each window by its
tensor size gives ≈6.5–7.1 CPU-s per "KARA-equivalent window" at 6 threads on
both inputs (*derived*), which is consistent and shows the excerpt is not
anomalous per unit of work. Cores busy average **3.82 of 6** over the full run
(3.71 on the 2026-10-27 excerpt, 3.69 in the re-run).

### 4.3 End-to-end and per-operation costs

| Operation | Measured | Notes |
|---|---|---|
| Separation, 225.5 s mix | **418.1 s** wall, 1 597.4 CPU-s | real models, same FFI API as the UI (`desktop_bench`) |
| Separation, 24 s excerpt | 62.7 s per pass (2026-10-27) / 82.7 s (2026-10-28) | same pipeline, fresh stage cache |
| Re-open completed session (cache hit) | **5.77 s** | skips inference entirely; ~1.5 s per audio minute (*estimate*) |
| Closure of a 60-minute session | ~90 s to open (*estimate*) | linear in duration because of full re-verification (§5.3) |
| Pitch, 5 stems, uncached | 5 × single-track Rubber Band render, serial | `audio_tools.rs:204-243`, one FFmpeg child at a time |
| Export, 5 stems | 5 × FFmpeg encode, serial | `studio.rs:508-525` |

Inference is 97–99 % of separation wall time. Everything else in the compute
path is small: the real FFT/OLA kernel processes **4.59 M frames/s
single-threaded** (158.76 M frames in 34.6 s), so one stage of the 3m45s track
costs ≈2.2 s and four stages ≈8.7 s = **2.1 %** of the run. Optimising the FFT,
the transpose/scatter writes, the residual/combine loops
(`bounded_separation.rs:72-109`) or the sieve of small allocations cannot move
the needle; the portfolio in §9 reflects that.

### 4.4 Fixed cost per pipeline pass — measured, not inferred

A 2-second excerpt (cut from the same source with the bundled FFmpeg) was run
through the identical pipeline at 6 threads. It cost **21.20 s and 22.87 s for
its two passes** — 22 s of wall time for two seconds of audio. No
audio-dependent explanation survives that. Fitting `T(d) = F + R·d` on the
same-campaign points `T(2 s) = 22.03 s` and `T(24 s) = 82.67 s` gives
**R ≈ 2.76 s per audio-second and F ≈ 16.5 s per pass**; the 2026-10-27 pair
(24 s → 62.7 s, 225.5 s → 418.1 s) gives F ≈ 20.4 s. Both campaigns therefore
agree on a **duration-independent cost of ≈16.5–20.4 s per pipeline pass**.

The most plausible mechanism is one ONNX session construction per model per
pass: pool capacity is 2 (`studio.rs:66`) and the chain cycles Main → KARA →
drums → bass, so every pass evicts and re-creates all four sessions at ≈4–5 s
each. Supporting evidence: pass 0 and pass 1 inside one process have nearly
identical durations (61.98 vs 63.38 s; 83.30 vs 82.03 s), which rules out a
one-off load cost. Two points are still a fit, not a proof of the cause; a
48 s confirmation run was started and did not complete, and isolating
session-construction time would need one instrumented build.

Share of a job: ≈4 % of a 7-minute song, ≈22 % of a 1-minute excerpt, and the
dominant cost of short interactive runs. **An application session that
separates several files pays it again for every job** while the pool keeps only
two of the four models — this is the case where keeping all four resident
(+≈1.1 GB RSS) or persisting ORT-optimised graphs (disk only, no RAM) is worth
measuring (§9, A2).

### 4.5 CPU conclusion

The machine is not CPU-starved (16 logical / 8 physical, 6 threads reserved,
3.7–3.8 busy on average), and it is not I/O-bound. It is **dependency-starved**:
a serial chain of single-model inference calls, each of which already scales
sublinearly with threads. The highest-value CPU work is therefore *concurrency
of independent inference work within the existing budget*, followed by
*removing redundant network frames* (new profile) and *GPU offload*
(qualification). Micro-optimising the DSP glue is noise.

---

## 5. Memory and storage

### 5.1 Peak memory is bounded and duration-independent

| Scenario | Peak RSS | Peak private | Elapsed | Scope |
|---|---:|---:|---:|---|
| Separation, 225.5 s mix (production path, 2 resident sessions) | **1 460.2 MB** | — | 418.3 s | `applecats-bounded-resources.json` |
| Separation, 24 s excerpt, 6 threads | 1 496.4 MB | — | 126.1 s | `threads-6-resources.json` |
| Separation, 24 s excerpt, 1/2/3/4/6 threads (2026-10-28 re-run) | 1 415.7 / 1 410.0 / 1 415.8 / 1 416.6 / **1 426.6 MB** | 1 404.8 / 1 405.4 / 1 405.7 / 1 405.7 / 1 418.2 MB | 423.5 / 280.9 / 208.2 / 203.3 / 166.6 s | `report/measurements/threads/*-resources.json` |
| Same excerpt, 1/2/4 threads (2026-10-27) | 1 485.9 / 1 495.4 / 1 486.9 MB | — | — | `threads-{1,2,4}-resources.json` |
| Real FFT/OLA kernel, 5 / 30 / 60 min | 41.98 / 41.10 / **40.86 MB** | 36.7 / 36.9 / 36.9 MB | 3.6 / 17.8 / 34.6 s | `kernel-{5,30,60}-min-resources.json` |
| Retained in-memory reference path (same 225.5 s mix) | 2 023.0 MB | — | 472.1 s | `applecats-native-v1-resources.json` |
| Dart/Flutter heap for a session | tens of KB (DTO peaks ≈39 KiB; waveform detail ≤156 KiB transient) | — | — | static audit |

The flat 1.41–1.50 GB across a 9.4× duration ratio and every thread count, and
the flat 40.5–42.0 MB kernel series across a 12× ratio, are the strongest
evidence that the duration-bounded rewrite works as designed: **memory is a
function of active models, windows, queues and caches — not of song length.**
Peak OS-thread count rose 21 → 30 from 1 to 6 intra-op threads, consistent with
one ONNX pool per resident session.

Composition (*estimate*, from the static audit): 178.95 MB of model files;
114 MB of resident weights for two sessions; **≈1.1 GB of ONNX CPU
arena/workspace for those two sessions**; 33–66 MB of per-window transients;
<1 MB playback. Nothing proportional to duration appears in the production path:
spools are read in 65 536-frame blocks, MDX accumulates one window, waveforms
are capped at 65 536 base buckets.

The admission estimate is honest: 1.5 GiB reserved per separation against a
measured 1.460 GB peak (≈4.6 % headroom), plus a 1.5 GiB resident charge against
a 4 GiB pool. The compact path (single resident session) would save an estimated
0.4–0.7 GB by dropping one arena and one set of weights, at the cost of ~0.3–0.8 s
of extra session loads per job.

**Do not cite `benchmarks/results/2026-10-27-native-v1-resources.json`
(3.06 GB) as the in-memory comparison.** That run reports 11.3 s elapsed with
only 3.4 CPU-s, i.e. it is not a separation; the meaningful reference is the
2.02 GB / 472.1 s artifact above.

### 5.2 Storage footprint (measured on this host, 2026-10-28)

| Tree | Bytes | Human | Nature |
|---|---:|---:|---|
| `build/windows/x64/runner/Release` (34 files) | 513 049 480 | **489.3 MiB** | shipped app folder |
| — `audio-tools/` (ffmpeg 227.8 MB + ffprobe 227.6 MB) | 455 525 965 | 434.4 MiB | 88.8 % of the release |
| — `onnxruntime.dll` | 18 894 336 | 18.0 MiB | CPU runtime |
| — `atomusicator_bootstrap_ffi.dll` | 7 306 752 | 7.0 MiB | engine |
| — `flutter_windows.dll` + `data/` | 29 187 862 | 27.8 MiB | UI runtime |
| `packaging/dist/Atomusicator-Setup-0.6.0.exe` | 136 984 991 | **130.6 MiB** | LZMA2 installer (3.75:1) |
| `%LOCALAPPDATA%\Atomusicator` (213 files) | 2 620 052 530 | **2.44 GiB** | app cache + journals |
| — `cache\stages\bounded-v1` | 1 730 432 792 | 1.61 GiB | intermediate spools |
| — `cache\stages\pitch-v1` | 889 619 738 | 848.4 MiB | pitch renders |
| Historical application data (142 files) | 8 465 345 139 | **7.88 GiB** | user sessions + weights |
| — `projects` (16 sessions) | 8 286 313 110 | 7.72 GiB | published stems (incl. 4.37 GiB of nested pitch renders) |
| — `models` (4 ONNX) | 179 000 353 | 170.7 MiB | weights (not shipped, re-downloadable with consent) |
| `rust_core/target` (dev machine) | 23 437 795 052 | 21.83 GiB | build cache, incl. a **617.3 MiB CUDA provider DLL that is never shipped** |
| `local_audio/` (QA evidence) | 2 721 043 141 | 2.53 GiB | campaign artifacts, archive don't delete |
| `build/` (incl. 3 installer-qualification copies) | 2 603 123 627 | 2.42 GiB | reproducible |

### 5.3 Per-session cost, write amplification and retention

Stems are 32-bit float WAV, 44.1 kHz stereo: **352 800 B/s = 21.17 MB/min =
1.27 GB/h per stem**.

| Quantity | Per minute | Per hour |
|---|---:|---:|
| Published session (5 stems, float32) | 105.8 MB | 6.35 GB (5.91 GiB) |
| Decoded source spool | 21.2 MB | 1.21 GB |
| 9 intermediate stage spools | 190.5 MB | **11.4 GB (10.65 GiB)** |
| Required free disk (`frames × 120 B` admission) | 317.5 MB | **18.2–19.1 GB** |
| Pitch batch (5 rendered stems) | 105.8 MB | 6.35 GB |
| Legacy/pitch cache quota | 2 GiB → covers ≈20.2 min of song | — |
| Stage cache quota | 4 GiB → covers ≈21 min of spools | — |

**Measured write amplification** (24 s run, 6 threads, §7.5): one pass wrote
118.5 MB (decoded 8.47 + nine spools 76.2 + five published stems 42.3) against
42.3 MB of published audio = **2.80× write amplification**; adding the publish
re-hash, the waveform scan and the stage reads, total read+write traffic is
≈6–7× the published size. At the hourly constants above this is ≈37 GB of I/O
traffic per hour of audio separated (*derived*).

Three consequences worth acting on:

1. **The 4 GiB stage quota bounds *idle* cache, not a single long job.**
   `FloatSpoolStore::create` prunes to 4 GiB before every new artifact
   (`float_spool.rs:231,47`) and the adapter prunes again after publication
   (`studio.rs:346`), but pruning only reclaims artifacts whose `.lease` file is
   not locked, and every artifact of the running job is held through a live lease
   until the job ends. A 60-minute separation therefore legitimately holds
   ≈10.9–11.4 GB of spools while running; the only real guard is the disk
   admission request. Documenting "≈19 GB free per 60-minute source" is a
   user-facing requirement the README still lacks.
2. **The 2 GiB pitch quota self-defeats above ~20 minutes of song.** A
   5-stem batch for a 30-minute song is 6.35 GB; it is pruned after being
   produced, so the next transposition re-renders all five stems every time,
   silently. A duration-aware quota or a "pitch render too large to cache"
   diagnostic is needed.
3. **Read amplification dominates a cache hit.** Opening a 3.75-minute session
   moves ≈2.37 GB in 5.77 s (≈411 MB/s effective): 683 MB of spool
   re-verification, 1 137 MB of publish read/write/hash, 379 MB waveform rescan,
   179 MB model verification. For a one-hour session that grows to ~90 s of
   opening. Verify-while-consuming and hash-during-write remove most of it
   without changing a single audio sample.

**Safe to clean vs never touch.** *Never*: original recordings referenced by
`sessions.json`, published session folders (the only copy of the stems), the four
ONNX weights in the legacy models directory, and `jobs\*.json` / `.job-owner`
recovery journals. *Safe (rebuildable)*: `cache\stages\bounded-v1` and
`pitch-v1`, `%TEMP%\.atomusicator-*` (see §7.4, finding 8), `rust_core/target`, `build/`,
`.dart_tool`, `packaging/dist` copies. *Review, don't bulk-delete*:
`outputs\pitch-renders` nested inside eight legacy sessions (4.37 GiB) and one
apparent duplicate pair of legacy sessions (≈620 MiB) — user data.

### 5.4 Storage-side hard limits

* RIFF/WAV: `wav_codec.rs:128` rejects files above 4 294 967 167 bytes =
  **536 870 896 frames = 3.38 h** per float32 stereo stem. No RF64/BW64. The
  error message says "shorter than 3 hours"; a 3–4 h single-file session is
  therefore refused at publication time, after the expensive inference.
* Every spool write, WAV and `session.json` is fsynced and publication is a
  directory rename (crash-safe), with the commit marker written *before* the
  rename (`block_workspace.rs:72-78`), so "committed" is re-derived from
  `session.json` existence during recovery.
* Installer payload is 130.6 MiB because two static 217 MiB FFmpeg binaries
  dominate it; making audio tools a first-run download or a shared-library
  build is the only large packaging win available (§9, Tier C).

---

## 6. GPU: present, idle, unqualified — and why that is deliberate

**Measured:** `nvidia-smi` reports `NVIDIA GeForce RTX 4060 Laptop GPU`,
`8188 MiB` total, **`0 MiB` used, `0 %` utilisation**, driver 617.14, sampled
before, during and after the CPU benchmark runs (`report/measurements/gpu/`).
The Flutter UI itself is a 2D compositor (Skia/ANGLE/Impeller paths inside
`flutter_windows.dll`); its VRAM use is a few tens of MiB (*estimate*), unrelated
to audio.

**There is no GPU execution path, by construction:**

* `OrtSessionPool::new` hardcodes `provider_order = vec![Cpu]`
  (`onnx.rs:40-41`); every production constructor calls `new(2)`.
  `with_provider_order` exists but has **no caller** in the workspace.
* The Windows build compiles `directml` + `cuda` ORT features
  (`crates/infrastructure/Cargo.toml`), producing a 617.3 MiB
  `onnxruntime_providers_cuda.dll` and a TensorRT provider DLL in
  `rust_core/target/release` — and `packaging/build_windows.ps1` **deletes them**
  before packaging, so the shipped payload contains only `onnxruntime.dll`
  (18.9 MB) and `onnxruntime_providers_shared.dll` (10.7 KB).
* Providers are registered `fail_silently()` and nothing reads the *effective*
  provider; `session.json` asserts `"provider":"CPUExecutionProvider"` as a
  literal (`studio.rs:337`). Today that is true; after enabling a GPU it would
  keep claiming CPU even if the GPU silently fell back — an observability defect
  that must be fixed **before** any acceleration is advertised.

**What qualification would require** (project plan F7, unchanged):
wire `[DirectMl, Cpu]` explicitly; disable memory pattern for DirectML (already
conditional, `onnx.rs:122-128`); keep `Run` sequential per session (DirectML does
not allow concurrent `Run` on one session); enumerate and log the *effective*
provider per session; re-measure per-stem output against the frozen CPU
reference (FP32 kernels differ numerically — this is a qualification, not a
free toggle); measure VRAM and sustained timing on the target laptop; confirm
redistribution terms for the provider DLLs; and add an explicit CPU fallback
path with provenance recorded in the job journal.

**Expected upside is unknown and must not be promised.** A 2–4× end-to-end
speed-up on an RTX 4060-class laptop is plausible for convolution-heavy FP32
inference, but this study did not test it and the project's own plan treats GPU
as optional and unqualified. Until F7 runs, the honest statement is: *the
machine has an idle GPU and the application cannot use it.*

---

## 7. Engine and thread verification

### 7.1 Test suite (re-run 2026-10-28)

| Suite | Result | Notes |
|---|---|---|
| `cargo test --workspace` (debug, locked) | **115 passed, 0 failed, 8 ignored**, 34 targets, 10.7 s | with the packaged runtime on `PATH` and `ATOMUSICATOR_FFMPEG` set |
| Opt-in tests (`--ignored`, real models + FFmpeg) | **7 passed, 1 failed**, 132 s total | `bounded_real_models_match_the_frozen_memory_pipeline` (67.8 s), `real_models_separate_a_short_stereo_signal_and_reconstruct_it` (33.4 s), FFmpeg pitch/export, five-stem DSP timing, live leases and the journal fixture all pass |
| Flutter suite | not re-run in this study | prior evidence: 25 widget/controller tests with the native DLL after the FLAC import fix |

**Two reproducibility findings (real, worth fixing).**

1. *Dangling ONNX Runtime link.* The first `cargo test` attempt failed with
   `0xc0000135 STATUS_DLL_NOT_FOUND` on five targets: the
   `rust_core/target/**/onnxruntime.dll` files were **symlinks into
   `%LOCALAPPDATA%\ort.pyke.io\...`**, a cache belonging to another Windows
   profile, so the links dangle. `PATH` alone does not help (the broken link in
   the executable directory wins); the repair is to materialise the *shipped*
   `onnxruntime.dll` (18 894 336 B, byte-identical to the packaged one) into
   `target\{debug,release}\{,deps,examples}`. Environment/tooling defect, not an
   engine defect, but "the standard suite passes" is untrue on a fresh machine
   until it is documented.
2. *One non-hermetic opt-in test.*
   `studio::tests::separation_job_publishes_a_reopenable_session` failed with
   `Acceso denegado. (os error 5)` while the other seven opt-in tests passed.
   Its `backend()` helper isolates jobs, projects and pitch (`studio.rs:598-611`)
   but the separation path still writes through
   `WorkspacePathPolicy::system_default()` into the real per-user stage cache
   (`studio.rs:320`), so the test depends on filesystem permissions outside the
   workspace. It needs the cache root injected (or an explicit environment skip);
   until then the opt-in suite cannot be reported as fully green, and it is not
   evidence of an engine defect.

### 7.2 Determinism and audio-integrity evidence

* The prior campaign ran **eight passes across 1/2/4/6 threads** and compared
  samples: `threads-*-comparison.json` report **zero sample differences** between
  configurations. Thread count therefore does not perturb the numerics — a
  required invariant for every optimisation in §9.
* Whole-track separation of the 225.5 s mix reproduces the frozen native
  baseline with **maximum reconstruction error 2.98e-7**, against the domain
  contract of 1e-5 (`quality.rs:3`), and the five stems are finite, aligned,
  same-rate, same-frame-count.
* Re-opening a completed session from stage artifacts yields the same stem
  hashes (`applecats-final-cache-reopen.json`, `applecats-confirmed-comparison.json`).

### 7.3 Realtime thread correctness (static audit, no defect found)

The callback (`playback.rs:840-922`) allocates nothing, takes no locks, performs
no I/O, pops one `AlignedFrame` from an `rtrb` SPSC ring buffer per output frame,
drops stale generations, emits silence on underrun **and freezes the shared
clock** instead of letting one stem run ahead. Five stems share one producer and
one clock (`aligned.rs:44-186`), so per-stem drift is structurally impossible in
the production path; the per-stem-reader path that could drift is reference-only.
Resampling (64-tap, 1 024-phase sinc) runs in the producer. Device errors reach
the UI as state (`playback.rs:833-857`), and the producer is joined on the
control thread, never in the callback. The physical-device check passed
play/pause/repeated seek/EOF with master gain at zero
(`final-physical-playback-check.json`). One production `unsafe` outside
generated bindings (`GlobalMemoryStatusEx`, correct buffer and length); no data
races identified; ONNX sessions are reference-counted and never evicted under a
live `Run`.

### 7.4 Findings that need attention (severity, not hypotheticals)

| # | Finding | Severity | Evidence |
|---|---|---|---|
| 1 | GPU unreachable **and unobservable**; `session.json` provider is a literal string | High for any future GPU claim; none today | `onnx.rs:40-41,186-192`; `studio.rs:337` |
| 2 | KARA re-pads the window grid per 8 s VAD chunk: 30.3 M network frames vs 15.4 M ideal (+19 % of all frames) | High (CPU), but **changes output** — new profile only | `bounded_separation.rs:133-188`; `mdx.rs:240-298` |
| 3 | Declared "reflect" padding degenerates to zero padding in the first/last window because the reflection maps the zero-filled trim onto itself; differs from the legacy `torch.stft(center=True, reflect)` for the first/last 87 ms (Main) / 186 ms (Bass) | Medium (parity) — needs a targeted test, not yet a proven defect | `mdx.rs:92-103`; `mdx_bounded.rs:57-71` |
| 4 | Two normalisation policies coexist: bounded stages normalise by the **stage-global** peak, the KARA 8 s path by a **per-chunk** peak, while the profile declares "global-input-peak-per-model-call" | Medium (documented-policy mismatch) | `mdx_bounded.rs:39,70`; `mdx.rs:250-254`; `quality.rs:67` |
| 5 | Cancellation is sampled between windows: worst-case stop latency ≈1 window, i.e. ≈3.8 s (Main/KARA) to ≈7.7 s (drums/bass) | Medium (UX) | `mdx_bounded.rs:54,78,117` |
| 6 | Starting any job pauses playback (`studio.rs:139`), so the planned "separate while playing" stress scenario cannot currently occur, and inference/playback contention is unmeasured | Low (product scope) | `studio.rs:136-141` |
| 7 | Journal writes `output_directory` before publication (`studio.rs:339` precedes `publish_blocks`), so a failed job can advertise a path that does not exist | Low | `studio.rs:339-343` |
| 8 | Leaked per-session waveform temp directories: the process-global `Studio` is never dropped, so each run leaves `.atomusicator-*` dirs in `%TEMP%` | Low (storage hygiene) — **measured: 85 directories / 27 753 280 B** from 17 runs, no startup sweep | `waveform_index.rs:14-18,96-100`; `studio.rs:44`; verified by enumeration |
| 9 | Dead code compiled into the shipped DLL: whole-buffer MDX adapter, Symphonia decoder, Hound exporter, peak cache, local session workspace, `with_provider_order`. Any future "simple" refactor reaching for them reintroduces O(duration) memory (≈476 MB transient per 5-minute stem) | Low (risk) | grep-verified |
| 10 | All shipped binaries are unsigned; trust rests on the build-time SHA-256 manifest only | Low (trust, release gate) | `Get-AuthenticodeSignature` → NotSigned |

### 7.5 Measured re-run summary (2026-10-28)

Full tables and provenance: `report/parts/measurements.md`; raw artifacts under
`report/measurements/`.

| Measurement | Result |
|---|---|
| Default suite | 115 passed / 0 failed / 8 ignored (34 targets, 10.7 s) |
| Opt-in suite (`--ignored`) | 7 passed / **1 failed** (non-hermetic separation test, os error 5) |
| Thread scaling 1/2/3/4/6 on the 24 s excerpt | 210.9 / 140.0 / 103.6 / 101.0 / **82.7 s** per pass; 1.51× / 2.04× / 2.09× / 2.55×; peak RSS 1.41–1.43 GB; peak threads 21→30; CPU-seconds +56 % at 6 threads |
| Campaign drift | same harness ≈32 % slower than 2026-10-27 at 6 threads — explained by concurrent edits/builds of this working tree during the campaign (lockfiles rewritten mid-run); the fresh numbers are an upper bound, the FFT/OLA kernel is *faster*, so not a CPU-wide throttle (§4.2.1) |
| Fixed cost per pass | **22.03 s for a 2 s input** ⇒ F ≈ 16.5 s (re-run) / 20.4 s (10-27); four ONNX session constructions per pass |
| Duration-independent memory (5/30/60 min FFT/OLA kernel) | peak RSS 40.5 / 40.5 / 40.5 MB, exact frames, 3.0 / 18.8 / ≈37 s |
| Disk write amplification (24 s run) | 118.5 MB written per pass vs 42.3 MB published = **2.80×**; with re-hash + waveform scan + stage reads ≈6–7× total traffic |
| End-to-end desktop run | **not re-run** — the engine validates the real `%LOCALAPPDATA%` projects path and the sandbox denies it; 2026-10-27 value stands (418.1 s, 1 460 MB) |
| Playback thread check | **not re-run** — same profile-path limitation; 2026-10-27 physical check passed |
| GPU provider inventory | no CUDA/TensorRT/DirectML provider shipped; provider order CPU-only; RTX 4060 at 0 MiB / 0 % |

**Bottom line of the verification:** the engine is functionally healthy — the
deterministic DSP contracts, the real-model equivalence test, the bounded
memory kernel, the session leases, the journal and the realtime callback tests
all pass, and the one failure is a test-harness isolation defect rather than
engine behaviour. What is *not* verified here is end-to-end wall time and
physical playback on the current binaries; both need a session with write
access to the user profile.

---

## 8. What acceleration would require without changing audio

The project's own contract is explicit and correct: a change is rejected if it
degrades extraction or synchronisation, even when it improves median time
(`implementation-plans/native-migration.md`, §11). Every recommendation below is
compatible with that contract, and each names its proof obligation.

**Proof obligation for every optimisation** (must be re-run before merge):
bit-identical float32 WAV hashes for the frozen `native-v1` profile on the
existing corpus; reconstruction ≤1e-5; identical results across thread counts;
the eight environment-gated tests (real models, FFmpeg, leases) green; peak RSS
and disk budgets within admission; and (for anything touching profiles) SI-SDR
per stem plus blinded listening on a reference corpus.

---

## 9. Optimisation portfolio (ranked by expected gain / effort)

### Tier A — no change to audio, no change to the DSP contract

| # | Change | Expected gain | Audio risk | Effort | How to verify |
|---|---|---|---|---|---|
| A1 | **Concurrent independent inference within the 6-thread budget** (two sessions of the same model, 3 threads each; e.g. two KARA chunks or two MDX windows; ordered overlap-add preserved) | 25–45 % throughput (*estimate*: current 3.71/6 busy cores at 6 threads; 2 concurrent 3-thread runs ≈ 2 × 2.1× aggregate vs 2.93×), at **+≈0.4–0.6 GB RSS** for the extra session/arena — the admission budget must be revised | None if accumulation order is preserved; thread-count invariance so far demonstrated at 1/2/4/6 samples and must be extended to 3 | M | bit-identical stems vs frozen baseline; 3-thread entry in the invariance matrix; wall/CPU/cores-busy vs 6×1; peak RSS |
| A2 | **Remove the per-pass session-construction cost** (measured F ≈ 16.5–20.4 s, §4.4): persist ORT-optimised graphs to disk (cost ≈178 MB, no RAM cost) **or** keep all four sessions warm (RAM +≈1.1 GB, which conflicts with A6) | Removes F per pass for later jobs (≈4 % of a 4-min song, ≈22 % of a 1-min QA excerpt); zero audio change | None (same graph/kernels) | S–M | per-job wall time for the 2 s and 24 s inputs; peak RSS; stem hashes |
| A3 | **Verify while consuming and hash while publishing** (drop spool re-read on cache hit, hash stems during the publish write) | −683 MB and −379 MB of I/O per reopened job; reopen 5.77 s → 2–2.5 s (*estimate*) | None — same integrity check, moved earlier | M | reopen wall time; corruption-injection tests still fail |
| A4 | **Parallelise the five FFmpeg children of pitch and export** (bounded 3–5 concurrent, one thread per child, staging + atomic rename, cancellation-aware progress) | Pitch/export jobs up to ≈3–4× faster; single-track latency unchanged | None — tracks are independent files; Rubber Band is deterministic per file | M | output hash equality per track; CPU/thread budget; cancel latency |
| A5 | **Remove redundant work per window**: reuse buffers, drop one of the two tensor copies and the double finite-scans, skip the per-sample SHA-256 stage key when no cache is configured | 0.5–1.6 % wall; lower allocator variance (*derived*: ≈10.8 GB less traffic per track) | None if arithmetic order untouched | S | stem equality; wall time |
| A6 | **Compact session pool by default** (capacity 1 for the serial chain) | Peak RSS −0.4…−0.7 GB (*estimate*), at +0.3–0.8 s of extra loads per job; mutually exclusive with the "keep sessions warm" variant of A2 | None — same graph/precision; compact path already verified ≤1e-6 | S | peak RSS on 225.5 s; stem hashes |
| A7 | **Build the waveform index from the spool during separation** instead of re-reading five published WAVs at open; reuse `FloatSpool` file handles | Removes a 379 MB rescan and ~1.5 s per audio minute of open time (*estimate*) | None (display data only) | S–M | open time; waveform equality tests |
| A8 | **UI paint isolation**: `RepaintBoundary` for the playhead, `select` on position instead of watching the whole studio state, keep the painter off `progress` | Stops re-tessellating ≈40 000 path segments every 180 ms at 400 % zoom; smoother UI, less raster CPU | None (paint output identical) | S | DevTools frame times, playing vs paused |
| A9 | **Measure 8 threads** once, under the same policy | ≤10 % (4→6 gave only 1.13×); possibly zero | None — samples were identical at 1/2/4/6 | S | `cpu_thread_bench`; note cache signatures don't include thread count today |
| A10 | **Block-wise polyphase resampler** in the playback producer | ~3–10 % of one core at 48 kHz output; headroom while other work runs | LSB-level unless tap accumulation order is preserved — qualify with a null test | M | callback CPU; null test / SNR |

### Tier B — changes numerics or algorithm: requires a new versioned profile

| # | Change | Expected gain | Cost / risk |
|---|---|---|---|
| B1 | **One continuous KARA pass** instead of 29 × 8 s windows | −19 % of all network frames (more when weighted by tensor size); ≈ −70 s of 418 s (*estimate*) | **Changes output** (per-chunk peak, padding, VAD gating). Forbidden under `native-v1`; belongs to `context-candidate-v1` with SI-SDR + blinded listening |
| B2 | **GPU execution provider** (DirectML first, CUDA if packaging/licensing allow) | Unknown; 2–4× plausible (*estimate*, untested) | Provider kernels change numerics; needs effective-provider observability, per-stem comparison, VRAM/sustained campaign, redistribution review |
| B3 | **ONNX Runtime upgrade** (newer CPU EP schedulers) | Unknown; may improve sublinear thread scaling | Dependency change; re-qualification of runtime signature and caches |
| B4 | **FP16 / quantisation / ensembles** | Potentially large | Explicitly out of scope: precision change, needs its own quality campaign |

### Tier C — storage, packaging and hygiene (no audio impact)

| # | Change | Gain |
|---|---|---|
| C1 | Duration-aware pitch quota (or explicit "too large to cache" diagnostic) | Stops silent 5-stem re-render above ~20 min songs (6.35 GB/batch) |
| C2 | Long-job storage guidance: 18–19 GB free per 60-minute source; document the 3.38 h RIFF limit | Removes a documented late failure after hours of inference |
| C3 | Reap `%TEMP%\.atomusicator-*` at startup and add an explicit engine shutdown that drops the loaded session | Removes 27.75 MB per 17 runs (measured) and unbounded growth |
| C4 | Drop `directml`/`cuda` features from the release build; build them only for GPU qualification jobs | −617 MB of dead build artifacts per machine; shorter builds |
| C5 | Optional/shared FFmpeg (first-run download or shared libs) | Installer 130.6 MiB → ≈25 MiB (*estimate*); release folder −434 MiB |
| C6 | Eviction/notification for published sessions, and stop defaulting new projects into the legacy folder | Bounds the 7.72 GiB legacy tree that users may believe obsolete |
| C7 | Sign the binaries or verify DLL hashes at startup | Trust beyond the build machine |

---

## 10. Recommended measurement roadmap (next, in order)

| ID | Experiment | Target / acceptance |
|---|---|---|
| E1 | Two concurrent inference streams × 3 threads vs one × 6 | ≥15 % wall improvement on the 24 s excerpt with bit-identical stems and ≤+0.7 GB RSS; if <5 %, close the idea |
| E2 | Isolate the per-pass fixed cost with one instrumented build (time the four session constructions vs decode/verify/publish), then implement A2 | ≥15 s saved per later separation on a 4-min song with identical stems and peak RSS within +1.2 GB; if the cost is not session construction, close A2 and keep the finding |
| E3 | Pitch/export parallelism with bounded concurrency | ≥2.5× on a 5-track pitch job with per-track hash equality |
| E4 | Verify-while-consume on reopen | Reopen ≤2.5 s for the 225.5 s session with corruption tests still failing |
| E5 | GPU qualification spike (DirectML, one model, offline) | Numeric diff per stem within a declared tolerance, effective provider logged, VRAM/sustained timing recorded before any product decision |
| E6 | Long-session campaign with 30/60-minute real-model runs (still open in the project plan) | RSS growth ≤ max(256 MiB, 20 % of the 30-min value), no underruns, storage budget respected |

Every experiment above changes no shipped DSP parameter; they are scheduling,
I/O and packaging changes wrapped in the project's existing proof obligations.

---

## 11. Practical guidance while the above is pending

* **Threads**: keep 6 intra-op threads (the current policy) — it is the fastest
  measured configuration, but do not expect more from adding threads; the
  remaining gain is concurrency, not count.
* **RAM**: the engine needs ≈1.5–1.6 GB of private working set for a separation
  plus the Flutter shell; 8 GB systems are comfortable, 4 GB systems will hit the
  compact path and slower loads.
* **Disk**: budget ≈19 GB free per hour of source (admission enforces it), keep
  the 4 GiB/2 GiB cache quotas in mind, and expect a 3.38 h per-file ceiling.
* **Pitch on long material (>20 min)**: assume every transposition re-renders
  all five stems; plan accordingly until C1 lands.
* **GPU**: none is used today; do not size a machine on GPU expectations until
  F7/E5 completes.
* **Antivirus/backup tools** that scan the spool directory will amplify the
  measured I/O (≈2.4 GB per reopen) — exclude the app cache folder for
  separation workloads.

---

## Appendix A — artifact index

| Path | Contents |
|---|---|
| `local_audio/closure-evidence/threads-{1,2,4,6}-{result,resources}.json` | thread-scaling timings, CPU-s, peak RSS |
| `local_audio/closure-evidence/kernel-{5,30,60}-min-{result,resources}.json` | duration-independent FFT/OLA memory |
| `local_audio/closure-evidence/applecats-bounded-{result,resources}.json` | end-to-end 225.5 s separation |
| `local_audio/closure-evidence/applecats-final-cache-reopen.json` | cache-hit reopen |
| `local_audio/closure-evidence/final-physical-playback-check.json` | real driver play/pause/seek/EOF |
| `report/measurements/` | fresh 2026-10-28 runs, logs, GPU probes, resource samples |
| `report/parts/rust-engine.md` | static engine audit with `file:line` evidence |
| `report/parts/dart-storage.md` | Dart/Flutter, storage and packaging audit |
| `benchmarks/README.md` | how the native evidence runner works |

## Appendix B — derived arithmetic used above

```
per-stem float32 44.1 kHz stereo        = 44100 * 2 * 4            = 352 800 B/s
5-stem session per hour                = 352 800 * 5 * 3600       = 6 350 400 000 B
9 stage spools per hour                = 352 800 * 9 * 3600       = 11 430 720 000 B
disk admission per hour (frames*120)   = 44100 * 120 * 3600       = 19 051 200 000 B
KARA network frames per 8 s VAD chunk  = 4 * 261 120 = 1 044 480  (2.96x audio)
full track: 60*261 120 + 116*261 120 + 31*523 264 * 2 = 78 399 488 network frames
parallel efficiency (6 threads, 24 s)  = 2.925 / 6                 = 48.8 %
cores busy (full track)                = 1597.375 / 418.342        = 3.82
fixed-cost fit, 2026-10-28 (6t)     = R 2.76 s/audio-s, F 16.5 s   (T(2 s)=22.03, T(24 s)=82.67)
fixed-cost fit, 2026-10-27 (6t)     = R 1.76 s/audio-s, F 20.4 s   (T(24 s)=62.68, T(225.5 s)=418.1)
write amplification (24 s pass)     = 118 531 291 B written / 42 336 340 B published = 2.80x
```
