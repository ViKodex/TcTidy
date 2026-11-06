param(
    [Parameter(Mandatory = $true)][string]$Payload15Directory,
    [Parameter(Mandatory = $true)][string]$Payload17Directory,
    [string]$AppVersion = '0.0.0-test',
    [string]$NumericVersion = '0.0.0.0',
    [string]$IsccPath = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
$securityResult = Join-Path ([System.IO.Path]::GetTempPath()) (
    'tctidy-installer-security-' + [guid]::NewGuid() + '.txt')

Push-Location -LiteralPath $root
try {
    $payload15 = (Resolve-Path -LiteralPath $Payload15Directory).Path
    $payload17 = (Resolve-Path -LiteralPath $Payload17Directory).Path
    if (-not (Test-Path -LiteralPath $IsccPath -PathType Leaf)) {
        throw "Inno Setup compiler not found: $IsccPath"
    }
    $common = @(
        "/DAppVersion=$AppVersion",
        "/DNumericVersion=$NumericVersion",
        "/DPayload15Dir=$payload15",
        "/DPayload17Dir=$payload17"
    )
    $targets = @(
        @{ Define = $null; Suffix = '' },
        @{ Define = 'DiscoveryFixture15'; Suffix = '-fixture-15' },
        @{ Define = 'DiscoveryFixture17'; Suffix = '-fixture-17' },
        @{ Define = 'DiscoveryFixture1517'; Suffix = '-fixture-15-and-17' },
        @{ Define = 'SecurityFixture'; Suffix = '-fixture-security' }
    )
    foreach ($target in $targets) {
        $output = Join-Path $root (
            "editors/vs/installer/dist/TcTidySetup-$AppVersion$($target.Suffix).exe")
        Remove-Item -LiteralPath $output -Force -ErrorAction SilentlyContinue
        $defines = $common + "/DOutputSuffix=$($target.Suffix)"
        if ($target.Define) { $defines += "/D$($target.Define)" }
        & $IsccPath @defines editors/vs/installer/TcTidy.iss
        if ($LASTEXITCODE -ne 0) {
            throw "Installer target $($target.Define) compilation failed."
        }
        if (-not (Test-Path -LiteralPath $output -PathType Leaf)) {
            throw "Missing compiled installer: $output"
        }
        if ($target.Define -eq 'SecurityFixture') {
            $quotedSecurityResult = '"' + $securityResult + '"'
            $securityArguments = @(
                '/VERYSILENT'
                '/SUPPRESSMSGBOXES'
                "/SECURITYFIXTUREOUTPUT=$quotedSecurityResult"
            ) -join ' '
            $securityProcess = Start-Process `
                -FilePath $output `
                -ArgumentList $securityArguments `
                -WindowStyle Hidden `
                -Wait `
                -PassThru
            $securityExitCode = $securityProcess.ExitCode
            $securityResultExists = Test-Path -LiteralPath $securityResult -PathType Leaf
            $securityResultContent = if ($securityResultExists) {
                Get-Content -LiteralPath $securityResult -Raw
            } else {
                '<missing>'
            }
            Write-Host "Security fixture exit code: $securityExitCode"
            Write-Host "Security fixture result exists: $securityResultExists"
            Write-Host "Security fixture result: $securityResultContent"
            if (($securityExitCode -ne 1) -or
                (-not $securityResultExists) -or
                ($securityResultContent -cne 'PASS')) {
                throw 'Installer security fixture failed.'
            }
        }
    }

    foreach ($shellVersion in @('15', '17')) {
        & (Join-Path $PSScriptRoot 'validate-installer.ps1') `
            -Payload15Directory $payload15 `
            -Payload17Directory $payload17 `
            -AppVersion $AppVersion `
            -NumericVersion $NumericVersion `
            -FixtureShellVersion $shellVersion `
            -IsccPath $IsccPath
    }
    & (Join-Path $PSScriptRoot 'validate-installer-target.ps1') `
        -Payload15Directory $payload15 `
        -Payload17Directory $payload17 `
        -IsccPath $IsccPath
} finally {
    Pop-Location
    Remove-Item -LiteralPath $securityResult -Force -ErrorAction SilentlyContinue
}
