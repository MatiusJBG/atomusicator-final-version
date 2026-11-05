#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Audio,
    [Parameter(Mandatory)][string]$ModelDirectory,
    [Parameter(Mandatory)][string]$FFmpeg,
    [Parameter(Mandatory)][string]$Report,
    [string]$Repository = (Split-Path $PSScriptRoot -Parent)
)
$ErrorActionPreference='Stop'
function Read-OptionalObservation([scriptblock]$Probe) {
    try { & $Probe } catch { [ordered]@{unavailable=$_.Exception.Message} }
}
$source=Get-Item -LiteralPath $Audio
$manifest=Get-Content -Raw -LiteralPath (Join-Path $Repository 'assets/manifests/model_license_manifest.json') | ConvertFrom-Json
$models=@(foreach($model in $manifest.models){
    $path=Join-Path $ModelDirectory $model.filename
    if(-not (Test-Path -LiteralPath $path -PathType Leaf)){throw "Missing model: $($model.filename)"}
    $hash=(Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
    if($hash -ne $model.sha256.ToLowerInvariant()){throw "Model hash mismatch: $($model.filename)"}
    [ordered]@{profile_id=$model.profile_id;filename=$model.filename;bytes=(Get-Item -LiteralPath $path).Length;sha256=$hash}
})
$cargo=Get-Content -Raw -LiteralPath (Join-Path $Repository 'rust_core/Cargo.toml')
$lock=Get-Content -Raw -LiteralPath (Join-Path $Repository 'rust_core/Cargo.lock')
$result=[ordered]@{
    schema_version=1
    captured_utc=[DateTime]::UtcNow.ToString('o')
    commit_sha=(& git -C $Repository rev-parse HEAD)
    working_tree_dirty=[bool](& git -C $Repository status --porcelain)
    app_version=[regex]::Match($cargo,'(?m)^version = "([^"]+)"').Groups[1].Value
    rust_version=Read-OptionalObservation { & rustc -Vv }
    ort_crate_version=[regex]::Match($lock,'(?s)name = "ort"\s+version = "([^"]+)"').Groups[1].Value
    ffmpeg_version=(& $FFmpeg -version | Select-Object -First 1)
    ffmpeg_sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath $FFmpeg).Hash.ToLowerInvariant()
    cpu=Read-OptionalObservation { @(Get-CimInstance Win32_Processor -Property Name,NumberOfCores,NumberOfLogicalProcessors | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors) }
    memory=Read-OptionalObservation { Get-CimInstance Win32_OperatingSystem -Property TotalVisibleMemorySize,FreePhysicalMemory | Select-Object TotalVisibleMemorySize,FreePhysicalMemory }
    gpu=Read-OptionalObservation { @(Get-CimInstance Win32_VideoController -Property Name,DriverVersion | Select-Object Name,DriverVersion) }
    power_scheme=Read-OptionalObservation { & powercfg /getactivescheme }
    input=[ordered]@{filename=$source.Name;bytes=$source.Length;sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath $source.FullName).Hash.ToLowerInvariant()}
    models=$models
    units='Win32_OperatingSystem memory values are KiB; file sizes are bytes'
}
$result | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $Report -Encoding utf8
