param(
    [Parameter(Mandatory = $true)][string]$Payload15Directory,
    [Parameter(Mandatory = $true)][string]$Payload17Directory,
    [Parameter(Mandatory = $true)][string]$AppVersion,
    [Parameter(Mandatory = $true)][string]$NumericVersion,
    [ValidateSet('15', '17')][string]$FixtureShellVersion = '15',
    [string]$IsccPath = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'invoke-waited-process.ps1')
$root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$payload15 = (Resolve-Path $Payload15Directory).Path
$payload17 = (Resolve-Path $Payload17Directory).Path
$payload = if ($FixtureShellVersion -eq '15') { $payload15 } else { $payload17 }
$fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) (
    'tctidy-installer-payload-' + [guid]::NewGuid())
$fixtureIde = Join-Path $fixtureRoot 'Common7\IDE'
$installed = Join-Path $fixtureIde 'Extensions\TcTidy'
$mergeMarker = Join-Path $fixtureIde 'extension.configurationchanged'
$pkgdefMarker = Join-Path $fixtureIde 'Extensions\extensions.configurationchanged'
$foreignSentinel = Join-Path $installed 'foreign-sentinel.txt'
$foreignSentinelHolding = Join-Path $fixtureRoot 'foreign-sentinel-validation-hold.txt'
$foreignSentinelContent = 'foreign sentinel must survive uninstall'
$legacyOnlyFile = 'Microsoft.VisualStudio.DpiAwareness.dll'
$legacyOnlyInstalled = Join-Path $installed $legacyOnlyFile
$uninstaller = Join-Path $installed 'unins000.exe'
$legacyFixture = Join-Path $root (
    'scripts/release/tests/fixtures/TcTidy-v0.3.2-unsafe.iss')
$legacyBuild = Join-Path $fixtureRoot 'legacy-installer'
$legacyPayload = Join-Path $fixtureRoot 'v0.3.2-payload'
$legacyInstaller = Join-Path $legacyBuild (
    'TcTidySetup-0.3.2-unsafe-fixture.exe')
$installer = Join-Path $root (
    "editors/vs/installer/dist/TcTidySetup-$AppVersion-payload-fixture-$FixtureShellVersion.exe")
$ownedFiles = @()
$uninstallAssertionsReady = $false

