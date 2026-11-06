param(
    [Parameter(Mandatory = $true)][string]$Tag,
    [Parameter(Mandatory = $true)][string]$ExpectedSha,
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [Parameter(Mandatory = $true)][string]$NoticesFile,
    [Parameter(Mandatory = $true)][string]$CargoInventoryFile,
    [string]$IsccPath = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'assert-version-text.ps1')
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
$notices = (Resolve-Path -LiteralPath $NoticesFile).Path
$cargoInventory = (Resolve-Path -LiteralPath $CargoInventoryFile).Path
[System.IO.Directory]::CreateDirectory($OutputDirectory) | Out-Null
$output = (Resolve-Path -LiteralPath $OutputDirectory).Path
$stage = Join-Path ([System.IO.Path]::GetTempPath()) ("tctidy-cli-" + [guid]::NewGuid())
$cliPayload = Join-Path ([System.IO.Path]::GetTempPath()) ("tctidy-cli-payload-" + [guid]::NewGuid())
$payload15 = Join-Path ([System.IO.Path]::GetTempPath()) ("tctidy-payload-xae15-" + [guid]::NewGuid())
$payload17 = Join-Path ([System.IO.Path]::GetTempPath()) ("tctidy-payload-xae17-" + [guid]::NewGuid())
$evidence = Join-Path ([System.IO.Path]::GetTempPath()) ("tctidy-evidence-" + [guid]::NewGuid())
$oldRustFlags = $env:RUSTFLAGS

