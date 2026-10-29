#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string[]]$Arguments,
    [Parameter(Mandatory)][string]$Report,
    [string]$Stdout,
    [string]$Stderr,
    [ValidateRange(25,10000)][int]$SampleIntervalMs = 100
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
$treePeakBytes=0L
$treePeakPrivateBytes=0L
$observed=@{}
$sampleCount=0
$maxSampleGapMs=0.0
$lastSampleMs=0.0
$treeAvailable=$true
while(-not $process.WaitForExit($SampleIntervalMs)){
    $process.Refresh()
    if(-not $process.HasExited){
        $peakBytes=[Math]::Max($peakBytes,$process.WorkingSet64)
        $peakPrivateBytes=[Math]::Max($peakPrivateBytes,$process.PrivateMemorySize64)
    }
    $nowMs=$timer.Elapsed.TotalMilliseconds
    $maxSampleGapMs=[Math]::Max($maxSampleGapMs,$nowMs-$lastSampleMs)
    $lastSampleMs=$nowMs
    $sampleCount++
    try {
        # Discover descendants by parent ID; no dependency on executable names.
        $snapshot=@(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,CreationDate,Name,WorkingSetSize,PrivatePageCount,KernelModeTime,UserModeTime -ErrorAction Stop)
        $ids=[Collections.Generic.HashSet[uint32]]::new()
        [void]$ids.Add([uint32]$process.Id)
        do {
            $added=$false
            foreach($entry in $snapshot){
                if($ids.Contains([uint32]$entry.ParentProcessId) -and $entry.CreationDate -ge $process.StartTime -and $ids.Add([uint32]$entry.ProcessId)){$added=$true}
            }
        } while($added)
        $rss=0L
        $private=0L
        foreach($entry in $snapshot){
            if(-not $ids.Contains([uint32]$entry.ProcessId)){continue}
            $rss+=[long]$entry.WorkingSetSize
            $private+=[long]$entry.PrivatePageCount
            $identity="$($entry.ProcessId)/$($entry.CreationDate.ToUniversalTime().Ticks)"
            $observed[$identity]=[ordered]@{pid=$entry.ProcessId;parent_pid=$entry.ParentProcessId;name=$entry.Name;cpu_seconds=([double]$entry.KernelModeTime+[double]$entry.UserModeTime)/1e7}
        }
        $treePeakBytes=[Math]::Max($treePeakBytes,$rss)
        $treePeakPrivateBytes=[Math]::Max($treePeakPrivateBytes,$private)
    } catch {
        $treeAvailable=$false
    }
}
$timer.Stop()
if($benchmarkOutputTask){$benchmarkOutputTask.GetAwaiter().GetResult() | Set-Content -LiteralPath $Stdout -Encoding utf8}
if($benchmarkErrorTask){$benchmarkErrorTask.GetAwaiter().GetResult() | Set-Content -LiteralPath $Stderr -Encoding utf8}
$result=[ordered]@{schema_version=2;exit_code=$process.ExitCode;elapsed_seconds=$timer.Elapsed.TotalSeconds;sampled_peak_rss_bytes=$peakBytes;sampled_peak_private_bytes=$peakPrivateBytes;rss_sampling_ms=$SampleIntervalMs;sample_count=$sampleCount;max_sample_gap_ms=$maxSampleGapMs;cpu_seconds=$process.TotalProcessorTime.TotalSeconds;tree_sampled_peak_rss_bytes=$(if($treeAvailable -and $sampleCount){$treePeakBytes}else{$null});tree_sampled_peak_private_bytes=$(if($treeAvailable -and $sampleCount){$treePeakPrivateBytes}else{$null});observed_processes=@($observed.Values);vram_bytes=$null;scope='root and observed descendants until root exits; launch the outermost process to include Flutter';limitations='CIM polling adds overhead; short-lived children and peaks between samples may be missed; RSS includes shared pages; private bytes are process commit, not system Commit Charge; VRAM not measured'}
$result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $Report -Encoding utf8
if($process.ExitCode -ne 0){throw "Benchmark exited with $($process.ExitCode)"}
