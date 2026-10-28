# Distribution and license readiness

Status: **release gate open**. This is an inventory of required work, not a
certificate that the current installer is ready for redistribution.

The owner chose proprietary licensing for first-party code on 2026-10-27.
The root `LICENSE` records that choice. Third-party licenses remain unchanged.
The Rust manifests previously declared MIT; review historical grants before
describing older revisions as proprietary. A later notice cannot revoke rights
already granted. Confirm ownership/permission for any code copied from the
legacy project and any design assets.

## Required distribution inventory

| Component | Evidence currently available | Required before publishing binaries |
| --- | --- | --- |
| First-party Rust, Dart, Windows runner, assets | Owner identity in Cargo metadata; root proprietary notice | Confirm ownership and authorized user/distribution terms; align package metadata |
| Flutter engine, Dart runtime, ICU, plugins and fonts | SDK/package licenses; generated Flutter `NOTICES.Z` in build output | Inventory exact shipped versions, extract readable notices, include copyright texts and engine/ICU/font notices |
| Rust production dependencies, including transitive crates | `rust_core/Cargo.lock` and resolved Cargo metadata | Generate dependency SBOM and license inventory, include applicable texts/notices; examine exceptions, multiple-license choices and source obligations |
| Symphonia and other file-copyleft dependencies | Exact crate source and licenses in the Cargo cache | Review the exact license and any modified covered files; provide required source without assuming all application code must change license |
| ONNX Runtime and bundled native dependencies | CPU DLL in release output; upstream license sources | Identify exact native build/version, include its LICENSE and ThirdPartyNotices, match bytes to provenance |
| FFmpeg/FFprobe full static build and its linked libraries (including Rubber Band) | `packaging/ffmpeg-runtime.json`; bundled GPL text and provider README | Review complete build configuration, all enabled library licenses, corresponding source/build scripts/patches and delivery method; preserve provider notices |
| MSVC app-local runtime DLLs | DLLs copied by Windows build | Record producing VS edition/version and applicable REDIST list/terms; ship only files permitted for redistribution |
| Four optional model weights | Hashes, URLs and attribution in `assets/manifests/model_license_manifest.json` | Verify weight-specific permissions and exact notices before mirroring; a code license alone is not proof of a weights license |
| Audio test corpus and UI/design assets | Local user-provided inputs/reference design | Keep private unless redistribution permission is explicit; do not include test recordings in release assets |

An SPDX expression or URL alone does not replace a required full license text.
Do not rewrite upstream copyright notices. Record license-text provenance and
hashes, and distinguish build-only tools from software actually distributed.

## Required release assets

1. Windows x64 installer and a portable ZIP of the **entire** release folder.
2. `SHA256SUMS.txt` covering the exact uploaded files, excluding itself.
3. A readable third-party notice bundle with all applicable full texts, exact
   versions, provenance and a machine-readable SBOM.
4. Corresponding-source material required for distributed copyleft components,
   including relevant build configuration, patches and dependency sources.
   Establish a compliant delivery method before publication; merely linking an
   unrelated current upstream branch is insufficient evidence of matching source.
5. Release notes with CPU-only scope, qualifications, known limitations, source
   commit, toolchain versions, installer provenance and checksums.

The installer already contains native DLLs and audio tools, but the current
packaging script does **not** collect the complete notice/source bundle above.
Extend packaging and its validator to require these artifacts, then rebuild.
Do not label the existing installer license-complete or silently publish it as
the final qualified build. Preserve its hash as the initial candidate instead.

Keep installers, DLLs, model weights, SDKs, user audio and vendor caches out of
ordinary Git history. Publish authorized binary packages as GitHub Release
assets. SDK/build-tool installation is documented in the README; they do not
need to be republished as application dependencies. An optional offline model
or dependency package needs its own permissions, manifest and checksum review.

## Primary references

- [FFmpeg legal information](https://ffmpeg.org/legal.html)
- [GNU GPL version 3](https://www.gnu.org/licenses/gpl-3.0.html)
- [ONNX Runtime license](https://github.com/microsoft/onnxruntime/blob/main/LICENSE)
- [Microsoft Visual C++ redistribution terms](https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files?view=msvc-170)
- [GitHub large-file limits and release assets](https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github)

Resolve component-license compatibility before distributing the proprietary
application with these components. Separate-process FFmpeg use and native
library linking must be evaluated according to their actual integration;
neither blanket GPL relicensing nor automatic proprietary compatibility is
assumed here.
