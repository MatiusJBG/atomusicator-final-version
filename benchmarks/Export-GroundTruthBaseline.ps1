#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Benchmark,
    [Parameter(Mandatory)][string]$Resources,
    [Parameter(Mandatory)][string]$Preparation,
    [Parameter(Mandatory)][string]$Environment,
    [Parameter(Mandatory)][string]$Diagnosis,
    [string]$Output='benchmarks/results/BASELINE_GROUND_TRUTH.json'
)
$ErrorActionPreference='Stop'
function Read-Json([string]$Path){Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json}
$bench=Read-Json $Benchmark
$resource=Read-Json $Resources
if($resource.exit_code -ne 0){throw 'Cannot certify a failed benchmark run'}
$prepared=Read-Json $Preparation
$environmentInfo=Read-Json $Environment
if($bench.pipeline -ne 'BoundedSeparation/native-v1' -or @($bench.cases).Count -ne 1){throw 'One completed bounded baseline case is required'}
$case=$bench.cases[0]
if($case.source_sha256 -ne $prepared.mix.sha256 -or $case.source_sha256 -ne $environmentInfo.input.sha256 -or $case.frames -ne $prepared.frames){throw 'Input or alignment provenance mismatch'}
foreach($stem in @('lead_vocal','backing_vocals','drums','bass','instrumental')){
    if(-not $case.metrics.$stem -or $case.metrics.$stem.reference_sha256 -ne $prepared.references.$stem.sha256){throw "Missing or changed reference: $stem"}
}
$summary=@(foreach($case in $bench.cases){
    foreach($item in $case.metrics.PSObject.Properties){
        $channels=@($item.Value.metrics.channels)
        $si=@($channels | ForEach-Object {$_.si_sdr})
        $sdr=@($channels | ForEach-Object {$_.sdr})
        [ordered]@{
            case_id=$case.case_id;stem=$item.Name;engine_stem=$item.Value.engine_stem
            si_sdr_db_channel_mean=$(if(@($si|Where-Object {$_.kind -ne 'decibels'}).Count -eq 0){($si.db|Measure-Object -Average).Average}else{$null})
            sdr_db_channel_mean=$(if(@($sdr|Where-Object {$_.kind -ne 'decibels'}).Count -eq 0){($sdr.db|Measure-Object -Average).Average}else{$null})
            reference_rms=[Math]::Sqrt(($channels|ForEach-Object {$_.reference_rms* $_.reference_rms}|Measure-Object -Average).Average)
            estimate_rms=[Math]::Sqrt(($channels|ForEach-Object {$_.estimate_rms* $_.estimate_rms}|Measure-Object -Average).Average)
            rms_error=[Math]::Sqrt(($channels|ForEach-Object {$_.rms_error* $_.rms_error}|Measure-Object -Average).Average)
        }
    }
})
$result=[ordered]@{
    schema_version=1;created_utc=[DateTime]::UtcNow.ToString('o')
    qualification_scope='One user-supplied reference set; this is not five-song or release qualification'
    channel_summary='Arithmetic mean of channel dB scores; RMS combines channel energies; undefined/perfect scores stay explicit in raw metrics'
    summary=$summary;benchmark=$bench;resources=$resource
    preparation=$prepared;environment=$environmentInfo;diagnosis=(Read-Json $Diagnosis)
}
$parent=Split-Path $Output -Parent
if($parent){[void](New-Item -ItemType Directory -Force -Path $parent)}
$result|ConvertTo-Json -Depth 40|Set-Content -LiteralPath $Output -Encoding utf8
