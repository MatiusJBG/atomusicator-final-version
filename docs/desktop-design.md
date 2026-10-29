# Penpot desktop implementation

Reference: `Separador Vocal · UI`, file `4b7520f4-628e-8193-8008-d4b8c22582c9`,
page `4b7520f4-628e-8193-8008-d4b8c22582ca`. The boards were inspected through
Penpot's rendered SVG, including actual coordinates, type sizes and fill colors.

## Screen map

| Screen | Penpot board | Application behavior |
| --- | --- | --- |
| Inicio | `1c4112ed-c56f-805a-8008-dc12d1eb87d4` | Local projects, search, filters, recent activity and project opening |
| Atomizador vacío | `1c4112ed-c56f-805a-8008-dc12e571fdc7` | Dashed import area, format pills, three instruction cards, privacy footer |
| Listo para separar | `1c4112ed-c56f-805a-8008-dc12ef46772b` | Actual source waveform/metadata, stem checkboxes, two-column review and confirmation |
| Procesando | `1c4112ed-c56f-805a-8008-dc13268197f0`, `1c4112ed-c56f-805a-8008-dc1333fb9c46` | Source-to-stems diagram, real job progress, five stages, cancellation |
| Resultado | `1c4112ed-c56f-805a-8008-dc130a24bd12` | Completion summary, stem previews and explicit mixer/export/folder actions |
| Mezclador | `1c4112ed-c56f-805a-8008-dc16246a7949` | Five aligned tracks, fixed controls, timeline zoom and three-part dock |
| Exportar | `fd17df5e-0ef2-80a3-8008-d4c080f6e5ca` | Two-column track selection, format pills, destination and in-dialog export progress |
| Configuración | `1c4112ed-c56f-805a-8008-dc12b5790aef` | Appearance and engine cards; saved text scale, density, reduced motion and grid settings |

## Geometry and styling

The default window is 1440 × 900. A 38px title bar sits above a 66px header;
the navigation rail is always 76px. The empty import area starts at x=96,
y≈176, is at most 1248px wide, and is followed by three 196px instruction
cards. The mixer controls are 436px wide on a large desktop. Tracks start
near y=184 and have an 8px gap; playback, pitcher and master share a 184px
bottom dock. Smaller windows use scrollable content and stacked settings.
The supported minimum window remains 920 × 640.

Inter is bundled locally, including its OFL license in `assets/fonts/`.
The palette follows the reference: canvas `#0B0F14`, navigation `#090E14`,
panels `#131A22`, primary `#2DD4BF`, text `#E6EDF3`, muted `#7E93A8`.
Stem colors are coral, purple, amber, blue and green; the playhead is orange.
Controls and user-facing navigation are in Spanish. Windows window controls
remain real native actions rather than simulated traffic-light buttons.

## Engine boundaries

The engine remains the existing native MDX/UVR/Kuielab CPU pipeline. Prototype
content such as Demucs, RTX 4060, BPM and time estimates is not presented as
actual engine data. The import list contains the four qualified formats,
WAV/MP3/FLAC/M4A. AIFF is not advertised as a qualified format.

The engine always generates all five stems. Review checkboxes initialize the
export selection; the review screen explicitly explains this. A master-mix
export tile is visible but disabled, because the exporter supports individual
stems. CUDA and the light-theme preview are explicitly unavailable. Advanced
sample-rate, bit-depth, channel and normalization controls remain accessible
without replacing the compact export layout. Existing model-download consent,
headroom warnings and per-file normalization explanations are preserved.

Source previews use the already bundled FFprobe/FFmpeg tools, stream downsampled
PCM into 650 min/max buckets, and terminate decoding when the provider is
disposed. They neither start playback nor alter the current mixer session.
Projects are discovered from direct child session folders in the configured
projects directory; no disk-wide recursive search or directory-link traversal
is performed. Corrupt or incomplete session metadata is excluded.

UI preferences are saved separately from native engine configuration in
`%LOCALAPPDATA%/Atomusicator/desktop-ui.json`. New project clears imported-source
selection, while retaining the existing engine session and original files.

## Verification and visual review

- `flutter analyze --no-pub`
- `flutter test --no-pub`
- `flutter test --no-pub test/support/render_penpot.dart --update-goldens`
- `flutter build windows --release --no-pub`

The visual renderer loads Inter and Material Icons and renders eight explicit
test-fixture screens at 1440 × 900 to `logs/penpot-*.png`. These are test
fixtures, not screenshots of user projects. Functional tests cover navigation,
review-to-separation-to-result-to-mixer, fresh projects, native commands,
selection, cancellation/recovery, export safeguards, project discovery,
settings persistence, and minimum desktop windows at 125% text scale.

To include native tests, set `ATOMUSICATOR_NATIVE_LIBRARY` to the built bridge
and `ATOMUSICATOR_FFMPEG` to the bundled FFmpeg. On this host FFmpeg cannot write
to the default system temporary directory, so native integration tests were
run with process-local `TEMP` and `TMP` set to `logs/native-test-temp`.
No OS permissions or security settings were changed.
