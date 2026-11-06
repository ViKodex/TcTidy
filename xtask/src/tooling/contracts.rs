//! Source contracts for the installer and the release scripts, checked by
//! `cargo test`. They complement the executable release-validator tests.

use super::{array, json, root, string, text, Result};
use std::collections::BTreeMap;
use std::path::Path;

fn require(source: &str, markers: &[&str]) -> Result {
    for marker in markers {
        if !source.contains(marker) {
            return Err(format!("missing contract: {marker}"));
        }
    }
    Ok(())
}

fn forbid(source: &str, markers: &[&str]) -> Result {
    for marker in markers {
        if source.contains(marker) {
            return Err(format!("forbidden contract: {marker}"));
        }
    }
    Ok(())
}

fn ordered(source: &str, markers: &[&str]) -> Result {
    let mut position = 0;
    for marker in markers {
        position += source[position..]
            .find(marker)
            .ok_or_else(|| format!("missing or unordered contract: {marker}"))?
            + marker.len();
    }
    Ok(())
}

fn installer_contract(source: &str) -> Result {
    require(
        source,
        &[
            "ExpectedCacheRelative",
            "TryDeriveMergeMarker",
            "Win32_ComputerSystem",
            "Win32_UserAccount",
            "Win32_UserProfile",
            "TryBuildComponentCachePath",
            "ClearConsoleComponentCache",
            "CurUninstallStep = usUninstall",
            "uninsdeletevalue uninsdeletekeyifempty",
            "UninstallLogMode=overwrite",
            "UsePreviousAppDir=no",
            "PreferredShellIndex(ShellInstalledVersions,",
            "WizardSilent()",
            "RunSecurityFixture() and ShellSelectionRuleHolds()",
            "[InstallDelete]",
            "Type: files; Name: \"{app}\\Microsoft.VisualStudio.DpiAwareness.dll\"",
            "Subkey: \"Software\\ViKodex\\TcTidy\\XaeShell\\{{#AppId15}\"",
            "Subkey: \"Software\\ViKodex\\TcTidy\\XaeShell\\{{#AppId17}\"",
        ],
    )?;
    forbid(
        source,
        &[
            "[UninstallDelete]",
            "Type: filesandordirs; Name: \"{app}\"",
            "ExecAsOriginalUser",
            " MsgBox(",
            "#define TargetMetadata",
            "LoadStringsFromFile",
        ],
    )?;
    let start = source
        .find("function TryGetTrimmedWmiString")
        .ok_or("missing WMI string helper")?;
    let helper = source[start..].split("\nend;").next().unwrap();
    ordered(
        helper,
        &[
            "Result := False",
            "Text := ''",
            "if VarIsEmpty(Value) or VarIsNull(Value) then",
            "Text := Value",
            "Text := Trim(Text)",
            "Result := Text <> ''",
        ],
    )?;
    for (value, destination) in [
        ("Computer.UserName", "ConsoleAccount"),
        ("Account.SID", "Sid"),
        ("Profile.LocalPath", "ProfilePath"),
    ] {
        require(
            source,
            &[&format!("TryGetTrimmedWmiString({value}, {destination})")],
        )?;
        if source.matches(value).count() != 1 {
            return Err(format!("WMI Variant {value} must be staged once"));
        }
    }
    forbid(
        source,
        &[
            "Trim(Value)",
            "Trim(Computer.UserName)",
            "Trim(Account.SID)",
            "Trim(Profile.LocalPath)",
        ],
    )?;
    let start = source
        .find("function TryBuildComponentCachePath")
        .ok_or("missing cache path helper")?;
    ordered(
        source[start..].split("\nend;").next().unwrap(),
        &[
            "if not IsAbsoluteDrivePath(ProfilePath) then",
            "RemoveBackslashUnlessRoot(ExpandFileName(ProfilePath))",
            "if not IsAbsoluteDrivePath(NormalizedProfile) then",
        ],
    )
}

