param(
    [Parameter(Mandatory = $true)][string]$Assembly,
    [Parameter(Mandatory = $true)][string]$Vsix,
    [Parameter(Mandatory = $true)][ValidateSet('15', '17')][string]$ShellVersion,
    [string]$ProductVersion = '',
    [string]$ProjectAssets = 'editors/vs/TcTidy.VsExtension/obj/project.assets.json',
    [string[]]$LicenseFiles = @('LICENSE-MIT', 'LICENSE-APACHE', 'editors/vs/TcTidy.VsExtension/LICENSE'),
    [string]$LicenseInventory = '',
    [string]$CargoMetadata = 'target/tctidy-cli-windows-metadata.json',
    [string]$CargoAbout = 'target/tctidy-cli-windows-about.json',
    [string]$CargoTree = 'target/tctidy-cli-windows-tree.txt'
)

$ErrorActionPreference = 'Stop'
$assemblyPath = (Resolve-Path $Assembly).Path
$archivePath = (Resolve-Path $Vsix).Path
$assetsPath = (Resolve-Path $ProjectAssets).Path
$licenseArguments = @($LicenseFiles | ForEach-Object { '--license'; (Resolve-Path $_).Path })
if (-not $LicenseInventory) {
    if ($ShellVersion -eq '15') {
        $LicenseInventory = 'editors/vs/TcTidy.VsExtension/THIRD-PARTY-INVENTORY.xae15.json'
        $manifestPath = "$PSScriptRoot/../../editors/vs/TcTidy.VsExtension/source.xae15.extension.vsixmanifest"
    } else {
        $LicenseInventory = 'editors/vs/TcTidy.VsExtension/THIRD-PARTY-INVENTORY.json'
        $manifestPath = "$PSScriptRoot/../../editors/vs/TcTidy.VsExtension/source.extension.vsixmanifest"
    }
} elseif ($ShellVersion -eq '15') {
    $manifestPath = "$PSScriptRoot/../../editors/vs/TcTidy.VsExtension/source.xae15.extension.vsixmanifest"
} else {
    $manifestPath = "$PSScriptRoot/../../editors/vs/TcTidy.VsExtension/source.extension.vsixmanifest"
}
$inventoryPath = (Resolve-Path $LicenseInventory).Path
$cargoMetadataPath = (Resolve-Path $CargoMetadata).Path
$cargoAboutPath = (Resolve-Path $CargoAbout).Path
$cargoTreePath = (Resolve-Path $CargoTree).Path

& cargo xtask vsix check `
    --shell-version $ShellVersion `
    --manifest $manifestPath `
    --project "$PSScriptRoot/../../editors/vs/TcTidy.VsExtension/TcTidy.VsExtension.csproj" `
    --project "$PSScriptRoot/../../editors/vs/TcTidy.VsExtension.Core/TcTidy.VsExtension.Core.csproj" `
    --vsix $archivePath
if ($LASTEXITCODE -ne 0) {
    throw 'VSIX shell-target, payload or managed-architecture validation failed.'
}

$resources = [System.Reflection.Assembly]::LoadFrom($assemblyPath).GetManifestResourceNames()
if ($resources -notcontains 'VSPackage.resources') {
    throw 'VSPackage.resources missing: the .cto did not merge into the VSPackage container.'
}
if ($resources -contains '_EmptyResource.resources') {
    throw '_EmptyResource.resources present: the .cto merged into the wrong container.'
}

Add-Type -AssemblyName System.IO.Compression.FileSystem

$payload = Join-Path ([System.IO.Path]::GetTempPath()) ("tctidy-vsix-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $payload | Out-Null
try {
    [System.IO.Compression.ZipFile]::ExtractToDirectory($archivePath, $payload)
    & cargo xtask licenses vsix `
        --payload $payload `
        --inventory $inventoryPath `
        @licenseArguments `
        --project-assets $assetsPath `
        --cargo-metadata $cargoMetadataPath `
        --cargo-about $cargoAboutPath `
        --cargo-tree $cargoTreePath
    if ($LASTEXITCODE -ne 0) { throw 'VSIX license inventory validation failed.' }

    # The About box reads the commit of the build from this stamp.
    $coreVersion = (Get-Item -LiteralPath (Join-Path $payload 'TcTidy.VsExtension.Core.dll')).VersionInfo.ProductVersion
    if ($coreVersion -cnotmatch '\+[0-9a-f]{7,}$') {
        throw "TcTidy.VsExtension.Core.dll does not name its commit: $coreVersion"
    }

    if ($ProductVersion) {
        $systemVersion = ($ProductVersion -split '[-+]')[0]
        $assemblyVersion = "$systemVersion.0"
        & (Join-Path $PSScriptRoot '../../editors/vs/Verify-Version.ps1') `
            -PayloadDir $payload `
            -FullVersion $ProductVersion `
            -SystemVersion $systemVersion `
            -AssemblyVersion $assemblyVersion
    }
} finally {
    Remove-Item -Recurse -Force $payload
}

Write-Host 'OK: VSIX resources, payload and product versions are valid.'