Push-Location -LiteralPath $root
try {
    if ($ExpectedSha -cnotmatch '^[0-9a-f]{40}$') {
        throw 'ExpectedSha must be a full lowercase 40-hex commit ID'
    }
    $actualSha = (& git rev-parse HEAD | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'cannot read the checked-out source SHA' }
    if ($actualSha -cne $ExpectedSha) {
        throw "checked-out source is $actualSha, expected $ExpectedSha"
    }
    $version = (& cargo xtask release version | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'cannot read the product version' }
    if ($Tag -ne "v$version") { throw "candidate label $Tag does not match source version $version" }
    & cargo xtask release check
    if ($LASTEXITCODE -ne 0) { throw 'release metadata check failed' }
    if ((& git status --porcelain | Out-String).Trim()) {
        throw 'candidate builds require a clean worktree'
    }
    if ((Get-Item -LiteralPath $notices).Length -eq 0) { throw "third-party notices are empty: $notices" }
    if ((Get-Item -LiteralPath $cargoInventory).Length -eq 0) { throw "Cargo dependency inventory is empty: $cargoInventory" }
    $provenancePath = Join-Path $output 'tctidy-windows-provenance.json'
    Remove-Item -LiteralPath $provenancePath -Force -ErrorAction SilentlyContinue
    [System.IO.Directory]::CreateDirectory($evidence) | Out-Null
    & cargo xtask licenses generate `
        --target x86_64-pc-windows-msvc `
        --inventory (Join-Path $evidence 'CARGO-DEPENDENCIES.json') `
        --notice (Join-Path $evidence 'THIRD-PARTY-NOTICES.md') `
        --metadata-output (Join-Path $evidence 'cargo-metadata.json') `
        --about-output (Join-Path $evidence 'cargo-about.json') `
        --tree-output (Join-Path $evidence 'cargo-tree.txt')
    if ($LASTEXITCODE -ne 0) { throw 'exact Cargo evidence generation failed' }
    & cargo xtask licenses compare `
        --supplied-inventory $cargoInventory `
        --supplied-notice $notices `
        --generated-inventory (Join-Path $evidence 'CARGO-DEPENDENCIES.json') `
        --generated-notice (Join-Path $evidence 'THIRD-PARTY-NOTICES.md')
    if ($LASTEXITCODE -ne 0) { throw 'supplied Cargo evidence is stale or invalid' }

    $env:RUSTFLAGS = '-C target-feature=+crt-static'
    & cargo build -p tctidy-cli --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'release CLI build failed' }
    $cliVersion = (& target/release/tctidy.exe --version | Out-String).Trim()
    if ($cliVersion -ne "tctidy $version") { throw "CLI reports '$cliVersion'" }

    [System.IO.Directory]::CreateDirectory($stage) | Out-Null
    $cliFiles = @('target/release/tctidy.exe', 'LICENSE-MIT', 'LICENSE-APACHE', 'README.md', 'CHANGELOG.md')
    Copy-Item -LiteralPath $cliFiles -Destination $stage
    Copy-Item -LiteralPath (Join-Path $evidence 'THIRD-PARTY-NOTICES.md') `
        -Destination $stage
    Copy-Item -LiteralPath (Join-Path $evidence 'CARGO-DEPENDENCIES.json') `
        -Destination $stage
    $base = "tctidy-$Tag-x86_64-pc-windows-msvc"
    $archive = Join-Path $output "$base.zip"
    Remove-Item -LiteralPath @($archive, "$archive.sha256") -Force -ErrorAction SilentlyContinue
    Compress-Archive -Path "$stage/*" -DestinationPath $archive
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -LiteralPath "$archive.sha256" -Value "$hash  $base.zip" -Encoding ascii -NoNewline
    [System.IO.Directory]::CreateDirectory($cliPayload) | Out-Null
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::ExtractToDirectory($archive, $cliPayload)
    & cargo xtask licenses notice `
        --payload $cliPayload `
        --notice THIRD-PARTY-NOTICES.md
    if ($LASTEXITCODE -ne 0) { throw 'CLI notice references do not resolve inside the archive.' }

    Copy-Item -LiteralPath (Join-Path $evidence 'THIRD-PARTY-NOTICES.md') `
        -Destination editors/vs/TcTidy.VsExtension/THIRD-PARTY-NOTICES.md -Force
    Copy-Item -LiteralPath (Join-Path $evidence 'CARGO-DEPENDENCIES.json') `
        -Destination editors/vs/TcTidy.VsExtension/CARGO-DEPENDENCIES.json -Force
    & (Join-Path $PSScriptRoot 'build-vsix.ps1') `
        -ProductVersion $version `
        -Payload15Directory $payload15 `
        -Payload17Directory $payload17 `
        -CargoMetadata (Join-Path $evidence 'cargo-metadata.json') `
        -CargoAbout (Join-Path $evidence 'cargo-about.json') `
        -CargoTree (Join-Path $evidence 'cargo-tree.txt')
    if (-not (Test-Path -LiteralPath $IsccPath -PathType Leaf)) { throw "Inno Setup compiler not found: $IsccPath" }
    $numeric = (($version -split '-')[0]) + '.0'
    $builtInstaller = "editors/vs/installer/dist/TcTidySetup-$version.exe"
    Remove-Item -LiteralPath @($builtInstaller, "$builtInstaller.sha256") -Force -ErrorAction SilentlyContinue
    & $IsccPath "/DAppVersion=$version" "/DNumericVersion=$numeric" `
        "/DPayload15Dir=$payload15" "/DPayload17Dir=$payload17" `
        editors/vs/installer/TcTidy.iss
    if ($LASTEXITCODE -ne 0) { throw 'installer build failed' }

    $installer = Get-Item -LiteralPath $builtInstaller
    if ($installer.Length -eq 0) { throw 'installer is empty' }
    $versionInfo = $installer.VersionInfo
    $productNumeric = "$($versionInfo.ProductMajorPart).$($versionInfo.ProductMinorPart).$($versionInfo.ProductBuildPart).$($versionInfo.ProductPrivatePart)"
    if ($productNumeric -ne $numeric) { throw "installer numeric product version reports '$productNumeric'" }
    Assert-ExactVersionText `
        -Actual $versionInfo.ProductVersion `
        -Expected $version `
        -Label 'installer text product version'
    Assert-ExactVersionText `
        -Actual $versionInfo.FileVersion `
        -Expected $numeric `
        -Label 'installer file version'
    foreach ($payload in @($payload15, $payload17)) {
        & editors/vs/Verify-Version.ps1 `
            -PayloadDir $payload `
            -InstallerPath $installer.FullName `
            -FullVersion $version `
            -SystemVersion (($version -split '-')[0]) `
            -AssemblyVersion $numeric
    }
    foreach ($fixtureShellVersion in @('15', '17')) {
        & ./scripts/release/validate-installer.ps1 `
            -Payload15Directory $payload15 `
            -Payload17Directory $payload17 `
            -AppVersion $version `
            -NumericVersion $numeric `
            -FixtureShellVersion $fixtureShellVersion `
            -IsccPath $IsccPath
    }

    $destination = Join-Path $output $installer.Name
    Copy-Item -LiteralPath $installer.FullName -Destination $destination -Force
    # Keep this after every installer content-changing step, including future signing.
    $hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -LiteralPath "$destination.sha256" -Value "$hash  $($installer.Name)" -Encoding ascii -NoNewline
    $provenance = [ordered]@{
        schema = 1
        component = 'windows'
        tag = $Tag
        version = $version
        source_sha = $actualSha
    }
    $provenance | ConvertTo-Json -Compress | Set-Content -LiteralPath $provenancePath -Encoding ascii
    Write-Host "Windows candidate ready at $output (source $actualSha)"
} finally {
    $env:RUSTFLAGS = $oldRustFlags
    Pop-Location
    Remove-Item -LiteralPath @($stage, $payload15, $payload17) -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath @($cliPayload, $evidence) -Recurse -Force -ErrorAction SilentlyContinue
}
