#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ReleaseDirectory,
    [Parameter(Mandatory)][string]$InnoCompiler,
    [string]$PreviousVersion = '0.6.0'
)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$releaseRoot = (Resolve-Path -LiteralPath $ReleaseDirectory).Path
$compiler = (Resolve-Path -LiteralPath $InnoCompiler).Path
$versionLine = Select-String -LiteralPath (Join-Path $repoRoot 'pubspec.yaml') -Pattern '^version:\s*([0-9]+\.[0-9]+\.[0-9]+)(?:\+\d+)?\s*$'
if (-not $versionLine) { throw 'Could not read the application version from pubspec.yaml.' }
$appVersion = $versionLine.Matches[0].Groups[1].Value
if ($PreviousVersion -notmatch '^\d+\.\d+\.\d+$' -or $PreviousVersion -eq $appVersion) { throw 'PreviousVersion must be a different semantic version.' }
$identity = [Guid]::NewGuid().ToString()
$testRoot = Join-Path $repoRoot "build\installer-qualification-$identity"
$installRoot = Join-Path $testRoot 'Installed app ñ with spaces'
$output = Join-Path $testRoot 'installers'
New-Item -ItemType Directory -Path $output -Force | Out-Null
New-Item -ItemType Directory -Path $installRoot -Force | Out-Null
# Model a normal Programs directory without changing the workspace or user ACLs.
& "$env:SystemRoot\System32\icacls.exe" $installRoot /setintegritylevel '(OI)(CI)M' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Could not label the owned installation fixture for normal execution.' }
# A unique AppId isolates registry/uninstall metadata from every real installation.
$testAppId = '{{' + $identity + '}'
$project = Join-Path $testRoot 'User projects ñ'
New-Item -ItemType Directory -Path $project -Force | Out-Null
$sentinel = Join-Path $project 'session.json'
'{"session_id":"installer-preservation-fixture"}' | Set-Content -LiteralPath $sentinel -Encoding utf8
$sentinelHash = (Get-FileHash -LiteralPath $sentinel).Hash
$appDataRoot = Join-Path $env:LOCALAPPDATA 'Atomusicator'
New-Item -ItemType Directory -Path $appDataRoot -Force | Out-Null
$appDataSentinel = Join-Path $appDataRoot "qualification-$identity.json"
'{"preserve":true}' | Set-Content -LiteralPath $appDataSentinel -Encoding utf8
$appDataHash = (Get-FileHash -LiteralPath $appDataSentinel).Hash
function Assert-UserFiles {
    if ((Get-FileHash -LiteralPath $sentinel).Hash -ne $sentinelHash -or
        (Get-FileHash -LiteralPath $appDataSentinel).Hash -ne $appDataHash) {
        throw 'Installer changed user-owned data.'
    }
}
try {
    foreach ($version in @($PreviousVersion, $appVersion)) {
        & $compiler "/DAppVersion=$version" "/DInstallerAppId=$testAppId" '/DInstallerCompression=zip' "/DReleaseDir=$releaseRoot" "/DOutputDir=$output" (Join-Path $PSScriptRoot 'Atomusicator.iss') *> (Join-Path $testRoot "compile-$version.log")
        if ($LASTEXITCODE -ne 0) { throw "Qualification installer $version did not compile." }
        $installer = Join-Path $output "Atomusicator-Setup-$version.exe"
        # Match the production build's normal executable integrity. Otherwise
        # a Low-label compiler workspace can make the fixture fail before logs
        # or per-user registration are created, independently of the installer.
        & "$env:SystemRoot\System32\icacls.exe" $installer /setintegritylevel M | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Could not label the generated qualification installer for normal execution.' }
        $arguments = @('/CURRENTUSER', '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/NOICONS',
            ('/DIR="' + $installRoot + '"'), ('/LOG="' + (Join-Path $testRoot "install-$version.log") + '"'))
        $setup = Start-Process -FilePath $installer -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $testRoot "setup-$version.stdout.log") -RedirectStandardError (Join-Path $testRoot "setup-$version.stderr.log")
        if (-not $setup.WaitForExit(120000)) { $setup.Kill(); throw "Installation $version exceeded two minutes; inspect its log." }
        if ($setup.ExitCode -ne 0) { throw "Installation $version exited $($setup.ExitCode)." }
        & (Join-Path $PSScriptRoot 'validate_windows_release.ps1') -ReleaseDirectory $installRoot -Launch -Report (Join-Path $testRoot "startup-$version.json")
        Assert-UserFiles
    }
    $uninstaller = Join-Path $installRoot 'unins000.exe'
    if (-not (Test-Path -LiteralPath $uninstaller -PathType Leaf)) { throw 'Isolated uninstaller missing.' }
    # Regression: uninstall must refuse while this exact test app is running.
    # Never close unrelated application instances to make this test pass.
    $runningApp = Start-Process -FilePath (Join-Path $installRoot 'atomusicator.exe') -WorkingDirectory $installRoot -WindowStyle Hidden -PassThru
    try {
        $marker = $null
        for ($probe = 0; $probe -lt 30 -and -not $marker; $probe++) {
            try { $marker = [Threading.Mutex]::OpenExisting('Local\Atomusicator.Running') }
            catch [Threading.WaitHandleCannotBeOpenedException] { Start-Sleep -Milliseconds 100 }
        }
        if (-not $marker) { throw 'Running app did not publish its installer mutex.' }
        $marker.Dispose()
        $blocked = Start-Process -FilePath $uninstaller -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART') -WindowStyle Hidden -PassThru
        if (-not $blocked.WaitForExit(30000)) { $blocked.Kill(); throw 'Blocked-uninstall probe did not exit.' }
        $runningApp.Refresh()
        if ($blocked.ExitCode -eq 0 -or $runningApp.HasExited -or
            -not (Test-Path -LiteralPath (Join-Path $installRoot 'atomusicator.exe'))) {
            throw 'Uninstaller failed to protect the running application.'
        }
        Assert-UserFiles
    } finally {
        if (-not $runningApp.HasExited) {
            $null = $runningApp.CloseMainWindow()
            if (-not $runningApp.WaitForExit(5000)) { $runningApp.Kill(); $runningApp.WaitForExit() }
        }
        $runningApp.Dispose()
    }
    $uninstall = Start-Process -FilePath $uninstaller -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', ('/LOG="' + (Join-Path $testRoot 'uninstall.log') + '"')) -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $testRoot 'uninstall.stdout.log') -RedirectStandardError (Join-Path $testRoot 'uninstall.stderr.log')
    if (-not $uninstall.WaitForExit(120000)) { $uninstall.Kill(); throw 'Uninstall exceeded two minutes; inspect its log.' }
    if ($uninstall.ExitCode -ne 0) { throw "Uninstall exited $($uninstall.ExitCode)." }
    Assert-UserFiles
    if (Test-Path -LiteralPath (Join-Path $installRoot 'atomusicator.exe')) { throw 'Application survived uninstall.' }
    [ordered]@{
        schema_version = 1
        isolated_app_id = $identity
        current_user_install = $true
        installer_version_upgrade = "$PreviousVersion -> $appVersion; same current payload"
        unicode_path_startup = $true
        user_projects_and_appdata_preserved = $true
        uninstall = $true
        uninstall_refuses_running_app = $true
        clean_windows_qualification = $false
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $testRoot 'result.json') -Encoding utf8
    Write-Host "Installer qualification evidence: $testRoot"
}
finally {
    # Only the unique probe created above is removed. Real app data is untouched.
    if (Test-Path -LiteralPath $appDataSentinel -PathType Leaf) { Remove-Item -LiteralPath $appDataSentinel }
}
