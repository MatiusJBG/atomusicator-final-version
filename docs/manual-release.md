# Manual push and first Windows release

The owner performs all remote mutations. These instructions do not publish
anything automatically. Implementation plans and private test evidence stay
in ignored folders. The release remains blocked until the validation and
[distribution-compliance](distribution-compliance.md) gates are closed.

## 1. Review and commit locally

From the repository root in PowerShell 7:

```powershell
git switch feature/complete-python-migration
git status --short
git diff --check
git diff
git log -5 --oneline
git check-ignore implementation-plans/validation-release-closure.md
```

Stage explicit reviewed paths, not `git add .`. Keep documentation, licensing,
packaging and validation fixes in separate coherent commits. For the prepared
documentation/license changes:

```powershell
git add LICENSE rust_core/Cargo.toml rust_core/crates/domain/Cargo.toml rust_core/crates/application/Cargo.toml rust_core/crates/infrastructure/Cargo.toml rust_core/crates/bootstrap_ffi/Cargo.toml
git diff --cached
git commit -m "chore(license): record proprietary first-party licensing"
git add README.md docs/distribution-compliance.md docs/manual-release.md
git diff --cached
git commit -m "docs: document Windows setup and release qualification"
```

Run the checks documented in the README after implementation changes. Do not
include `implementation-plans`, media, sessions, downloaded SDKs, DLLs or model
weights in these commits. Check staged large files and secrets before pushing.

## 2. Push the feature branch yourself

```powershell
git fetch origin
git log --oneline origin/feature/complete-python-migration..HEAD
git diff --stat origin/feature/complete-python-migration...HEAD
git push --dry-run origin feature/complete-python-migration
git push origin feature/complete-python-migration
```

If rejected, inspect divergence and resolve it locally; do not force-push.
There is no automatic merge or PR creation in this workflow.

## 3. Freeze the qualified build

After every release gate passes, build from a clean worktree at the intended
release commit and record `git rev-parse HEAD`. Rebuild if code, notices,
packaging or bundled dependencies change. Re-run package and clean-Windows
qualification on the exact installer that will be uploaded.

Use version `0.6.0` only if it is still unused. Before tagging:

```powershell
git status --short
git tag --list v0.6.0
git ls-remote --tags origin refs/tags/v0.6.0
```

If the tag already exists, choose a new version and update the Flutter version,
Cargo workspace version, installer definition/build parameter and documentation
together. Do not overwrite an existing release or move a published tag.

Only after confirming the tree is clean, gates passed and version is available:

```powershell
git tag -a v0.6.0 -m "Atomusicator 0.6.0 Windows CPU release"
git push origin refs/tags/v0.6.0
```

Tag the commit actually used for the final build. The current installer is the
initial candidate; it must be rebuilt if mandatory licenses are added later.

## 4. Upload packages in GitHub Releases

Open [repository releases](https://github.com/MatiusJBG/atomusicator-final-version/releases),
choose **Draft a new release**, and select the verified tag. Upload the installer,
portable ZIP, checksums, third-party notice/SBOM bundle and required corresponding
source archives. Save as a draft until assets and download checks are verified.

Use the browser asset uploader or GitHub CLI; `git push` uploads Git objects,
not installer release assets. The installer is larger than GitHub's 100 MiB
ordinary-Git limit, so do not remove its ignore rule or force-add it.

Release notes must list the exact commit, supported Windows/CPU configuration,
setup/use instructions, required disk space measured during qualification,
license information and remaining limitations. If any qualification is still
open, do not present it as a fully validated stable release; a preview requires
explicit owner approval and clearly documented limits.

Download the draft assets with an authorized account and verify checksums.
Publish manually only when the release checklist has signed results. Then test
the published download on the clean Windows machine as well.

## Collaborators and attribution

Only invite a real, explicitly identified GitHub account through repository
settings. Codex has no personal GitHub account that can accept collaborator
access. Do not invite an account merely named `codex` or `openai`, or grant a
service account access without identifying its owner. AI assistance can be
acknowledged in release notes without granting repository permissions.