fn security_fixture(source: &str) -> Result {
    ordered(
        source,
        &[
            "$quotedSecurityResult = '\"' + $securityResult + '\"'",
            "$securityArguments = @(",
            "\"/SECURITYFIXTUREOUTPUT=$quotedSecurityResult\"",
            ") -join ' '",
            "$securityProcess = Start-Process",
            "-ArgumentList $securityArguments",
            "-Wait",
            "-PassThru",
            "$securityExitCode = $securityProcess.ExitCode",
            "$securityResultExists = Test-Path -LiteralPath $securityResult",
            "$securityResultContent = if ($securityResultExists)",
            "if (($securityExitCode -ne 1) -or",
            "($securityResultContent -cne 'PASS'))",
        ],
    )?;
    forbid(source, &["& $output /VERYSILENT", "$LASTEXITCODE -ne 1"])
}

fn check(root: &Path) -> Result {
    let load = |path: &str| text(&root.join(path)).map(|text| text.replace("\r\n", "\n"));
    let vsix_builder = load("scripts/release/build-vsix.ps1")?;
    let installer_tests = load("scripts/release/test-installer.ps1")?;
    security_fixture(&installer_tests)?;
    require(
        &installer_tests,
        &[
            "DiscoveryFixture15",
            "DiscoveryFixture17",
            "DiscoveryFixture1517",
            "SecurityFixture",
            "validate-installer.ps1",
            "validate-installer-target.ps1",
            "[string]$NumericVersion = '0.0.0.0'",
            "foreach ($shellVersion in @('15', '17'))",
            "-WindowStyle Hidden",
        ],
    )?;
    require(
        &vsix_builder,
        &[
            "Version = '15'",
            "Version = '17'",
            "/t:Restore",
            "/p:XaeShellVersion=$shellVersion",
            "/p:DeployExtension=false",
            "validate-vsix.ps1",
            "powershell.exe @validationArguments",
            "'-NoProfile'",
            "'-File'",
            "'-ProductVersion', $ProductVersion",
            "'-CargoMetadata', $metadata",
            "'-CargoAbout', $about",
            "'-CargoTree', $tree",
            "[System.IO.Compression.ZipFile]::ExtractToDirectory",
            "Resolve-Path -LiteralPath $vsix",
        ],
    )?;
    ordered(
        &vsix_builder,
        &[
            "/t:Restore",
            "/p:DeployExtension=false",
            "'validate-vsix.ps1'",
            "[System.IO.Compression.ZipFile]::ExtractToDirectory",
        ],
    )?;
    ordered(
        &vsix_builder,
        &[
            "cargo build -p tctidy-cli --release --locked",
            "-Destination editors/vs/TcTidy.VsExtension/tctidy.exe",
            "/t:Restore",
            "[System.IO.Compression.ZipFile]::ExtractToDirectory",
            "does not carry the CLI of this checkout",
        ],
    )?;
    let installer = load("editors/vs/installer/TcTidy.iss")?;
    installer_contract(&installer)?;
    let validator = load("scripts/release/validate-installer.ps1")?;
    require(
        &validator,
        &[
            "foreign sentinel",
            "legacy and current owned files",
            "PayloadFixture",
            "cargo xtask licenses installer",
            "Test-Path -LiteralPath $destination -PathType Leaf",
            "Get-FileHash -LiteralPath $_.FullName",
            "Get-FileHash -LiteralPath $destination",
            "Test-Path -LiteralPath $_ -PathType Leaf",
            "Get-ChildItem -LiteralPath $payload -File -Recurse",
            "Get-ChildItem -LiteralPath $installed -File",
            "legacy-only file survived current installer upgrade",
            "legacy-only file survived current installer uninstall",
        ],
    )?;
    let target_validator = load("scripts/release/validate-installer-target.ps1")?;
    require(
        &target_validator,
        &[
            "PayloadFixture1517",
            "/SHELL=15.0",
            "/SHELL=17.0",
            "Get-FileHash -LiteralPath $installed",
            "DisplayVersion",
        ],
    )?;
    let legacy = load("scripts/release/tests/fixtures/TcTidy-v0.3.2-unsafe.iss")?;
    require(
        &legacy,
        &["[UninstallDelete]", "Type: filesandordirs; Name: \"{app}\""],
    )?;
    let windows = load("scripts/release/build-windows.ps1")?;
    let linux = load("scripts/release/build-linux.sh")?;
    let candidate = load("scripts/release/validate-candidate.sh")?;
    for source in [&windows, &linux] {
        require(
            source,
            &[
                "cargo xtask licenses generate",
                "cargo xtask licenses compare",
                "cargo xtask licenses notice",
                "cargo xtask release check",
                "git status --porcelain",
                "git rev-parse HEAD",
            ],
        )?;
    }
    for source in [&windows, &linux, &candidate] {
        forbid(
            source,
            &[
                "gh api",
                "gh release",
                "gh run",
                "gh workflow",
                "GITHUB_TOKEN",
            ],
        )?;
    }
    require(
        &windows,
        &[
            "build-vsix.ps1",
            "Verify-Version.ps1",
            "CargoInventoryFile",
            "Get-FileHash -LiteralPath",
            "Resolve-Path -LiteralPath $OutputDirectory",
            "Resolve-Path -LiteralPath $NoticesFile",
            "Push-Location -LiteralPath $root",
            "[System.IO.Compression.ZipFile]::ExtractToDirectory",
            "Assert-ExactVersionText",
            "-Actual $versionInfo.ProductVersion",
            "-Actual $versionInfo.FileVersion",
            "tctidy-windows-provenance.json",
        ],
    )?;
    ordered(
        &windows,
        &[
            "Copy-Item -LiteralPath $installer.FullName -Destination $destination",
            "Get-FileHash -LiteralPath $destination -Algorithm SHA256",
            "Set-Content -LiteralPath \"$destination.sha256\"",
        ],
    )?;
    require(
        &linux,
        &[
            "sha256sum",
            "tctidy-linux-provenance.json",
            "cp \"$evidence/CARGO-DEPENDENCIES.json\" \"$stage/\"",
        ],
    )?;
    require(
        &candidate,
        &[
            "cargo xtask release verify-provenance",
            "cargo xtask release check-assets",
            "git status --porcelain",
            "git rev-parse HEAD",
        ],
    )?;
    let manifest = load(".github/release-assets.txt")?;
    forbid(&manifest, &["provenance"])?;
    require(
        &manifest,
        &[
            "TcTidySetup-{version}.exe",
            "TcTidySetup-{version}.exe.sha256",
        ],
    )?;
    let vsix_validator = load("scripts/release/validate-vsix.ps1")?;
    require(
        &vsix_validator,
        &[
            "cargo xtask vsix check",
            "cargo xtask licenses vsix",
            "does not name its commit",
            "Verify-Version.ps1",
            "@licenseArguments",
            "-FullVersion $ProductVersion",
            "-SystemVersion $systemVersion",
            "-AssemblyVersion $assemblyVersion",
            "VSPackage.resources",
            "_EmptyResource.resources",
        ],
    )?;
    let helper = load("scripts/release/assert-version-text.ps1")?;
    require(&helper, &["$Actual.Trim()", "-cne $Expected"])?;
    inventory_versions(root)
}

