param(
    [Parameter(Mandatory = $true)][string]$Payload15Directory,
    [Parameter(Mandatory = $true)][string]$Payload17Directory,
    [string]$IsccPath = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
)

# Runs unattended installs over fixtures of both XAE Shell generations, to check which
# shell receives TcTidy. Each scenario is a series of setups and the version each shell
# holds at the end, read from its uninstall record.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'invoke-waited-process.ps1')
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
$payloads = @{
    '15' = (Resolve-Path -LiteralPath $Payload15Directory).Path
    '17' = (Resolve-Path -LiteralPath $Payload17Directory).Path
}
# The AppIds of the PayloadFixture1517 fixture in TcTidy.iss.
$appIds = @{
    '15' = '{1B6C9E1C-7C2F-4B7B-9E0A-5D5B7C3A1F15}'
    '17' = '{2C7DAF2D-8D30-4C8C-AF1B-6E6C8D4B2F17}'
}
$package = 'TcTidy.VsExtension.dll'
$workRoot = Join-Path ([System.IO.Path]::GetTempPath()) (
    'tctidy-installer-target-' + [guid]::NewGuid())
$shellsRoot = Join-Path $workRoot 'shells'

function Get-ShellIde([string]$Generation) {
    Join-Path $shellsRoot "xae$Generation\Common7\IDE"
}

function Get-ShellPackage([string]$Generation) {
    Join-Path (Get-ShellIde $Generation) "Extensions\TcTidy\$package"
}

function Get-RecordKey([string]$Generation) {
    "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$($appIds[$Generation])_is1"
}

function New-Shells {
    foreach ($generation in '15', '17') {
        New-Item -ItemType Directory -Path (Get-ShellIde $generation) | Out-Null
        Copy-Item "$env:WINDIR\System32\where.exe" (
            Join-Path (Get-ShellIde $generation) 'TcXaeShell.exe')
    }
}

# Removes what a scenario left, whatever it failed on, without hiding that failure.
function Remove-Shells {
    try {
        foreach ($generation in '15', '17') {
            $uninstaller = Join-Path (Get-ShellIde $generation) 'Extensions\TcTidy\unins000.exe'
            if (Test-Path -LiteralPath $uninstaller -PathType Leaf) {
                try {
                    Invoke-WaitedProcess `
                        -FilePath $uninstaller `
                        -ArgumentList '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART' `
                        -Label "XAE $generation fixture uninstall"
                } catch {
                    Write-Warning $_.Exception.Message
                }
            }
            Remove-Item -LiteralPath (Get-RecordKey $generation) -Recurse -Force `
                -ErrorAction SilentlyContinue
        }
    } finally {
        Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $shellsRoot
    }
}

function Invoke-Setup([string]$Run, [string]$Label) {
    $version, $arguments = $Run -split ' ', 2
    $quotedRoot = '"' + $shellsRoot + '"'
    Invoke-WaitedProcess `
        -FilePath (Join-Path $workRoot "TcTidySetup-$version.exe") `
        -ArgumentList "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /PAYLOADFIXTUREROOT=$quotedRoot $arguments" `
        -Label $Label
}

function Assert-Target {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string[]]$Runs,
        [Parameter(Mandatory = $true)][hashtable]$Expected,
        [string]$UnrecordedCopy = ''
    )

    New-Shells
    try {
        if ($UnrecordedCopy) {
            $seed = Get-ShellPackage $UnrecordedCopy
            New-Item -ItemType Directory -Path (Split-Path -Parent $seed) | Out-Null
            Set-Content -LiteralPath $seed -Value 'unrecorded copy' -Encoding ascii -NoNewline
        }
        foreach ($run in $Runs) {
            Invoke-Setup -Run $run -Label "${Name}: setup $run"
        }

        foreach ($generation in '15', '17') {
            $installed = Get-ShellPackage $generation
            $holds = (Get-ItemProperty -LiteralPath (Get-RecordKey $generation) `
                    -ErrorAction SilentlyContinue).DisplayVersion
            $wanted = $Expected[$generation]
            $marker = Join-Path (Get-ShellIde $generation) 'extension.configurationchanged'
            if ($holds -cne $wanted) {
                throw "${Name}: XAE $generation holds '$holds' instead of '$wanted'"
            }
            if ($wanted) {
                if ((Get-FileHash -LiteralPath $installed).Hash -cne
                    (Get-FileHash -LiteralPath (Join-Path $payloads[$generation] $package)).Hash) {
                    throw "${Name}: XAE $generation did not receive its own payload"
                }
                if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) {
                    throw "${Name}: XAE $generation did not get its merge marker"
                }
            } else {
                if (Test-Path -LiteralPath $installed) {
                    throw "${Name}: setup installed into XAE $generation"
                }
                if (Test-Path -LiteralPath $marker) {
                    throw "${Name}: setup touched the merge marker of XAE $generation"
                }
            }
        }
        Write-Host "OK: $Name"
    } finally {
        Remove-Shells
    }
}

