[CmdletBinding()]
param(
    [switch]$SkipInstaller
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$releaseDirectory = Join-Path $repoRoot 'build\windows\x64\runner\Release'
$rustManifest = Join-Path $repoRoot 'rust_core\Cargo.toml'
$rustReleaseDirectory = $null
$installerDefinition = Join-Path $PSScriptRoot 'Atomusicator.iss'
$installerOutput = Join-Path $PSScriptRoot 'dist'

foreach ($tool in @('flutter', 'cargo')) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        throw "Required command '$tool' was not found on PATH."
    }
}

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

# ONNX Runtime and its optional execution-provider DLLs are emitted beside the
# Rust cdylib by ort's copy-dylibs feature. Copy only those known runtime files.
$runtimeLibraries = Get-ChildItem -LiteralPath $rustReleaseDirectory -File |
    Where-Object { $_.Name -match '^(onnxruntime(.*)?|DirectML)\.dll$' }
if (-not ($runtimeLibraries | Where-Object { $_.Name -eq 'onnxruntime.dll' })) {
    throw "onnxruntime.dll was not produced in $rustReleaseDirectory."
}
foreach ($library in $runtimeLibraries) {
    Copy-Item -LiteralPath $library.FullName -Destination $releaseDirectory -Force
}

if (-not $SkipInstaller) {
    $compiler = Get-Command 'ISCC.exe' -ErrorAction SilentlyContinue
    if (-not $compiler) {
        foreach ($candidate in @(
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
    & $compiler.Source "/DAppVersion=0.6.0" "/DReleaseDir=$releaseDirectory" "/DOutputDir=$installerOutput" $installerDefinition
    if ($LASTEXITCODE -ne 0) { throw 'Inno Setup compilation failed.' }
}

Write-Host "Windows release files: $releaseDirectory"
if (-not $SkipInstaller) { Write-Host "Installer output: $installerOutput" }
