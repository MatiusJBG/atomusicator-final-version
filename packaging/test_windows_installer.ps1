[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ReleaseDirectory,
    [Parameter(Mandatory)][string]$InnoCompiler
)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$releaseRoot = (Resolve-Path -LiteralPath $ReleaseDirectory).Path
$compiler = (Resolve-Path -LiteralPath $InnoCompiler).Path
$identity = [Guid]::NewGuid().ToString()
$testRoot = Join-Path $repoRoot "build\installer-qualification-$identity"
$installRoot = Join-Path $testRoot 'Installed app ñ with spaces'
$output = Join-Path $testRoot 'installers'
New-Item -ItemType Directory -Path $output -Force | Out-Null
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
    foreach ($version in @('0.5.9', '0.6.0')) {
        & $compiler "/DAppVersion=$version" "/DInstallerAppId=$testAppId" '/DInstallerCompression=zip' "/DReleaseDir=$releaseRoot" "/DOutputDir=$output" (Join-Path $PSScriptRoot 'Atomusicator.iss') *> (Join-Path $testRoot "compile-$version.log")
        if ($LASTEXITCODE -ne 0) { throw "Qualification installer $version did not compile." }
        $installer = Join-Path $output "Atomusicator-Setup-$version.exe"
        $arguments = @('/CURRENTUSER', '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/NOICONS',
            ('/DIR="' + $installRoot + '"'), ('/LOG="' + (Join-Path $testRoot "install-$version.log") + '"'))
        $setup = Start-Process -FilePath $installer -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
        if ($setup.ExitCode -ne 0) { throw "Installation $version exited $($setup.ExitCode)." }
        & (Join-Path $PSScriptRoot 'validate_windows_release.ps1') -ReleaseDirectory $installRoot -Launch -Report (Join-Path $testRoot "startup-$version.json")
        Assert-UserFiles
    }
    $uninstaller = Join-Path $installRoot 'unins000.exe'
    if (-not (Test-Path -LiteralPath $uninstaller -PathType Leaf)) { throw 'Isolated uninstaller missing.' }
    $uninstall = Start-Process -FilePath $uninstaller -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', ('/LOG="' + (Join-Path $testRoot 'uninstall.log') + '"')) -WindowStyle Hidden -Wait -PassThru
    if ($uninstall.ExitCode -ne 0) { throw "Uninstall exited $($uninstall.ExitCode)." }
    Assert-UserFiles
    if (Test-Path -LiteralPath (Join-Path $installRoot 'atomusicator.exe')) { throw 'Application survived uninstall.' }
    [ordered]@{
        schema_version = 1
        isolated_app_id = $identity
        current_user_install = $true
        installer_version_upgrade = '0.5.9 -> 0.6.0; same current payload'
        unicode_path_startup = $true
        user_projects_and_appdata_preserved = $true
        uninstall = $true
        clean_windows_qualification = $false
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $testRoot 'result.json') -Encoding utf8
    Write-Host "Installer qualification evidence: $testRoot"
}
finally {
    # Only the unique probe created above is removed. Real app data is untouched.
    if (Test-Path -LiteralPath $appDataSentinel -PathType Leaf) { Remove-Item -LiteralPath $appDataSentinel }
}