New-Item -ItemType Directory -Path $workRoot | Out-Null
Push-Location -LiteralPath $root
try {
    if (-not (Test-Path -LiteralPath $IsccPath -PathType Leaf)) {
        throw "Inno Setup compiler not found: $IsccPath"
    }
    foreach ($version in '1.0.0', '2.0.0', '2.0.0-test', '3.0.0') {
        & $IsccPath `
            "/DAppVersion=$version" `
            '/DNumericVersion=1.0.0.0' `
            "/DPayload15Dir=$($payloads['15'])" `
            "/DPayload17Dir=$($payloads['17'])" `
            '/DPayloadFixture' `
            '/DPayloadFixture1517' `
            "/O$workRoot" `
            editors/vs/installer/TcTidy.iss
        if ($LASTEXITCODE -ne 0) { throw "installer target fixture $version failed to build" }
    }

    Assert-Target -Name 'a fresh install goes to the newest shell' `
        -Runs @('2.0.0') `
        -Expected @{ '17' = '2.0.0' }
    Assert-Target -Name 'an upgrade goes to 15.0 when only 15.0 has TcTidy' `
        -Runs @('1.0.0 /SHELL=15.0', '2.0.0') `
        -Expected @{ '15' = '2.0.0' }
    Assert-Target -Name 'an upgrade goes to 17.0 when only 17.0 has TcTidy' `
        -Runs @('1.0.0 /SHELL=17.0', '2.0.0') `
        -Expected @{ '17' = '2.0.0' }
    Assert-Target -Name 'with two older copies, one setup updates the newest shell' `
        -Runs @('1.0.0 /SHELL=15.0', '1.0.0 /SHELL=17.0', '2.0.0') `
        -Expected @{ '15' = '1.0.0'; '17' = '2.0.0' }
    Assert-Target -Name 'with two older copies, a second setup updates the other shell' `
        -Runs @('1.0.0 /SHELL=15.0', '1.0.0 /SHELL=17.0', '2.0.0', '2.0.0') `
        -Expected @{ '15' = '2.0.0'; '17' = '2.0.0' }
    Assert-Target -Name 'running the same setup again stays in 17.0' `
        -Runs @('2.0.0', '2.0.0') `
        -Expected @{ '17' = '2.0.0' }
    Assert-Target -Name 'running the same setup again stays in 15.0' `
        -Runs @('2.0.0 /SHELL=15.0', '2.0.0') `
        -Expected @{ '15' = '2.0.0' }
    Assert-Target -Name 'a newer copy is not downgraded while another shell is free' `
        -Runs @('3.0.0 /SHELL=15.0', '2.0.0') `
        -Expected @{ '15' = '3.0.0'; '17' = '2.0.0' }
    Assert-Target -Name 'a newer copy is not downgraded while another shell is older' `
        -Runs @('3.0.0 /SHELL=17.0', '1.0.0 /SHELL=15.0', '2.0.0') `
        -Expected @{ '15' = '2.0.0'; '17' = '3.0.0' }
    Assert-Target -Name 'a test build replaces the release with its number' `
        -Runs @('2.0.0 /SHELL=15.0', '2.0.0-test') `
        -Expected @{ '15' = '2.0.0-test' }
    Assert-Target -Name 'an unrecorded copy is upgraded' `
        -UnrecordedCopy '15' `
        -Runs @('2.0.0') `
        -Expected @{ '15' = '2.0.0' }
    Assert-Target -Name 'a named shell wins over the preselection' `
        -Runs @('1.0.0 /SHELL=15.0', '2.0.0 /SHELL=17.0') `
        -Expected @{ '15' = '1.0.0'; '17' = '2.0.0' }

    $elsewhere = Join-Path $workRoot 'elsewhere'
    Assert-Target -Name '/DIR= does not change the target' `
        -Runs @("2.0.0 /DIR=`"$elsewhere`"") `
        -Expected @{ '17' = '2.0.0' }
    if (Test-Path -LiteralPath $elsewhere) { throw 'setup installed into the /DIR= folder' }

    New-Shells
    try {
        $quotedRoot = '"' + $shellsRoot + '"'
        $process = Start-Process `
            -FilePath (Join-Path $workRoot 'TcTidySetup-2.0.0.exe') `
            -ArgumentList "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /PAYLOADFIXTUREROOT=$quotedRoot /SHELL=16.0" `
            -WindowStyle Hidden `
            -Wait `
            -PassThru
        if ($process.ExitCode -eq 0) { throw 'setup accepted a shell that does not exist' }
        foreach ($generation in '15', '17') {
            if (Test-Path -LiteralPath (Get-ShellPackage $generation)) {
                throw "setup installed into XAE $generation for a shell that does not exist"
            }
        }
        Write-Host 'OK: a shell that does not exist fails the setup'
    } finally {
        Remove-Shells
    }
} finally {
    Pop-Location
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $workRoot
}
