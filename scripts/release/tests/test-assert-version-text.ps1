$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../assert-version-text.ps1')

Assert-ExactVersionText `
    -Actual ('0.3.2' + (' ' * 45)) `
    -Expected '0.3.2' `
    -Label 'padded product version'
Assert-ExactVersionText `
    -Actual ("`t0.3.2.0`r`n") `
    -Expected '0.3.2.0' `
    -Label 'padded file version'

foreach ($junk in @('0.3.2 junk', '0.3.2.0', 'v0.3.2')) {
    $accepted = $true
    try {
        Assert-ExactVersionText `
            -Actual $junk `
            -Expected '0.3.2' `
            -Label 'junk fixture'
    } catch {
        if ($_.Exception.Message -cne "junk fixture reports '$junk'") {
            throw
        }
        $accepted = $false
    }
    if ($accepted) {
        throw "version contract accepted '$junk'"
    }
}

Write-Host 'OK: padded version resources normalize without accepting junk.'
