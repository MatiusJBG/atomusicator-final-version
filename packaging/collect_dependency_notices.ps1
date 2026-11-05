#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ReleaseDirectory,
    [Parameter(Mandatory)][string]$FlutterRoot,
    [string]$Repository=(Split-Path $PSScriptRoot -Parent)
)
$ErrorActionPreference='Stop'
$releaseRoot=[IO.Path]::GetFullPath($ReleaseDirectory)
$noticeRoot=Join-Path $releaseRoot 'licenses'
New-Item -ItemType Directory -Force $noticeRoot|Out-Null
$metadataText=& cargo metadata --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc --manifest-path (Join-Path $Repository 'rust_core/Cargo.toml')
if($LASTEXITCODE -ne 0){throw 'Cannot resolve the locked Windows dependency graph'}
$metadata=$metadataText|ConvertFrom-Json -AsHashtable
$packages=@{};foreach($package in $metadata.packages){$packages[$package.id]=$package}
$nodes=@{};foreach($node in $metadata.resolve.nodes){$nodes[$node.id]=$node}
$root=@($metadata.packages|Where-Object name -eq 'atomusicator-bootstrap-ffi')[0]
$pending=[Collections.Generic.Queue[string]]::new();$pending.Enqueue($root.id)
$seen=[Collections.Generic.HashSet[string]]::new()
while($pending.Count){$id=$pending.Dequeue();if(-not $seen.Add($id)){continue};foreach($dep in $nodes[$id].deps){if(@($dep.dep_kinds|Where-Object kind -ne 'dev').Count){$pending.Enqueue($dep.pkg)}}}
$inventory=@();$missing=@();$spdxPackages=@();$spdxIds=@{}
$fallbacks=Get-Content (Join-Path $Repository 'third_party/licenses/provenance.json') -Raw|ConvertFrom-Json
foreach($id in $seen){$package=$packages[$id];$spdxIds[$id]='SPDXRef-'+($package.name+'-'+$package.version -replace '[^A-Za-z0-9.-]','-')}
foreach($id in ($seen|Sort-Object)){
    $package=$packages[$id];$sourceRoot=Split-Path $package.manifest_path -Parent
    $firstParty=$null -eq $package.source
    $texts=@();$sourceBundle=$null
    if(-not $firstParty){
        $destination=Join-Path $noticeRoot "rust/$($package.name)-$($package.version)"
        $files=@(Get-ChildItem -LiteralPath $sourceRoot -File -Recurse|Where-Object {$_.Name -match '^(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE)([._-].*)?$' -and -not ($_.Attributes -band [IO.FileAttributes]::ReparsePoint)})
        foreach($file in $files){
            $relative=[IO.Path]::GetRelativePath($sourceRoot,$file.FullName)
            $target=Join-Path $destination $relative
            New-Item -ItemType Directory -Force (Split-Path $target -Parent)|Out-Null
            Copy-Item -LiteralPath $file.FullName -Destination $target -Force
            $texts+=@{path=[IO.Path]::GetRelativePath($releaseRoot,$target);sha256=(Get-FileHash -LiteralPath $target).Hash.ToLowerInvariant()}
        }
        if(-not $texts.Count){
            $family=if($package.name -like 'symphonia*' -and $package.version -eq '0.5.5'){'symphonia'}elseif($package.name -in @('flutter_rust_bridge','flutter_rust_bridge_macros') -and $package.version -eq '2.8.0'){'flutter_rust_bridge'}elseif($package.name -eq 'dasp_sample' -and $package.version -eq '0.11.0'){'dasp_sample'}else{$null}
            foreach($entry in @($fallbacks|Where-Object family -eq $family)){
                $license=Join-Path $Repository $entry.path
                if((Get-FileHash -LiteralPath $license).Hash -ne $entry.sha256){throw "Upstream license hash mismatch: $license"}
                New-Item -ItemType Directory -Force $destination|Out-Null
                $target=Join-Path $destination ([IO.Path]::GetFileName($license))
                Copy-Item -LiteralPath $license -Destination $target -Force
                $texts+=@{path=[IO.Path]::GetRelativePath($releaseRoot,$target);sha256=$entry.sha256;provenance=$entry.url;upstream_commit=$entry.commit}
            }
        }
        if(-not $texts.Count){$missing+=@{name=$package.name;version=$package.version;license=$package.license}}
        if($package.license -match '(MPL|LGPL|GPL|CDDL)-'){
            $sourceDirectory=Join-Path $noticeRoot 'covered-sources'
            New-Item -ItemType Directory -Force $sourceDirectory|Out-Null
            $zip=Join-Path $sourceDirectory "$($package.name)-$($package.version).zip"
            $temporaryZip=Join-Path $sourceDirectory ([Guid]::NewGuid().ToString('N')+'.zip')
            [IO.Compression.ZipFile]::CreateFromDirectory($sourceRoot,$temporaryZip,[IO.Compression.CompressionLevel]::Optimal,$false)
            [IO.File]::Move($temporaryZip,$zip,$true)
            $sourceBundle=@{path=[IO.Path]::GetRelativePath($releaseRoot,$zip);sha256=(Get-FileHash -LiteralPath $zip).Hash.ToLowerInvariant()}
        }
    }
    $declared=if($firstParty){'LicenseRef-Proprietary'}elseif($package.license){$package.license}else{'NOASSERTION'}
    $inventory+=@{name=$package.name;version=$package.version;source=$package.source;declared_license=$declared;license_files=$texts;covered_source_bundle=$sourceBundle;first_party=$firstParty}
    $download=if($package.source -like 'registry+*'){"https://crates.io/api/v1/crates/$($package.name)/$($package.version)/download"}else{'NOASSERTION'}
    $spdxPackages+=@{name=$package.name;SPDXID=$spdxIds[$id];versionInfo=$package.version;downloadLocation=$download;filesAnalyzed=$false;licenseConcluded='NOASSERTION';licenseDeclared=$declared;copyrightText='NOASSERTION';externalRefs=@(@{referenceCategory='PACKAGE-MANAGER';referenceType='purl';referenceLocator="pkg:cargo/$($package.name)@$($package.version)"})}
}
$relationships=@(@{spdxElementId='SPDXRef-DOCUMENT';relationshipType='DESCRIBES';relatedSpdxElement=$spdxIds[$root.id]})
foreach($id in $seen){foreach($dep in $nodes[$id].deps){if($seen.Contains($dep.pkg) -and @($dep.dep_kinds|Where-Object kind -ne 'dev').Count){$relationships+=@{spdxElementId=$spdxIds[$id];relationshipType='DEPENDS_ON';relatedSpdxElement=$spdxIds[$dep.pkg]}}}}
@{spdxVersion='SPDX-2.3';dataLicense='CC0-1.0';SPDXID='SPDXRef-DOCUMENT';name="Atomusicator $($root.version) locked Windows Rust graph (includes build dependencies)";documentNamespace="https://spdx.org/spdxdocs/atomusicator-$([guid]::NewGuid())";creationInfo=@{creators=@('Tool: Atomusicator dependency inventory');created=[DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')};packages=$spdxPackages;relationships=$relationships;hasExtractedLicensingInfos=@(@{licenseId='LicenseRef-Proprietary';extractedText=(Get-Content (Join-Path $Repository 'LICENSE') -Raw)})}|ConvertTo-Json -Depth 15|Set-Content (Join-Path $releaseRoot 'sbom-rust.spdx.json') -Encoding utf8
foreach($copy in @(
    @{source=(Join-Path $FlutterRoot 'LICENSE');name='flutter/LICENSE.txt'},
    @{source=(Join-Path $FlutterRoot 'bin/cache/pkg/sky_engine/LICENSE');name='flutter/ENGINE-THIRD-PARTY-LICENSES.txt'},
    @{source=(Join-Path $FlutterRoot 'bin/cache/artifacts/engine/windows-x64-release/license.windows_flutter.md');name='flutter/ENGINE-LICENSE-PROVENANCE.md'},
    @{source=(Join-Path $FlutterRoot 'bin/cache/dart-sdk/LICENSE');name='flutter/DART-LICENSE.txt'}
)){
    if(-not(Test-Path -LiteralPath $copy.source -PathType Leaf)){throw "Required Flutter license missing: $($copy.source)"}
    $target=Join-Path $noticeRoot $copy.name
    New-Item -ItemType Directory -Force (Split-Path $target -Parent)|Out-Null
    Copy-Item -LiteralPath $copy.source -Destination $target -Force
}
foreach($entry in @($fallbacks|Where-Object family -eq 'onnxruntime-1.20.0')){
    $source=Join-Path $Repository $entry.path
    if((Get-FileHash -LiteralPath $source).Hash -ne $entry.sha256){throw 'ONNX Runtime notice provenance mismatch'}
    $destination=Join-Path $noticeRoot 'onnxruntime'
    New-Item -ItemType Directory -Force $destination|Out-Null
    Copy-Item -LiteralPath $source -Destination $destination -Force
}
$dartText=& (Join-Path $FlutterRoot 'bin/cache/dart-sdk/bin/dart.exe') pub deps --json --directory $Repository
if($LASTEXITCODE -ne 0){throw 'Cannot resolve Dart dependency inventory'}
$dart=$dartText|ConvertFrom-Json -AsHashtable
$dartPackages=@{};foreach($package in $dart.packages){$dartPackages[$package.name]=$package}
$dartSeen=[Collections.Generic.HashSet[string]]::new();$dartPending=[Collections.Generic.Queue[string]]::new()
$dartPending.Enqueue($dart.root)
while($dartPending.Count){$name=$dartPending.Dequeue();if(-not $dartSeen.Add($name)){continue};$package=$dartPackages[$name];$dependencies=if($name -eq $dart.root){$package.directDependencies}else{$package.dependencies};foreach($dep in $dependencies){$dartPending.Enqueue($dep)}}
$configPath=Join-Path $Repository '.dart_tool/package_config.json'
$config=Get-Content $configPath -Raw|ConvertFrom-Json
$dartInventory=@();$dartMissing=@()
foreach($name in ($dartSeen|Sort-Object)){
    if($name -eq $dart.root){continue}
    $entry=@($config.packages|Where-Object name -eq $name)[0]
    if(-not $entry){throw "Dart package root missing: $name"}
    $uri=[Uri]::new([Uri]$configPath,[string]$entry.rootUri)
    $directory=$uri.LocalPath
    $texts=@()
    $licenseFiles=@(Get-ChildItem -LiteralPath $directory -File|Where-Object Name -Match '^(LICENSE|LICENCE|COPYING|NOTICE)([._-].*)?$')
    # Flutter SDK packages inherit the SDK root license. Only use this fallback
    # for SDK dependencies whose resolved directory is inside this SDK.
    $sdkPackages=[IO.Path]::GetFullPath((Join-Path $FlutterRoot 'packages'))+[IO.Path]::DirectorySeparatorChar
    if(-not $licenseFiles.Count -and $dartPackages[$name].source -eq 'sdk' -and
        [IO.Path]::GetFullPath($directory).StartsWith($sdkPackages,[StringComparison]::OrdinalIgnoreCase)){
        $licenseFiles=@(Get-Item -LiteralPath (Join-Path $FlutterRoot 'LICENSE'))
    }
    foreach($file in $licenseFiles){
        $destination=Join-Path $noticeRoot "dart/$name"
        New-Item -ItemType Directory -Force $destination|Out-Null
        $target=Join-Path $destination $file.Name
        Copy-Item -LiteralPath $file.FullName -Destination $target -Force
        $texts+=@{path=[IO.Path]::GetRelativePath($releaseRoot,$target);sha256=(Get-FileHash -LiteralPath $target).Hash.ToLowerInvariant()}
    }
    if(-not $texts.Count){$dartMissing+=$name}
    $dartInventory+=@{name=$name;version=$dartPackages[$name].version;source=$dartPackages[$name].source;license_files=$texts}
}
@{schema_version=1;scope='Dart runtime dependency closure, excluding root development dependencies';packages=$dartInventory;missing_license_texts=$dartMissing}|ConvertTo-Json -Depth 10|Set-Content (Join-Path $releaseRoot 'dependency-license-inventory-dart.json') -Encoding utf8
@{schema_version=1;scope='Locked Windows Rust graph including build tools; Flutter engine/Dart notices are separate; generated metadata does not relicense first-party code';rust=$inventory;missing_rust_license_texts=$missing;public_distribution_ready=$false;pending_obligations=@('Verify FFmpeg complete corresponding source, including enabled dependencies and build scripts','Record exact ONNX Runtime native notices and MSVC redistribution provenance','Complete final Windows/device qualification and review all artifact-specific terms')}|ConvertTo-Json -Depth 12|Set-Content (Join-Path $releaseRoot 'dependency-license-inventory.json') -Encoding utf8
Write-Output "Collected $($inventory.Count) Rust package records; missing license-text sets: $($missing.Count)"
Write-Output "Collected $($dartInventory.Count) Dart runtime package records; missing license-text sets: $($dartMissing.Count)"
if($missing.Count -or $dartMissing.Count){throw 'Incomplete dependency license texts; inspect generated inventories'}
