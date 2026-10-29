#requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$SkipInstaller,
    [string]$FfmpegRuntimeDirectory,
    [string]$InnoCompiler
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$releaseDirectory = Join-Path $repoRoot 'build\windows\x64\runner\Release'
$rustManifest = Join-Path $repoRoot 'rust_core\Cargo.toml'
$rustReleaseDirectory = $null
$installerDefinition = Join-Path $PSScriptRoot 'Atomusicator.iss'
$installerOutput = Join-Path $PSScriptRoot 'dist'
$versionLine = Select-String -LiteralPath (Join-Path $repoRoot 'pubspec.yaml') -Pattern '^version:\s*([0-9]+\.[0-9]+\.[0-9]+)(?:\+\d+)?\s*$'
if (-not $versionLine) { throw 'Could not read the application version from pubspec.yaml.' }
$appVersion = $versionLine.Matches[0].Groups[1].Value

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    if (Test-Path -LiteralPath (Join-Path $cargoBin 'cargo.exe')) { $env:PATH = "$cargoBin;$env:PATH" }
}

foreach ($tool in @('flutter', 'cargo')) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        throw "Required command '$tool' was not found on PATH."
    }
}

& (Join-Path $PSScriptRoot 'prepare_audio_tools.ps1') -SourceDirectory $FfmpegRuntimeDirectory

Push-Location $repoRoot
try {
    $cargoMetadata = cargo metadata --manifest-path $rustManifest --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Could not read the Rust workspace metadata.' }
    $rustTargetDirectory = ($cargoMetadata | ConvertFrom-Json).target_directory
    $rustReleaseDirectory = Join-Path $rustTargetDirectory 'release'

    flutter pub get
    if ($LASTEXITCODE -ne 0) { throw 'flutter pub get failed.' }

    flutter build windows --release
    if ($LASTEXITCODE -ne 0) { throw 'Flutter Windows release build failed.' }

    cargo build --manifest-path $rustManifest -p atomusicator-bootstrap-ffi --release
    if ($LASTEXITCODE -ne 0) { throw 'Rust FFI release build failed.' }
}
finally {
    Pop-Location
}

$appExecutable = Join-Path $releaseDirectory 'atomusicator.exe'
if (-not (Test-Path -LiteralPath $appExecutable -PathType Leaf)) {
    throw "Flutter build output was not found: $appExecutable"
}

$bridgeLibrary = Join-Path $rustReleaseDirectory 'atomusicator_bootstrap_ffi.dll'
if (-not (Test-Path -LiteralPath $bridgeLibrary -PathType Leaf)) {
    throw "Rust FFI library was not found: $bridgeLibrary"
}
Copy-Item -LiteralPath $bridgeLibrary -Destination $releaseDirectory -Force
$audioTools = Join-Path $releaseDirectory 'audio-tools'
New-Item -ItemType Directory -Path $audioTools -Force | Out-Null
Copy-Item -Path (Join-Path $repoRoot 'build\ffmpeg-runtime\*') -Destination $audioTools -Recurse -Force

# This distribution qualifies CPU only. Do not ship unqualified GPU runtimes.
$runtimeLibraries = Get-ChildItem -LiteralPath $rustReleaseDirectory -File |
    Where-Object { $_.Name -in @('onnxruntime.dll', 'onnxruntime_providers_shared.dll') }
if (-not ($runtimeLibraries | Where-Object { $_.Name -eq 'onnxruntime.dll' })) {
    throw "onnxruntime.dll was not produced in $rustReleaseDirectory."
}
foreach ($library in $runtimeLibraries) {
    Copy-Item -LiteralPath $library.FullName -Destination $releaseDirectory -Force
}
foreach ($unqualified in @('onnxruntime_providers_cuda.dll', 'onnxruntime_providers_tensorrt.dll', 'onnxruntime_providers_dml.dll', 'DirectML.dll')) {
    $staleRuntime = Join-Path $releaseDirectory $unqualified
    if (Test-Path -LiteralPath $staleRuntime -PathType Leaf) {
        Remove-Item -LiteralPath $staleRuntime -Force
    }
}

