[CmdletBinding()]
param([string]$SourceDirectory)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$buildRoot = Join-Path $repoRoot 'build'
$destination = Join-Path $buildRoot 'ffmpeg-runtime'
$manifest = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'ffmpeg-runtime.json') | ConvertFrom-Json
$runtimeFiles = @('bin/ffmpeg.exe', 'bin/ffprobe.exe', 'LICENSE', 'README.txt')
New-Item -ItemType Directory -Path $buildRoot -Force | Out-Null

function Test-AudioRuntime([string]$Directory) {
    foreach ($name in $runtimeFiles) {
        if (-not (Test-Path -LiteralPath (Join-Path $Directory $name) -PathType Leaf)) { return $false }
    }
    foreach ($entry in $manifest.files.PSObject.Properties) {
        if ((Get-FileHash -LiteralPath (Join-Path $Directory $entry.Name) -Algorithm SHA256).Hash -ne $entry.Value) { return $false }
    }
    return $true
}

if ($SourceDirectory) {
    $source = (Resolve-Path -LiteralPath $SourceDirectory).Path
    if (-not (Test-AudioRuntime $source)) { throw 'Source runtime does not match the pinned FFmpeg manifest.' }
} elseif (Test-AudioRuntime $destination) {
    $source = $destination
} else {
    $archive = Join-Path $buildRoot ($manifest.archive_root + '.zip')
    if (-not (Test-Path -LiteralPath $archive) -or (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $manifest.archive_sha256) {
        Invoke-WebRequest -Uri $manifest.archive_url -OutFile $archive
    }
    if ((Get-Item -LiteralPath $archive).Length -ne $manifest.archive_size -or (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $manifest.archive_sha256) { throw 'FFmpeg archive failed size/SHA-256 verification.' }
    $source = Join-Path $buildRoot ('ffmpeg-staging-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $source -Force | Out-Null
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        # Extract only these exact trusted names, never an arbitrary archive path.
        foreach ($name in $runtimeFiles) {
            $entry = $zip.GetEntry($manifest.archive_root + '/' + $name)
            if (-not $entry -or $entry.Length -gt 536870912) { throw "Missing or oversized archive entry: $name" }
            $target = Join-Path $source $name
            New-Item -ItemType Directory -Path (Split-Path $target) -Force | Out-Null
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $target, $false)
        }
    } finally { $zip.Dispose() }
    if (-not (Test-AudioRuntime $source)) { throw 'Extracted FFmpeg files failed SHA-256 verification.' }
}
if ($source -ne $destination) {
    foreach ($name in $runtimeFiles) {
        $target = Join-Path $destination $name
        New-Item -ItemType Directory -Path (Split-Path $target) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $source $name) -Destination $target -Force
    }
}
$tool = Join-Path $destination 'bin/ffmpeg.exe'
$filters = & $tool -hide_banner -filters 2>&1 | Out-String
if ($LASTEXITCODE -ne 0 -or $filters -notmatch 'rubberband') { throw 'FFmpeg is missing the Rubber Band pitch filter.' }
$encoders = & $tool -hide_banner -encoders 2>&1 | Out-String
if ($LASTEXITCODE -ne 0 -or $encoders -notmatch 'libmp3lame') { throw 'FFmpeg is missing the MP3 encoder.' }
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'ffmpeg-runtime.json') -Destination (Join-Path $destination 'runtime-manifest.json') -Force
Write-Host "Verified audio tools: $destination"
