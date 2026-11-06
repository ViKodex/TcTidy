function Assert-ExactVersionText {
    param(
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Actual,
        [Parameter(Mandatory = $true)][string]$Expected,
        [Parameter(Mandatory = $true)][string]$Label
    )

    $normalized = $Actual.Trim()
    if ($normalized -cne $Expected) {
        throw "$Label reports '$Actual'"
    }
}
