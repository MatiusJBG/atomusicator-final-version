[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ReleaseDirectory,
    [switch]$Launch,
    [string]$Report
)
$ErrorActionPreference = 'Stop'
$releaseRoot = (Resolve-Path -LiteralPath $ReleaseDirectory).Path
$manifest = Get-Content -LiteralPath (Join-Path $releaseRoot 'release-manifest.json') -Raw | ConvertFrom-Json
if ($manifest.schema_version -ne 1) { throw 'Unsupported release manifest.' }
foreach ($entry in $manifest.files) {
    $path = [IO.Path]::GetFullPath((Join-Path $releaseRoot $entry.path))
    if (-not $path.StartsWith($releaseRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Manifest path escapes the release: $($entry.path)"
    }
    if ((Get-Item -LiteralPath $path).Length -ne $entry.bytes -or
        (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $entry.sha256) {
        throw "Release integrity failure: $($entry.path)"
    }
}
foreach ($required in @('atomusicator.exe', 'atomusicator_bootstrap_ffi.dll', 'flutter_windows.dll',
    'onnxruntime.dll', 'msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll',
    'data\icudtl.dat', 'data\app.so', 'audio-tools\bin\ffmpeg.exe',
    'audio-tools\bin\ffprobe.exe', 'audio-tools\LICENSE', 'audio-tools\runtime-manifest.json')) {
    if (-not (Test-Path -LiteralPath (Join-Path $releaseRoot $required) -PathType Leaf)) { throw "Missing runtime file: $required" }
}
if (Get-ChildItem -LiteralPath $releaseRoot -Recurse -File | Where-Object { $_.Extension -in @('.py', '.pyc', '.pyd') -or $_.Name -match '^python.*\.(exe|dll)$' }) {
    throw 'Unexpected Python runtime in the native distribution.'
}
$oldPath = $env:PATH
$oldFfmpeg = $env:ATOMUSICATOR_FFMPEG
$process = $null
try {
    # Developer toolchains and model/repository folders cannot satisfy DLL lookup here.
    $env:PATH = "$env:SystemRoot\System32;$env:SystemRoot"
    $env:ATOMUSICATOR_FFMPEG = $null
    $ffmpeg = & (Join-Path $releaseRoot 'audio-tools\bin\ffmpeg.exe') -hide_banner -filters 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0 -or $ffmpeg -notmatch '\brubberband\b') { throw 'Bundled FFmpeg lacks the pitch filter.' }
    if ($Launch) {
        $process = Start-Process -FilePath (Join-Path $releaseRoot 'atomusicator.exe') -WorkingDirectory $releaseRoot -WindowStyle Hidden -PassThru
        Start-Sleep -Seconds 5
        $process.Refresh()
        if ($process.HasExited) { throw "Desktop startup failed, exit code $($process.ExitCode)." }
    }
    $result = [ordered]@{
        schema_version = 1
        verified_files = @($manifest.files).Count
        reduced_path_startup = [bool]$Launch
        bundled_rubberband = $true
        clean_windows_qualification = $false
        explanation = 'Host smoke test; clean Windows and physical audio require separate evidence.'
    }
    if ($Report) { $result | ConvertTo-Json | Set-Content -LiteralPath $Report -Encoding utf8 }
    $result | ConvertTo-Json
}
finally {
    if ($process -and -not $process.HasExited) {
        $null = $process.CloseMainWindow()
        if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
    }
    $env:PATH = $oldPath
    $env:ATOMUSICATOR_FFMPEG = $oldFfmpeg
}
