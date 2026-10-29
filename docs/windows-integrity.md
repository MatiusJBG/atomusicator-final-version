# Windows development: TEMP and AppData access denied

## Confirmed cause

On the affected development machine the repository directory has an inheritable
Low mandatory integrity label. Generated executables inherited it, including
the Debug/Release Flutter runner and local Rust examples. The normal user has
full discretionary access to TEMP and AppData, but that does not override
Windows mandatory integrity control.

Windows starts a process at the lower of the launching user's integrity and the
executable image's integrity. Low processes cannot write to normal Medium
directories. This behavior is documented in Microsoft's
[Mandatory Integrity Control](https://learn.microsoft.com/en-us/windows/win32/secauthz/mandatory-integrity-control).

Consequently the same cause blocked both Dart VM's `_createDevFS` temporary
directory and native workspace writes. Merely changing the workspace fallback
could not fix it. Tests hosted by the SDK's normally labeled `flutter_tester`
did not reproduce the application's inherited execution label.

## Correction and scope

- The Windows runner's CMake post-build step applies the normal **Medium**
  integrity label to the generated application executable.
- Packaging applies the same label to bundled executable helpers and the new
  installer. `-PrepareDebug` also copies matching native DLLs and pinned audio
  tools into the debug runner directory and labels its executable helpers.
- Only generated executable file labels are changed. User-data, TEMP, source
  directory permissions, discretionary ACLs and Windows protection settings
  remain unchanged. No administrator execution or High-integrity label is used.
- A process already running at Low integrity cannot gain privileges from this
  build step; labeling failure stops the build. Run development builds from the
  normal user session.

From a VS Code PowerShell 7 terminal at the repository root, with the app stopped:

```powershell
./packaging/build_windows.ps1 -SkipInstaller -PrepareDebug
flutter run -d windows
```

Run the preparation again after Rust/runtime changes. Dart hot reload does not
rebuild Rust. Normal incremental Flutter builds repeat the runner's label step
when linking a new executable. A previously built installer is not regenerated
by `-SkipInstaller`.

## Verification

A native diagnostic initially reported Low process integrity (`0x1000`) and
access denied writing to TEMP, native AppData and the legacy workspace. Changing
only that diagnostic executable's label to Medium changed its process integrity
to `0x2000`; all three write probes then passed. Original files were untouched.

The corrected Rust desktop check opened the authorized local five-stem musical
session and passed muted playback, live pitch, pause, seek and EOF. The rebuilt
Flutter Debug runner completed DevFS synchronization against the original
system TEMP and hot reload (202 ms), without redirecting TEMP or changing its
permissions. The prepared Debug FFmpeg decoded one second of the authorized
local FLAC into the original system TEMP (176,644-byte WAV); the temporary output
was removed. Seven import/native-bridge tests passed against the Debug runtime,
and the release passed 40-file integrity verification and reduced-PATH startup.
These are development-host checks, not clean-Windows qualification.

To inspect an output label without changing it:

```powershell
icacls ./build/windows/x64/runner/Debug/atomusicator.exe
```

The expected executable label is `Medium Mandatory Level`. Names may be localized
by Windows. Detailed diagnostic logs and fixtures stay in ignored build/audio
evidence directories.
