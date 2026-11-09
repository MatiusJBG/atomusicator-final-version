#requires -Version 7.0
# Local arm64 APK build. No GitHub Actions.
# Pinned Android FFmpeg is optional. If packaging/ffmpeg-android.json exists and
# names a local file whose SHA-256 matches, it is copied beside the bridge as
# libffmpeg.so. This script never downloads FFmpeg.
# Expected path when that manifest is absent:
#   android/app/src/main/jniLibs/arm64-v8a/libffmpeg.so
# The engine also looks for audio-tools/bin/ffmpeg and ffmpeg next to
# libatomusicator_bootstrap_ffi.so (the directory parsed from /proc/self/maps).
# Without that binary, Symphonia decodes WAV/FLAC/MP3/M4A and WAV export stays
# in-process. FLAC and MP3 export then return a clear error.
[CmdletBinding()]
param(
    [string]$FfmpegRuntimeDirectory,
    [switch]$SkipFlutter
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$jniDir = Join-Path $repoRoot 'android\app\src\main\jniLibs\arm64-v8a'
$targetTriple = 'aarch64-linux-android'
$missing = @()

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    if (Test-Path -LiteralPath (Join-Path $cargoBin 'cargo.exe')) { $env:PATH = "$cargoBin;$env:PATH" }
}
if (-not (Get-Command flutter -ErrorAction SilentlyContinue)) { $missing += 'flutter (not on PATH)' }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { $missing += 'cargo (not on PATH)' }
if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) { $missing += 'rustup (not on PATH)' }

$sdk = $env:ANDROID_HOME
if (-not $sdk) { $sdk = $env:ANDROID_SDK_ROOT }
if (-not $sdk) { $sdk = Join-Path $env:LOCALAPPDATA 'Android\Sdk' }
if (-not (Test-Path -LiteralPath $sdk)) { $missing += "Android SDK (ANDROID_HOME / ANDROID_SDK_ROOT, looked at $sdk)" }

$ndk = $env:ANDROID_NDK_HOME
if (-not $ndk -and (Test-Path -LiteralPath $sdk)) {
    $ndkRoot = Join-Path $sdk 'ndk'
    if (Test-Path -LiteralPath $ndkRoot) {
        $ndk = Get-ChildItem -LiteralPath $ndkRoot -Directory | Sort-Object Name -Descending | Select-Object -First 1 -ExpandProperty FullName
    }
}
if (-not $ndk -or -not (Test-Path -LiteralPath $ndk)) { $missing += 'Android NDK (ANDROID_NDK_HOME or <SDK>\ndk\<version>)' }

$installedTargets = @()
if (Get-Command rustup -ErrorAction SilentlyContinue) {
    $installedTargets = @(rustup target list --installed)
    if ($installedTargets -notcontains $targetTriple) {
        $missing += "Rust target $targetTriple (install with: rustup target add $targetTriple)"
    }
}

if ($missing.Count -gt 0) {
    throw ("Android release prerequisites are missing:`n- " + ($missing -join "`n- "))
}

$prebuilt = Join-Path $ndk 'toolchains\llvm\prebuilt'
$hostDir = if ($IsWindows) { 'windows-x86_64' } elseif ($IsMacOS) { 'darwin-x86_64' } else { 'linux-x86_64' }
$bin = Join-Path $prebuilt "$hostDir\bin"
$api = 26
$clang = Join-Path $bin "aarch64-linux-android$api-clang.cmd"
if (-not (Test-Path -LiteralPath $clang)) {
    $clang = Join-Path $bin "aarch64-linux-android$api-clang"
}
$clangxx = Join-Path $bin "aarch64-linux-android$api-clang++.cmd"
if (-not (Test-Path -LiteralPath $clangxx)) {
    $clangxx = Join-Path $bin "aarch64-linux-android$api-clang++"
}
$ar = Join-Path $bin 'llvm-ar.exe'
if (-not (Test-Path -LiteralPath $ar)) { $ar = Join-Path $bin 'llvm-ar' }
if (-not (Test-Path -LiteralPath $clang)) { throw "NDK clang for API $api was not found under $bin" }
if (-not (Test-Path -LiteralPath $clangxx)) { throw "NDK clang++ for API $api was not found under $bin" }
if (-not (Test-Path -LiteralPath $ar)) { throw "NDK llvm-ar was not found under $bin" }