fn inventory_versions(root: &Path) -> Result {
    let project = text(&root.join("editors/vs/TcTidy.VsExtension/TcTidy.VsExtension.csproj"))?;
    let document = roxmltree::Document::parse(&project).map_err(|e| e.to_string())?;
    let references: BTreeMap<_, _> = document
        .descendants()
        .filter(|node| node.has_tag_name("PackageReference"))
        .filter_map(|node| Some((node.attribute("Include")?, node.attribute("Version")?)))
        .collect();
    let notice = text(
        &root
            .join("editors/vs/TcTidy.VsExtension")
            .join(super::licenses::EXTENSION_NOTICE),
    )?;
    for name in [
        "THIRD-PARTY-INVENTORY.xae15.json",
        "THIRD-PARTY-INVENTORY.json",
    ] {
        let inventory = json(&root.join("editors/vs/TcTidy.VsExtension").join(name))?;
        for entry in array(&inventory, "nugetFiles")? {
            if references.get(string(entry, "package")?) != Some(&string(entry, "version")?) {
                return Err(format!("{name} disagrees with NuGet project versions"));
            }
            super::licenses::nuget_attribution(&notice, entry)?;
        }
        for entry in array(&inventory, "cargoBundles")? {
            if entry["package"] != "tctidy-cli" || entry["target"] != "x86_64-pc-windows-msvc" {
                return Err("extension inventory has invalid Cargo bundle".into());
            }
        }
    }
    Ok(())
}

