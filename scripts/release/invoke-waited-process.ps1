function Invoke-WaitedProcess {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$ArgumentList,
        [Parameter(Mandatory = $true)][string]$Label
    )

    try {
        $process = Start-Process `
            -FilePath $FilePath `
            -ArgumentList $ArgumentList `
            -WindowStyle Hidden `
            -Wait `
            -PassThru `
            -ErrorAction Stop
        $exitCode = $process.ExitCode
    } catch {
        throw "cannot start or wait for ${Label}: $($_.Exception.Message)"
    }
    if ($exitCode -ne 0) {
        throw "$Label failed with exit code $exitCode"
    }
}
