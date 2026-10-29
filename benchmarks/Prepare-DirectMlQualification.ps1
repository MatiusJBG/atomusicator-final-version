#requires -Version 7.0
[CmdletBinding()]
param([string]$Directory = 'build/toolchain/directml-qualification')
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') {
    throw 'This qualification SDK requires Windows x64.'
}
$root = [IO.Path]::GetFullPath($Directory)
New-Item -ItemType Directory -Force -Path $root | Out-Null
$packages = @(
    @{Name='ort'; Id='microsoft.ml.onnxruntime.directml'; Version='1.20.0'; Hash='E9E341DAF6157F5E89413D9AD457E576B80832788916863C6427566C45F0DEC9'},
    @{Name='directml'; Id='microsoft.ai.directml'; Version='1.15.2'; Hash='9F07482559087088A4DBA4AE76EEEEE1FAD3F7077A92CCFBDB439C6BC2964C09'}
)
foreach ($package in $packages) {
    $archive = Join-Path $root "$($package.Name).nupkg"
    $url = "https://api.nuget.org/v3-flatcontainer/$($package.Id)/$($package.Version)/$($package.Id).$($package.Version).nupkg"
    if (-not (Test-Path -LiteralPath $archive)) { Invoke-WebRequest -Uri $url -OutFile $archive }
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $package.Hash) {
        throw "Package hash mismatch: $archive"
    }
    [IO.Compression.ZipFile]::ExtractToDirectory($archive, (Join-Path $root $package.Name), $true)
}
$sdk = Join-Path $root 'sdk'
New-Item -ItemType Directory -Force -Path "$sdk/lib", "$sdk/licenses" | Out-Null
Copy-Item -LiteralPath "$root/ort/runtimes/win-x64/native/onnxruntime.dll", "$root/ort/runtimes/win-x64/native/onnxruntime.lib", "$root/directml/bin/x64-win/DirectML.dll" -Destination "$sdk/lib"
Copy-Item -LiteralPath "$root/ort/LICENSE" -Destination "$sdk/licenses/ONNX-Runtime-LICENSE.txt"
Copy-Item -LiteralPath "$root/ort/ThirdPartyNotices.txt" -Destination "$sdk/licenses/ONNX-Runtime-ThirdPartyNotices.txt"
Copy-Item -LiteralPath "$root/directml/LICENSE.txt" -Destination "$sdk/licenses/DirectML-LICENSE.txt"
foreach ($binary in @("$sdk/lib/onnxruntime.dll", "$sdk/lib/DirectML.dll")) {
    $signature = Get-AuthenticodeSignature -LiteralPath $binary
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation(?:,|$)') {
        throw "Invalid Microsoft binary signature: $binary"
    }
}
@{
    schema_version=1
    scope='Local qualification SDK; does not enable or redistribute a production accelerator'
    packages=$packages
    files=@(Get-ChildItem "$sdk/lib", "$sdk/licenses" -File | ForEach-Object {
        @{name=$_.Name; sha256=(Get-FileHash -LiteralPath $_.FullName).Hash; bytes=$_.Length}
    })
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$sdk/provenance.json" -Encoding utf8
Write-Output "Prepared and verified local SDK: $sdk"
Write-Output 'Build provider_qualify with ORT_LIB_LOCATION=<sdk>, ORT_PREFER_DYNAMIC_LINK=1 and a separate CARGO_TARGET_DIR.'