#[test]
#[cfg(unix)]
fn candidate_failure_preserves_evidence_but_success_consumes_it() {
    let source = text(&root().join("scripts/release/validate-candidate.sh")).unwrap();
    let start = source.find("cleanup() {").unwrap();
    let end =
        source[start..].find("trap cleanup EXIT").unwrap() + start + "trap cleanup EXIT".len();
    for success in [false, true] {
        let temporary = tempfile::tempdir().unwrap();
        for directory in ["candidate", "provenance", "extracted"] {
            std::fs::create_dir(temporary.path().join(directory)).unwrap();
        }
        for name in [
            "tctidy-linux-provenance.json",
            "tctidy-windows-provenance.json",
        ] {
            std::fs::write(
                temporary.path().join("provenance").join(name),
                b"source evidence",
            )
            .unwrap();
        }
        let script = format!(
            "provenance_dir=provenance\ncandidate_dir=candidate\nextracted=extracted\n{}\n{}\n",
            &source[start..end],
            if success { "true" } else { "false" }
        );
        let status = std::process::Command::new("bash")
            .args(["-c", &script])
            .current_dir(temporary.path())
            .status()
            .unwrap();
        assert_eq!(status.success(), success);
        for name in [
            "tctidy-linux-provenance.json",
            "tctidy-windows-provenance.json",
        ] {
            let path = temporary.path().join("candidate").join(name);
            if success {
                assert!(!path.exists());
            } else {
                assert_eq!(std::fs::read(path).unwrap(), b"source evidence");
            }
        }
        assert!(!temporary.path().join("provenance").exists());
    }
}

#[test]
fn repository_contracts() {
    check(&root()).unwrap();
}

#[test]
fn rejects_unsafe_wmi_and_cache_changes() {
    let source = text(&root().join("editors/vs/installer/TcTidy.iss")).unwrap();
    installer_contract(&source).unwrap();
    for marker in [
        "Text := Trim(Text)",
        "if not IsAbsoluteDrivePath(ProfilePath) then",
        "if not IsAbsoluteDrivePath(NormalizedProfile) then",
        "UninstallLogMode=overwrite",
    ] {
        assert!(
            installer_contract(&source.replacen(marker, "removed", 1)).is_err(),
            "{marker}"
        );
    }
    assert!(
        installer_contract(&source.replace("Text := Trim(Text)", "Text := Trim(Value)")).is_err()
    );
}

#[test]
fn security_fixture_requires_quoted_arguments_and_process_wait() {
    let source = text(&root().join("scripts/release/test-installer.ps1")).unwrap();
    security_fixture(&source).unwrap();
    for marker in [
        "\"/SECURITYFIXTUREOUTPUT=$quotedSecurityResult\"",
        "-Wait",
        "-PassThru",
        "$securityProcess.ExitCode",
    ] {
        assert!(
            security_fixture(&source.replacen(marker, "removed", 1)).is_err(),
            "{marker}"
        );
    }
}

#[test]
fn ordered_markers_must_appear_in_sequence() {
    assert!(ordered("publish upload draft", &["draft", "upload", "publish"]).is_err());
    ordered(
        "draft check upload verify publish",
        &["draft", "check", "upload", "verify", "publish"],
    )
    .unwrap();
}