$vcvars = 'C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat'
if (-not (Test-Path -LiteralPath $vcvars)) {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path -LiteralPath $vswhere) {
        $vsRoot = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($vsRoot) { $vcvars = Join-Path $vsRoot 'VC\Auxiliary\Build\vcvars64.bat' }
    }
}
if (Test-Path -LiteralPath $vcvars) {
    cmd /c "call `"$vcvars`" && set" | ForEach-Object {
        if ($_ -match '^(.*?)=(.*)$') { Set-Item -Path "Env:$($Matches[1])" -Value $Matches[2] }
    }
} else {
    Write-Warning "vcvars64.bat was not found. Host C crates such as ring may fail to compile while cross-building."
}
$gitPerl = 'C:\Program Files\Git\usr\bin'
if (-not (Get-Command perl -ErrorAction SilentlyContinue) -and (Test-Path -LiteralPath (Join-Path $gitPerl 'perl.exe'))) {
    # Append to PATH rather than prepend to avoid shadowing MSVC link.exe with Git's /usr/bin/link.exe
    $env:PATH = "$env:PATH;$gitPerl"
}
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path -LiteralPath (Join-Path $cargoBin 'cargo.exe')) {
    $env:PATH = "$cargoBin;$env:PATH"
}

$env:ANDROID_NDK_HOME = $ndk
$env:CC_aarch64_linux_android = $clang
$env:CXX_aarch64_linux_android = $clangxx
$env:AR_aarch64_linux_android = $ar
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = $clang
$buildTemp = Join-Path $repoRoot 'build\android-tmp'
New-Item -ItemType Directory -Force -Path $buildTemp | Out-Null
$env:TEMP = $buildTemp
$env:TMP = $buildTemp
$env:TMPDIR = $buildTemp
# Keep compiler command lines short enough for clang/cl on Windows.
$env:CARGO_TARGET_DIR = Join-Path $env:SystemDrive 'atmus\atomusicator-android'

New-Item -ItemType Directory -Path $jniDir -Force | Out-Null
$onnxManifest = Join-Path $PSScriptRoot 'onnxruntime-android.json'
$onnxTarget = Join-Path $jniDir 'libonnxruntime.so'
if (-not (Test-Path -LiteralPath $onnxTarget -PathType Leaf)) {
    $cachedOnnx = Join-Path $repoRoot 'build\jni\arm64-v8a\libonnxruntime.so'
    if (Test-Path -LiteralPath $cachedOnnx -PathType Leaf) {
        Copy-Item -LiteralPath $cachedOnnx -Destination $onnxTarget -Force
    }
}
if (-not (Test-Path -LiteralPath $onnxTarget -PathType Leaf)) {
    throw "libonnxruntime.so was not found in $jniDir. Release APK packaging aborted."
}
if (Test-Path -LiteralPath $onnxManifest) {
    $spec = Get-Content -LiteralPath $onnxManifest -Raw | ConvertFrom-Json
    $expected = $null
    if ($spec.files.PSObject.Properties['libonnxruntime.so']) {
        $expected = [string]$spec.files.'libonnxruntime.so'
    } elseif ($spec.files.PSObject.Properties['lib/arm64-v8a/libonnxruntime.so']) {
        $expected = [string]$spec.files.'lib/arm64-v8a/libonnxruntime.so'
    }
    if ($expected) {
        $expected = $expected.ToLowerInvariant()
        $actual = (Get-FileHash -LiteralPath $onnxTarget -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $expected) {
            throw "SHA-256 mismatch for libonnxruntime.so. Expected $expected, got $actual."
        }
        Write-Host "Verified libonnxruntime.so SHA-256 ($actual) against $onnxManifest."
    }
}
$env:ORT_LIB_LOCATION = $jniDir
$env:ORT_PREFER_DYNAMIC_LINK = '1'

Push-Location $repoRoot
try {
    flutter pub get
    if ($LASTEXITCODE -ne 0) { throw 'flutter pub get failed.' }
    cargo build --manifest-path (Join-Path $repoRoot 'rust_core\Cargo.toml') -p atomusicator-bootstrap-ffi --release --target $targetTriple
    if ($LASTEXITCODE -ne 0) { throw 'Rust Android FFI release build failed.' }
}
finally {
    Pop-Location
}

$rustOut = if ($env:CARGO_TARGET_DIR -and (Test-Path -LiteralPath (Join-Path $env:CARGO_TARGET_DIR "$targetTriple\release"))) {
    Join-Path $env:CARGO_TARGET_DIR "$targetTriple\release"
} elseif (Test-Path -LiteralPath (Join-Path $repoRoot "rust_core\target\$targetTriple\release")) {
    Join-Path $repoRoot "rust_core\target\$targetTriple\release"
} else {
    $meta = cargo metadata --manifest-path (Join-Path $repoRoot 'rust_core\Cargo.toml') --no-deps --format-version 1 | ConvertFrom-Json
    Join-Path $meta.target_directory "$targetTriple\release"
}
$bridge = Join-Path $rustOut 'libatomusicator_bootstrap_ffi.so'
if (-not (Test-Path -LiteralPath $bridge -PathType Leaf)) {
    throw "Rust FFI library was not found: $bridge"
}
New-Item -ItemType Directory -Path $jniDir -Force | Out-Null
Copy-Item -LiteralPath $bridge -Destination (Join-Path $jniDir 'libatomusicator_bootstrap_ffi.so') -Force

$sysrootLib = Join-Path $prebuilt "$hostDir\sysroot\usr\lib\$targetTriple"
$cxxShared = Join-Path $sysrootLib 'libc++_shared.so'
if (-not (Test-Path -LiteralPath $cxxShared -PathType Leaf)) {
    $cxxShared = Get-ChildItem -LiteralPath $ndk -Filter 'libc++_shared.so' -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -like "*$targetTriple*" } |
        Select-Object -First 1 -ExpandProperty FullName
}
if (-not $cxxShared -or -not (Test-Path -LiteralPath $cxxShared -PathType Leaf)) {
    throw "libc++_shared.so for $targetTriple was not found in NDK ($ndk)."
}
Copy-Item -LiteralPath $cxxShared -Destination (Join-Path $jniDir 'libc++_shared.so') -Force

$onnx = Get-ChildItem -LiteralPath $rustOut -Filter 'libonnxruntime.so' -File -ErrorAction SilentlyContinue | Select-Object -First 1
if ($onnx) {
    Copy-Item -LiteralPath $onnx.FullName -Destination (Join-Path $jniDir 'libonnxruntime.so') -Force
}

if (-not $onnx -and -not (Test-Path (Join-Path $jniDir 'libonnxruntime.so'))) {
    throw "libonnxruntime.so was not found in $rustOut or $jniDir. Release APK packaging aborted."
}

$onnxManifest = Join-Path $PSScriptRoot 'onnxruntime-android.json'
if (Test-Path -LiteralPath $onnxManifest) {
    $spec = Get-Content -LiteralPath $onnxManifest -Raw | ConvertFrom-Json
    $expected = $null
    if ($spec.files.PSObject.Properties['libonnxruntime.so']) {
        $expected = [string]$spec.files.'libonnxruntime.so'
    } elseif ($spec.files.PSObject.Properties['lib/arm64-v8a/libonnxruntime.so']) {
        $expected = [string]$spec.files.'lib/arm64-v8a/libonnxruntime.so'
    }
    if ($expected) {
        $expected = $expected.ToLowerInvariant()
        $onnxFile = Join-Path $jniDir 'libonnxruntime.so'
        $actual = (Get-FileHash -LiteralPath $onnxFile -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $expected) {
            throw "SHA-256 mismatch for libonnxruntime.so. Expected $expected, got $actual."
        }
        Write-Host "Verified libonnxruntime.so SHA-256 ($actual) against $onnxManifest."
    }
}

$ffmpegManifest = Join-Path $PSScriptRoot 'ffmpeg-android.json'
if (Test-Path -LiteralPath $ffmpegManifest) {
    $spec = Get-Content -LiteralPath $ffmpegManifest -Raw | ConvertFrom-Json
    if (-not $FfmpegRuntimeDirectory) {
        throw "packaging/ffmpeg-android.json is present but -FfmpegRuntimeDirectory was not given. Refusing to download FFmpeg."
    }
    foreach ($property in $spec.files.PSObject.Properties) {
        $source = Join-Path $FfmpegRuntimeDirectory ($property.Name -replace '/', '\')
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "Pinned Android FFmpeg file is missing: $source"
        }
        $hash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
        $expected = ([string]$property.Value).ToLowerInvariant()
        if ($hash -ne $expected) {
            throw "SHA-256 mismatch for $($property.Name). Expected $expected, got $hash."
        }
        $leaf = Split-Path -Leaf $property.Name
        if ($leaf -eq 'ffmpeg') { $leaf = 'libffmpeg.so' }
        Copy-Item -LiteralPath $source -Destination (Join-Path $jniDir $leaf) -Force
    }
} else {
    Write-Host "No packaging/ffmpeg-android.json. Symphonia decodes audio and WAV export stays in-process."
    Write-Host "To enable FLAC/MP3 export and loudnorm, place a hash-pinned binary at:"
    Write-Host "  $(Join-Path $jniDir 'libffmpeg.so')"
}

if (-not $SkipFlutter) {
    $assetsModelsDir = Join-Path $repoRoot 'android\app\src\main\assets\models'
    New-Item -ItemType Directory -Path $assetsModelsDir -Force | Out-Null
    $localModelsDir = Join-Path $env:LOCALAPPDATA 'Atomusicator\models'
    $modelFiles = @(
        'UVR_MDXNET_Main.onnx',
        'UVR_MDXNET_KARA_2.onnx',
        'kuielab_a_drums.onnx',
        'kuielab_a_bass.onnx'
    )
    foreach ($modelName in $modelFiles) {
        $destModel = Join-Path $assetsModelsDir $modelName
        if (-not (Test-Path -LiteralPath $destModel -PathType Leaf)) {
            $srcModel = Join-Path $localModelsDir $modelName
            if (Test-Path -LiteralPath $srcModel -PathType Leaf) {
                Copy-Item -LiteralPath $srcModel -Destination $destModel -Force
                Write-Host "Staged verified ONNX model into Android assets: $modelName"
            }
        }
    }

    Push-Location $repoRoot
    try {
        flutter build apk --release --target-platform android-arm64
        if ($LASTEXITCODE -ne 0) { throw 'flutter build apk failed.' }
    }
    finally {
        Pop-Location
    }

    $apkPath = Join-Path $repoRoot 'build\app\outputs\flutter-apk\app-release.apk'
    if (-not (Test-Path -LiteralPath $apkPath -PathType Leaf)) {
        throw "Release APK was not generated at: $apkPath"
    }
    Write-Host "Inspecting release APK archive: $apkPath"
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($apkPath)
    try {
        $entryNames = @($zip.Entries | ForEach-Object { $_.FullName })
        $requiredEntries = @(
            'lib/arm64-v8a/libc++_shared.so',
            'lib/arm64-v8a/libatomusicator_bootstrap_ffi.so',
            'lib/arm64-v8a/libonnxruntime.so',
            'assets/models/UVR_MDXNET_Main.onnx',
            'assets/models/UVR_MDXNET_KARA_2.onnx',
            'assets/models/kuielab_a_drums.onnx',
            'assets/models/kuielab_a_bass.onnx'
        )
        $missingInApk = @()
        foreach ($req in $requiredEntries) {
            if ($entryNames -notcontains $req) {
                $missingInApk += $req
            } else {
                Write-Host "  [OK] Found in APK: $req"
            }
        }
        if ($missingInApk.Count -gt 0) {
            throw "APK archive is missing required native libraries:`n- " + ($missingInApk -join "`n- ")
        }
    }
    finally {
        $zip.Dispose()
    }

    # Verify signature if apksigner is available
    $buildTools = Join-Path $sdk 'build-tools'
    if (Test-Path -LiteralPath $buildTools) {
        $apksigner = Get-ChildItem -LiteralPath $buildTools -Filter 'apksigner.bat' -Recurse -File -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName
        if ($apksigner) {
            Write-Host "Verifying release APK signature with apksigner..."
            & $apksigner verify --verbose $apkPath
            if ($LASTEXITCODE -eq 0) {
                Write-Host "  [OK] APK signature verified successfully."
            }
        }
    }

    Write-Host "Release APK packaging and verification completed successfully: $apkPath"
}
