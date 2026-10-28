#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string[]]$Arguments,
    [Parameter(Mandatory)][string]$Report,
    [string]$Stdout,
    [string]$Stderr
)
$ErrorActionPreference='Stop'
$info=[Diagnostics.ProcessStartInfo]::new((Resolve-Path -LiteralPath $Executable).Path)
$info.UseShellExecute=$false
$info.CreateNoWindow=$true
$info.RedirectStandardOutput=[bool]$Stdout
$info.RedirectStandardError=[bool]$Stderr
foreach($argument in $Arguments){$info.ArgumentList.Add($argument)}
$process=[Diagnostics.Process]::new()
$process.StartInfo=$info
$timer=[Diagnostics.Stopwatch]::StartNew()
if(-not $process.Start()){throw 'Could not start benchmark'}
$benchmarkOutputTask=if($Stdout){$process.StandardOutput.ReadToEndAsync()}else{$null}
$benchmarkErrorTask=if($Stderr){$process.StandardError.ReadToEndAsync()}else{$null}
$peakBytes=0L
$peakPrivateBytes=0L
while(-not $process.WaitForExit(100)){
    $process.Refresh()
    if(-not $process.HasExited){
        $peakBytes=[Math]::Max($peakBytes,$process.WorkingSet64)
        $peakPrivateBytes=[Math]::Max($peakPrivateBytes,$process.PrivateMemorySize64)
    }
}
$timer.Stop()
if($benchmarkOutputTask){$benchmarkOutputTask.GetAwaiter().GetResult() | Set-Content -LiteralPath $Stdout -Encoding utf8}
if($benchmarkErrorTask){$benchmarkErrorTask.GetAwaiter().GetResult() | Set-Content -LiteralPath $Stderr -Encoding utf8}
$result=[ordered]@{schema_version=1;exit_code=$process.ExitCode;elapsed_seconds=$timer.Elapsed.TotalSeconds;sampled_peak_rss_bytes=$peakBytes;sampled_peak_private_bytes=$peakPrivateBytes;rss_sampling_ms=100;cpu_seconds=$process.TotalProcessorTime.TotalSeconds;scope='native parent process; excludes child codecs and Flutter shell'}
$result | ConvertTo-Json | Set-Content -LiteralPath $Report -Encoding utf8
if($process.ExitCode -ne 0){throw "Benchmark exited with $($process.ExitCode)"}
