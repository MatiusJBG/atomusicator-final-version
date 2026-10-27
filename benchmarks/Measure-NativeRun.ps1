#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string[]]$Arguments,
    [Parameter(Mandatory)][string]$Report
)
$ErrorActionPreference='Stop'
$info=[Diagnostics.ProcessStartInfo]::new((Resolve-Path -LiteralPath $Executable).Path)
$info.UseShellExecute=$false
$info.CreateNoWindow=$true
foreach($argument in $Arguments){$info.ArgumentList.Add($argument)}
$process=[Diagnostics.Process]::new()
$process.StartInfo=$info
$timer=[Diagnostics.Stopwatch]::StartNew()
if(-not $process.Start()){throw 'Could not start benchmark'}
$peakBytes=0L
while(-not $process.WaitForExit(100)){
    $process.Refresh()
    if(-not $process.HasExited){$peakBytes=[Math]::Max($peakBytes,$process.WorkingSet64)}
}
$timer.Stop()
$result=[ordered]@{schema_version=1;exit_code=$process.ExitCode;elapsed_seconds=$timer.Elapsed.TotalSeconds;sampled_peak_rss_bytes=$peakBytes;rss_sampling_ms=100;cpu_seconds=$process.TotalProcessorTime.TotalSeconds}
$result | ConvertTo-Json | Set-Content -LiteralPath $Report -Encoding utf8
if($process.ExitCode -ne 0){throw "Benchmark exited with $($process.ExitCode)"}
