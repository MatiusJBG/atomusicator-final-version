#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$CpuReport,
    [Parameter(Mandatory)][string]$AcceleratorReport,
    [Parameter(Mandatory)][string]$WaveformComparison,
    [Parameter(Mandatory)][string]$WindowQualification,
    [Parameter(Mandatory)][string]$Output
)
$ErrorActionPreference='Stop'
function Read-Json([string]$Path){Get-Content -LiteralPath $Path -Raw|ConvertFrom-Json -AsHashtable}
function Number($Value,[string]$Name){
    if($null -eq $Value -or $Value -is [string] -or $Value -is [bool] -or -not [double]::IsFinite([double]$Value)){throw "Missing/nonfinite number: $Name"}
    [double]$Value
}
function Hash-Equal($First,$Second,[string]$Name){
    if($First -notmatch '^[a-f0-9]{64}$' -or $Second -ne $First){throw "Hash mismatch: $Name"}
}
$cpu=Read-Json $CpuReport; $gpu=Read-Json $AcceleratorReport
$pcm=Read-Json $WaveformComparison; $windows=Read-Json $WindowQualification
if(@($cpu.cases).Count -ne 1 -or @($gpu.cases).Count -ne 1){throw 'Exactly one completed case per provider is required'}
if($cpu.provider_requested -ne 'cpu' -or $gpu.provider_requested -ne 'directml'){throw 'Explicit CPU and DirectML reports required'}
if($cpu.pipeline -ne $gpu.pipeline -or $cpu.pipeline -notin @('BoundedSeparation/native-v1','BoundedSeparation/native-v2-original-mix-bass')){throw 'Quality pipeline mismatch'}
if(-not $cpu.quality_profile -or ($cpu.quality_profile|ConvertTo-Json -Depth 20 -Compress) -cne ($gpu.quality_profile|ConvertTo-Json -Depth 20 -Compress)){throw 'Quality descriptor mismatch'}
if($cpu.pipeline -ne "BoundedSeparation/$($cpu.quality_profile.id)"){throw 'Pipeline/profile identity mismatch'}
if(@($cpu.session_diagnostics).Count -eq 0 -or @($gpu.session_diagnostics).Count -eq 0 -or @($cpu.session_diagnostics|Where-Object provider_registered -ne 'Cpu').Count -or @($gpu.session_diagnostics|Where-Object provider_registered -ne 'DirectMl').Count){throw 'Provider registration missing or CPU fallback observed'}
$a=$cpu.cases[0]; $b=$gpu.cases[0]
Hash-Equal $a.source_sha256 $b.source_sha256 'input'
if((Number $a.frames 'frames') -le 0 -or $a.frames -ne $b.frames -or $a.frames -ne $pcm.frames -or $a.sample_rate -ne 44100 -or $b.sample_rate -ne 44100 -or $pcm.sample_rate_hz -ne 44100){throw 'Audio alignment mismatch'}
if(@($cpu.models).Count -ne 4 -or ($cpu.models|ConvertTo-Json -Depth 20 -Compress) -cne ($gpu.models|ConvertTo-Json -Depth 20 -Compress)){throw 'Model inventory mismatch'}
if(@($cpu.models.profile_id|Select-Object -Unique).Count -ne 4){throw 'Duplicate model identities'}
foreach($model in $cpu.models){
    $proof=@($windows.models|Where-Object profile_id -eq $model.profile_id)
    if($proof.Count -ne 1 -or $proof[0].node_and_window_parity_pass -ne $true -or (Number $proof[0].assignment.kernel_events.DmlExecutionProvider 'DirectML kernels') -le 0){throw "Actual-node window evidence missing: $($model.profile_id)"}
    Hash-Equal $model.sha256 $proof[0].model_sha256 $model.profile_id
}
$rows=@(foreach($stem in @('lead_vocal','backing_vocals','drums','bass','instrumental')){
    Hash-Equal $a.metrics[$stem].reference_sha256 $b.metrics[$stem].reference_sha256 "$stem reference"
    Hash-Equal $a.wav_sha256[$stem] $pcm.stems[$stem].baseline_sha256 "$stem CPU WAV"
    Hash-Equal $b.wav_sha256[$stem] $pcm.stems[$stem].sha256 "$stem DirectML WAV"
    $channels=@($a.metrics[$stem].metrics.channels); $other=@($b.metrics[$stem].metrics.channels)
    $difference=@($pcm.stems[$stem].difference_from_baseline_not_ground_truth.channels)
    if($channels.Count -ne 2 -or $other.Count -ne 2 -or $difference.Count -ne 2){throw "Stereo metrics missing: $stem"}
    $si=@(); $sdr=@(); $maxError=0.0
    for($channel=0;$channel -lt 2;$channel++){
        foreach($metric in @('si_sdr','sdr')){if($channels[$channel][$metric].kind -ne 'decibels' -or $other[$channel][$metric].kind -ne 'decibels'){throw "Undefined score requires separate review: $stem/$metric"}}
        $si += [Math]::Abs((Number $channels[$channel].si_sdr.db 'CPU SI-SDR')-(Number $other[$channel].si_sdr.db 'DirectML SI-SDR'))
        $sdr += [Math]::Abs((Number $channels[$channel].sdr.db 'CPU SDR')-(Number $other[$channel].sdr.db 'DirectML SDR'))
        $pcmError=Number $difference[$channel].max_abs_error 'PCM error'
        if($pcmError -lt 0){throw 'PCM error cannot be negative'}
        $maxError=[Math]::Max($maxError,$pcmError)
    }
    $maxSi=($si|Measure-Object -Maximum).Maximum
    @{stem=$stem;max_pcm_error=$maxError;max_channel_si_sdr_delta_db=$maxSi;max_channel_sdr_delta_db=($sdr|Measure-Object -Maximum).Maximum;passed=($maxError -le 1e-4 -and $maxSi -le 0.1)}
})
foreach($case in @($a,$b)){
    if(@($case.reconstruction.channels).Count -ne 2){throw 'Reconstruction metrics missing'}
    foreach($channel in $case.reconstruction.channels){$reconstructionError=Number $channel.max_abs_error 'reconstruction';if($reconstructionError -lt 0 -or $reconstructionError -gt 1e-5){throw 'Reconstruction tolerance exceeded'}}
}
$passed=@($rows|Where-Object passed -ne $true).Count -eq 0
$parent=Split-Path $Output -Parent
if($parent){New-Item -ItemType Directory -Force -Path $parent|Out-Null}
@{schema_version=1;scope='One reference set; numerical provider parity only, not corpus diversity, VRAM, batching, device soak or release certification';numerical_parity_pass=$passed;pipeline=$cpu.pipeline;input_sha256=$a.source_sha256;frames=$a.frames;thresholds=@{max_pcm_error=1e-4;max_channel_si_sdr_delta_db=0.1;reconstruction_error=1e-5};stems=$rows;cpu_report_sha256=(Get-FileHash $CpuReport).Hash;accelerator_report_sha256=(Get-FileHash $AcceleratorReport).Hash;waveform_comparison_sha256=(Get-FileHash $WaveformComparison).Hash;window_qualification_sha256=(Get-FileHash $WindowQualification).Hash}|ConvertTo-Json -Depth 10|Set-Content -LiteralPath $Output -Encoding utf8
if(-not $passed){throw 'Numerical provider parity failed; report saved'}
Write-Output "Numerical provider parity passed: $Output"