foreach ($required in @('msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll')) {
    if (-not (Test-Path -LiteralPath (Join-Path $releaseDirectory $required) -PathType Leaf)) {
        throw "The app-local Visual C++ runtime is incomplete: $required. Check the Visual Studio redistributable installation."
    }
}

# Record exactly what is distributed, without including the manifest in itself.
$pitchVendor = Join-Path $repoRoot 'rust_core\crates\infrastructure\native\vendor'
$pitchLicenses = Join-Path $releaseDirectory 'licenses\signalsmith'
New-Item -ItemType Directory -Path $pitchLicenses -Force | Out-Null
foreach ($entry in @(
    @{source='STRETCH-LICENSE.txt'; target='STRETCH-LICENSE.txt'},
    @{source='signalsmith-linear\LICENSE.txt'; target='LINEAR-LICENSE.txt'},
    @{source='PROVENANCE.json'; target='PROVENANCE.json'}
)) {
    Copy-Item -LiteralPath (Join-Path $pitchVendor $entry.source) -Destination (Join-Path $pitchLicenses $entry.target) -Force
}
Copy-Item -LiteralPath (Join-Path $repoRoot 'THIRD_PARTY_NOTICES.md') -Destination $releaseDirectory -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'LICENSE') -Destination $releaseDirectory -Force

# Bundled helper EXEs can inherit a Low label from the build workspace too.
# Medium is normal user integrity, not administrator elevation. Keep directory
# permissions and the labels on TEMP/AppData untouched.
foreach ($name in @('atomusicator.exe', 'audio-tools\bin\ffmpeg.exe', 'audio-tools\bin\ffprobe.exe')) {
    $executable = Join-Path $releaseDirectory $name
    & "$env:SystemRoot\System32\icacls.exe" $executable /setintegritylevel M | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Could not set normal integrity on $executable." }
}

$releaseRoot = [System.IO.Path]::GetFullPath($releaseDirectory)
$manifest = Get-ChildItem -LiteralPath $releaseRoot -File -Recurse |
    Where-Object { $_.Name -ne 'release-manifest.json' } |
    Sort-Object FullName | ForEach-Object {
        [ordered]@{
            path = [System.IO.Path]::GetRelativePath($releaseRoot, $_.FullName)
            bytes = $_.Length
            sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
[ordered]@{ schema_version = 1; files = @($manifest) } |
    ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $releaseRoot 'release-manifest.json') -Encoding utf8

if (-not $SkipInstaller) {
    $compiler = $null
    if ($InnoCompiler) {
        $compiler = [pscustomobject]@{ Source = (Resolve-Path -LiteralPath $InnoCompiler).Path }
    } else {
        $compiler = Get-Command 'ISCC.exe' -ErrorAction SilentlyContinue
    }
    if (-not $compiler) {
        foreach ($candidate in @(
            (Join-Path $repoRoot 'build\toolchain\inno\ISCC.exe'),
            (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'),
            (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe')
        )) {
            if (Test-Path -LiteralPath $candidate -PathType Leaf) {
                $compiler = [pscustomobject]@{ Source = $candidate }
                break
            }
        }
    }
    if (-not $compiler) { throw 'Inno Setup 6 was not found. Install it or pass -SkipInstaller to prepare the release folder only.' }

    New-Item -ItemType Directory -Path $installerOutput -Force | Out-Null
    & $compiler.Source "/DAppVersion=$appVersion" "/DReleaseDir=$releaseDirectory" "/DOutputDir=$installerOutput" $installerDefinition
    if ($LASTEXITCODE -ne 0) { throw 'Inno Setup compilation failed.' }
    $installerExecutable = Join-Path $installerOutput "Atomusicator-Setup-$appVersion.exe"
    & "$env:SystemRoot\System32\icacls.exe" $installerExecutable /setintegritylevel M | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Could not set normal integrity on the generated installer.' }
}

Write-Host "Windows release files: $releaseDirectory"
if (-not $SkipInstaller) { Write-Host "Installer output: $installerOutput" }
