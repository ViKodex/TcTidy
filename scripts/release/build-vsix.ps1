param(
    [Parameter(Mandatory = $true)][string]$ProductVersion,
    [Parameter(Mandatory = $true)][string]$Payload15Directory,
    [Parameter(Mandatory = $true)][string]$Payload17Directory,
    [string]$CargoMetadata = 'target/tctidy-cli-windows-metadata.json',
    [string]$CargoAbout = 'target/tctidy-cli-windows-about.json',
    [string]$CargoTree = 'target/tctidy-cli-windows-tree.txt',
    [string]$MSBuildPath = 'msbuild'
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path

Push-Location -LiteralPath $root
try {
    $metadata = (Resolve-Path -LiteralPath $CargoMetadata).Path
    $about = (Resolve-Path -LiteralPath $CargoAbout).Path
    $tree = (Resolve-Path -LiteralPath $CargoTree).Path
    $shellBuilds = @(
        @{ Version = '15'; Payload = $Payload15Directory },
        @{ Version = '17'; Payload = $Payload17Directory }
    )
    foreach ($shellBuild in $shellBuilds) {
        $destination = $shellBuild.Payload
        if (-not [System.IO.Path]::IsPathRooted($destination)) {
            $destination = Join-Path $root $destination
        }
        $shellBuild.Payload = [System.IO.Path]::GetFullPath($destination)
        if (Test-Path -LiteralPath $shellBuild.Payload) {
            if ((-not (Test-Path -LiteralPath $shellBuild.Payload -PathType Container)) -or
                (Get-ChildItem -LiteralPath $shellBuild.Payload -Force | Select-Object -First 1)) {
                throw "VSIX extraction requires an empty directory: $($shellBuild.Payload)"
            }
        }
    }
    if ($shellBuilds[0].Payload -eq $shellBuilds[1].Payload) {
        throw 'XAE 15 and 17 require separate payload directories.'
    }

    # The VSIX packs whatever tctidy.exe sits in the project folder, which git ignores,
    # so bundle the CLI of this checkout rather than trust one left there.
    $engine = 'target/release/tctidy.exe'
    $oldRustFlags = $env:RUSTFLAGS
    $env:RUSTFLAGS = '-C target-feature=+crt-static'
    try {
        & cargo build -p tctidy-cli --release --locked
        if ($LASTEXITCODE -ne 0) { throw 'release CLI build failed' }
    } finally {
        $env:RUSTFLAGS = $oldRustFlags
    }
    Copy-Item -LiteralPath $engine -Destination editors/vs/TcTidy.VsExtension/tctidy.exe -Force
    $engineHash = (Get-FileHash -LiteralPath $engine).Hash

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    # Validate each shell before restoring the next one: NuGet rewrites project.assets.json.
    foreach ($shellBuild in $shellBuilds) {
        $shellVersion = $shellBuild.Version
        & $MSBuildPath editors/vs/TcTidy.VsExtension/TcTidy.VsExtension.csproj `
            /t:Restore /p:Configuration=Release /p:XaeShellVersion=$shellVersion
        if ($LASTEXITCODE -ne 0) { throw "XAE $shellVersion VSIX restore failed" }
        & $MSBuildPath editors/vs/TcTidy.VsExtension/TcTidy.VsExtension.csproj `
            /p:Configuration=Release /p:XaeShellVersion=$shellVersion `
            /p:DeployExtension=false
        if ($LASTEXITCODE -ne 0) { throw "XAE $shellVersion VSIX build failed" }

        $basePath = "editors/vs/TcTidy.VsExtension/bin/Release/Xae$shellVersion"
        $assembly = Join-Path $basePath 'TcTidy.VsExtension.dll'
        $vsix = Join-Path $basePath 'TcTidy.VsExtension.vsix'
        # Each shell loads assemblies with the same identity, so validate in a fresh process.
        $validationArguments = @(
            '-NoProfile',
            '-File', (Join-Path $PSScriptRoot 'validate-vsix.ps1'),
            '-Assembly', $assembly,
            '-Vsix', $vsix,
            '-ShellVersion', $shellVersion,
            '-ProductVersion', $ProductVersion,
            '-CargoMetadata', $metadata,
            '-CargoAbout', $about,
            '-CargoTree', $tree
        )
        & powershell.exe @validationArguments
        if ($LASTEXITCODE -ne 0) { throw "XAE $shellVersion VSIX validation failed" }

        [System.IO.Directory]::CreateDirectory($shellBuild.Payload) | Out-Null
        [System.IO.Compression.ZipFile]::ExtractToDirectory(
            (Resolve-Path -LiteralPath $vsix).Path, $shellBuild.Payload)
        $packed = Join-Path $shellBuild.Payload 'tctidy.exe'
        if ((Get-FileHash -LiteralPath $packed).Hash -cne $engineHash) {
            throw "XAE $shellVersion VSIX does not carry the CLI of this checkout"
        }
    }
} finally {
    Pop-Location
}