Push-Location $root
try {
    if (-not (Test-Path $IsccPath -PathType Leaf)) {
        throw "Inno Setup compiler not found: $IsccPath"
    }
    if (-not (Test-Path (Join-Path $payload 'tctidy.exe') -PathType Leaf)) {
        throw 'installer fixture payload lacks tctidy.exe'
    }
    New-Item -ItemType Directory -Path $fixtureIde -Force | Out-Null
    New-Item -ItemType Directory -Path $legacyBuild -Force | Out-Null
    New-Item -ItemType Directory -Path $legacyPayload -Force | Out-Null
    Copy-Item "$env:WINDIR\System32\where.exe" (Join-Path $fixtureIde 'TcXaeShell.exe')
    if (Test-Path (Join-Path $payload $legacyOnlyFile)) {
        throw "current payload unexpectedly contains legacy-only file: $legacyOnlyFile"
    }
    Get-ChildItem -LiteralPath $payload -Force | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination $legacyPayload -Recurse
    }
    Set-Content -LiteralPath (Join-Path $legacyPayload $legacyOnlyFile) `
        -Value 'v0.3.2 legacy-owned assembly fixture' -Encoding ascii -NoNewline

    & $IsccPath `
        "/DPayloadDir=$legacyPayload" `
        "/O$legacyBuild" `
        $legacyFixture
    if ($LASTEXITCODE -ne 0) { throw 'v0.3.2 uninstall fixture build failed' }

    $fixtureDefines = @(
        "/DAppVersion=$AppVersion"
        "/DNumericVersion=$NumericVersion"
        "/DPayload15Dir=$payload15"
        "/DPayload17Dir=$payload17"
        "/DOutputSuffix=-payload-fixture-$FixtureShellVersion"
        '/DPayloadFixture'
    )
    if ($FixtureShellVersion -eq '17') {
        $fixtureDefines += '/DPayloadFixture17'
    }
    & $IsccPath @fixtureDefines editors/vs/installer/TcTidy.iss
    if ($LASTEXITCODE -ne 0) { throw 'current installer fixture build failed' }

    $quotedFixtureRoot = '"' + $fixtureRoot + '"'
    $quotedInstalled = '"' + $installed + '"'
    $installArguments = @(
        '/VERYSILENT'
        '/SUPPRESSMSGBOXES'
        '/NORESTART'
        "/PAYLOADFIXTUREROOT=$quotedFixtureRoot"
        "/DIR=$quotedInstalled"
    ) -join ' '
    $legacyInstallLog = Join-Path $fixtureRoot 'legacy-install.log'
    try {
        Invoke-WaitedProcess `
            -FilePath $legacyInstaller `
            -ArgumentList ($installArguments + ' /LOG=' + '"' + $legacyInstallLog + '"') `
            -Label 'v0.3.2 uninstall fixture installation'
    } catch {
        if (Test-Path -LiteralPath $legacyInstallLog -PathType Leaf) {
            Get-Content -LiteralPath $legacyInstallLog | Write-Host
        }
        throw
    }

    if (-not (Test-Path $uninstaller -PathType Leaf)) {
        throw "v0.3.2 uninstall fixture has no uninstaller: $uninstaller"
    }
    if (-not (Test-Path $legacyOnlyInstalled -PathType Leaf)) {
        throw "v0.3.2 installer fixture lacks legacy-only file: $legacyOnlyInstalled"
    }
    Set-Content -LiteralPath $foreignSentinel `
        -Value $foreignSentinelContent -Encoding ascii -NoNewline

    Invoke-WaitedProcess `
        -FilePath $installer `
        -ArgumentList $installArguments `
        -Label 'current installer fixture upgrade'

    if (Test-Path $legacyOnlyInstalled) {
        throw "legacy-only file survived current installer upgrade: $legacyOnlyInstalled"
    }
    $ownedFiles = @(
        Get-ChildItem -LiteralPath $payload -File -Recurse | ForEach-Object {
            $relative = $_.FullName.Substring($payload.Length).TrimStart(
                [char[]]'\/')
            $destination = Join-Path $installed $relative
            if (-not (Test-Path -LiteralPath $destination -PathType Leaf)) {
                throw "installed payload file is missing: $relative"
            }
            if ((Get-FileHash -LiteralPath $_.FullName).Hash -cne
                (Get-FileHash -LiteralPath $destination).Hash) {
                throw "installed payload file differs: $relative"
            }
            $destination
        }
    )
    $ownedFiles += @(
        Get-ChildItem -LiteralPath $installed -File | Where-Object {
            $_.Name -match '^unins[0-9]{3}\.(exe|dat)$'
        } | ForEach-Object { $_.FullName }
    )
    $ownedFiles += $legacyOnlyInstalled
    if ($ownedFiles.Count -eq 0) {
        throw 'installer fixture recorded no owned files'
    }
    if (-not (Test-Path $foreignSentinel -PathType Leaf) -or
        ((Get-Content -LiteralPath $foreignSentinel -Raw) -cne
            $foreignSentinelContent)) {
        throw 'current installer upgrade removed or changed the foreign sentinel'
    }
    Move-Item -LiteralPath $foreignSentinel -Destination $foreignSentinelHolding
    try {
        & cargo xtask licenses installer `
            --reference-payload $payload `
            --installed-payload $installed
        if ($LASTEXITCODE -ne 0) { throw 'installed payload validation failed' }
    } finally {
        if (Test-Path $foreignSentinelHolding -PathType Leaf) {
            Move-Item -LiteralPath $foreignSentinelHolding -Destination $foreignSentinel
        }
    }
    foreach ($marker in $mergeMarker, $pkgdefMarker) {
        if (-not (Test-Path $marker -PathType Leaf)) {
            throw "installer fixture did not create merge marker: $marker"
        }
        Remove-Item -LiteralPath $marker -Force
    }
    $uninstallAssertionsReady = $true
} finally {
    try {
        if (Test-Path $uninstaller -PathType Leaf) {
            Invoke-WaitedProcess `
                -FilePath $uninstaller `
                -ArgumentList '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART' `
                -Label 'current installer fixture uninstallation'
            if ($uninstallAssertionsReady) {
                if (-not (Test-Path $foreignSentinel -PathType Leaf) -or
                    ((Get-Content -LiteralPath $foreignSentinel -Raw) -cne
                        $foreignSentinelContent)) {
                    throw 'installer fixture removed or changed the foreign sentinel'
                }
                $remainingOwned = @($ownedFiles | Where-Object {
                    Test-Path -LiteralPath $_ -PathType Leaf
                })
                if ($remainingOwned.Count -ne 0) {
                    throw ('uninstaller left legacy and current owned files: ' +
                        ($remainingOwned -join ', '))
                }
                if (Test-Path $legacyOnlyInstalled) {
                    throw ('legacy-only file survived current installer uninstall: ' +
                        $legacyOnlyInstalled)
                }
                foreach ($marker in $mergeMarker, $pkgdefMarker) {
                    if (-not (Test-Path $marker -PathType Leaf)) {
                        throw "uninstaller did not recreate the exact merge marker: $marker"
                    }
                }
            }
        } elseif ($uninstallAssertionsReady) {
            throw "installer fixture lost its uninstaller: $uninstaller"
        }
    } finally {
        Pop-Location
        Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $fixtureRoot
        Remove-Item -Force -ErrorAction SilentlyContinue $installer
    }
}
