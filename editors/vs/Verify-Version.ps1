param(
    [Parameter(Mandatory = $true)]
    [string]$PayloadDir,
    [string]$InstallerPath = '',
    [Parameter(Mandatory = $true)]
    [string]$FullVersion,
    [Parameter(Mandatory = $true)]
    [string]$SystemVersion,
    [Parameter(Mandatory = $true)]
    [string]$AssemblyVersion
)

$ErrorActionPreference = 'Stop'

function Assert-Equal {
    param(
        [string]$Name,
        [string]$Actual,
        [string]$Expected
    )

    if ($Actual -ne $Expected) {
        throw "$Name is '$Actual'; expected '$Expected'."
    }
}

$payload = (Resolve-Path $PayloadDir).Path
$manifestPath = Join-Path $payload 'extension.vsixmanifest'
$assemblyPath = Join-Path $payload 'TcTidy.VsExtension.dll'
$pkgdefPath = Join-Path $payload 'TcTidy.VsExtension.pkgdef'

[xml]$manifest = Get-Content -Raw $manifestPath
Assert-Equal 'VSIX manifest version' ([string]$manifest.PackageManifest.Metadata.Identity.Version) $SystemVersion

$assemblyName = [System.Reflection.AssemblyName]::GetAssemblyName($assemblyPath)
Assert-Equal 'assembly version' $assemblyName.Version.ToString() $AssemblyVersion

$assemblyInfo = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($assemblyPath)
Assert-Equal 'assembly file version' $assemblyInfo.FileVersion $AssemblyVersion
Assert-Equal 'assembly informational/product version' $assemblyInfo.ProductVersion $FullVersion

$pkgdef = Get-Content -Raw $pkgdefPath
$escapedFullVersion = [regex]::Escape($FullVersion)
if ($pkgdef -notmatch ('"PID"\s*=\s*"' + $escapedFullVersion + '"')) {
    throw "Installed-product registration does not expose '$FullVersion'."
}

if ($InstallerPath) {
    $installer = (Resolve-Path $InstallerPath).Path
    Assert-Equal 'installer filename' (Split-Path -Leaf $installer) "TcTidySetup-$FullVersion.exe"
    $installerInfo = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($installer)
    Assert-Equal 'installer file version' $installerInfo.FileVersion.Trim() $AssemblyVersion
    Assert-Equal 'installer product version' $installerInfo.ProductVersion.Trim() $FullVersion
}

Write-Host "OK: every requested VSIX and installer version field exposes $FullVersion ($AssemblyVersion for System.Version fields)."
