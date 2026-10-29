#requires -Version 7.0
$ErrorActionPreference='Stop'
$root=Join-Path ([IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))) "build/parity-fixture-$([Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory $root | Out-Null
$script=Join-Path $PSScriptRoot '../Compare-ProviderReports.ps1'
$stems=@('lead_vocal','backing_vocals','drums','bass','instrumental')
$metrics=@{}; $cpuHashes=@{}; $gpuHashes=@{}; $pcm=@{}
foreach($stem in $stems){
    $metrics[$stem]=@{reference_sha256=('d'*64);metrics=@{channels=@(
        @{si_sdr=@{kind='decibels';db=3.0};sdr=@{kind='decibels';db=4.0}},
        @{si_sdr=@{kind='decibels';db=3.0};sdr=@{kind='decibels';db=4.0}}
    )}}
    $cpuHashes[$stem]='a'*64; $gpuHashes[$stem]='b'*64
    $pcm[$stem]=@{sha256=('b'*64);baseline_sha256=('a'*64);difference_from_baseline_not_ground_truth=@{channels=@(@{max_abs_error=1e-5},@{max_abs_error=1e-5})}}
}
$models=@(foreach($id in @('main','kara','drums','bass')){@{profile_id=$id;sha256=('e'*64)}})
$case=@{source_sha256=('c'*64);frames=1000;sample_rate=44100;metrics=$metrics;wav_sha256=$cpuHashes;reconstruction=@{channels=@(@{max_abs_error=1e-7},@{max_abs_error=1e-7})}}
$cpu=@{pipeline='BoundedSeparation/native-v2-original-mix-bass';quality_profile=@{id='native-v2-original-mix-bass'};provider_requested='cpu';models=$models;cases=@($case);session_diagnostics=@(@{provider_registered='Cpu'})}
function Save([string]$Name,$Value){$Value|ConvertTo-Json -Depth 20|Set-Content (Join-Path $root "$Name.json")}
Save 'cpu' $cpu
$gpu=($cpu|ConvertTo-Json -Depth 20|ConvertFrom-Json -AsHashtable)
$gpu.provider_requested='directml'; $gpu.session_diagnostics[0].provider_registered='DirectMl'; $gpu.cases[0].wav_sha256=$gpuHashes
Save 'gpu' $gpu
$comparison=@{frames=1000;sample_rate_hz=44100;stems=$pcm}; Save 'comparison' $comparison
$windows=@{models=@(foreach($model in $models){@{profile_id=$model.profile_id;model_sha256=$model.sha256;node_and_window_parity_pass=$true;assignment=@{kernel_events=@{DmlExecutionProvider=112}}}})}
Save 'windows' $windows
function Run { & $script -CpuReport "$root/cpu.json" -AcceleratorReport "$root/gpu.json" -WaveformComparison "$root/comparison.json" -WindowQualification "$root/windows.json" -Output "$root/result.json" }
function Expect-Failure([scriptblock]$Action){$failed=$false;try{& $Action}catch{$failed=$true};if(-not $failed){throw 'Invalid evidence was accepted'}}
Run
if(-not (Get-Content "$root/result.json" -Raw|ConvertFrom-Json).numerical_parity_pass){throw 'Valid fixture did not pass'}
& (Get-Command pwsh).Source -NoProfile -File $script -CpuReport "$root/cpu.json" -AcceleratorReport "$root/gpu.json" -WaveformComparison "$root/comparison.json" -WindowQualification "$root/windows.json" -Output "$root/cli-result.json"
if($LASTEXITCODE -ne 0){throw 'CLI invocation failed'}
$gpu.cases[0].metrics.bass.metrics.channels[0].si_sdr.db=4.0; Save 'gpu' $gpu
Expect-Failure {Run}
$gpu.cases[0].metrics.bass.metrics.channels[0].si_sdr.db=3.0; Save 'gpu' $gpu
$comparison.stems.drums.baseline_sha256='f'*64; Save 'comparison' $comparison
Expect-Failure {Run}
$comparison.stems.drums.baseline_sha256='a'*64; Save 'comparison' $comparison
$gpu.session_diagnostics[0].provider_registered='Cpu'; Save 'gpu' $gpu
Expect-Failure {Run}
$gpu.session_diagnostics[0].provider_registered='DirectMl'; Save 'gpu' $gpu
$comparison.stems.bass.difference_from_baseline_not_ground_truth.channels[0].max_abs_error=0.001; Save 'comparison' $comparison
Expect-Failure {Run}
$comparison.stems.bass.difference_from_baseline_not_ground_truth.channels[0].max_abs_error=1e-5; Save 'comparison' $comparison
$gpu.cases[0].metrics.drums.reference_sha256='f'*64; Save 'gpu' $gpu
Expect-Failure {Run}
Write-Output 'Provider parity fixtures passed (valid, SI-SDR regression, mismatched WAV, CPU fallback, PCM error, changed reference).'
