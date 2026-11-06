//! Product-version and release-asset maintenance.

use semver::Version;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::version_projection::VersionProjection;
#[cfg(test)]
use crate::version_sources::replace_call_value;
use crate::version_sources::{
    attribute_value, call_value, installed_product_version, parse_quoted_assignment, replace_once,
    section_bounds, section_value, unique_line, vsix_updates, ASSEMBLY_INFO, PACKAGE_SOURCE,
    ROOT_MANIFEST, VSIX_MANIFEST, VSIX_MANIFEST_XAE15,
};

const LOCKFILE: &str = "Cargo.lock";

const ASSET_MANIFEST: &str = ".github/release-assets.txt";
const CHANGELOG: &str = "CHANGELOG.md";
const INSTALLER_SOURCE: &str = "editors/vs/installer/TcTidy.iss";
const THIRD_PARTY_INVENTORY: &str = "editors/vs/TcTidy.VsExtension/THIRD-PARTY-INVENTORY.json";
const THIRD_PARTY_INVENTORY_XAE15: &str =
    "editors/vs/TcTidy.VsExtension/THIRD-PARTY-INVENTORY.xae15.json";
const RELEASE_WORKFLOW: &str = ".github/workflows/release.yml";
const GIT_REPOSITORY_LOCAL_ENVIRONMENT: &[&str] = &[
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_CONFIG",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_PARAMETERS",
    "GIT_DIR",
    "GIT_GRAFT_FILE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_OBJECT_DIRECTORY",
    "GIT_PREFIX",
    "GIT_REPLACE_REF_BASE",
    "GIT_SHALLOW_FILE",
    "GIT_WORK_TREE",
];

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let root = repo_root()?;
    match args {
        [command, tag, sha, linux, windows] if command == "verify-provenance" => {
            crate::tooling::integrity::provenance(tag, sha, Path::new(linux), Path::new(windows))
        }
        [command, tag, directory, response] if command == "verify-upload" => {
            crate::tooling::integrity::uploaded(tag, Path::new(directory), Path::new(response))
        }
        [command] if command == "version" => {
            println!("{}", read_workspace_version(&root)?);
            Ok(())
        }
        [command] if command == "check" => {
            let version = check_repo(&root, None)?;
            println!("release metadata OK: {version}");
            Ok(())
        }
        [command, flag, tag] if command == "check" && flag == "--tag" => {
            let version = check_repo(&root, Some(tag))?;
            println!("release metadata OK: {version} ({tag})");
            Ok(())
        }
        [command, version] if command == "prepare" => prepare(&root, version),
        [command, directory, flag, tag] if command == "check-assets" && flag == "--tag" => {
            check_assets(&root, Path::new(directory), tag)
        }
        _ => Err(
            "usage: cargo xtask release <version | check [--tag vX.Y.Z] | \
             prepare X.Y.Z | check-assets DIR --tag vX.Y.Z | \
             verify-provenance TAG SHA LINUX_JSON WINDOWS_JSON | verify-upload TAG DIR RESPONSE_JSON>"
                .to_string(),
        ),
    }
}

fn repo_root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask has no repository parent".to_string())
}

fn parse_product_version(raw: &str) -> Result<Version, String> {
    let version =
        Version::parse(raw).map_err(|error| format!("invalid SemVer {raw:?}: {error}"))?;
    if !version.build.is_empty() {
        return Err("build metadata is not supported for TcTidy product versions".to_string());
    }
    if !version.pre.is_empty() {
        let parts: Vec<_> = version.pre.as_str().split('.').collect();
        let valid_kind = matches!(parts.first(), Some(&"alpha" | &"beta" | &"rc"));
        let valid_number = parts
            .get(1)
            .and_then(|value| value.parse::<u32>().ok())
            .is_some_and(|value| value > 0);
        if parts.len() != 2 || !valid_kind || !valid_number {
            return Err(
                "prereleases must use alpha.N, beta.N, or rc.N with N greater than zero"
                    .to_string(),
            );
        }
    }
    VersionProjection::from_semver(&version)?;
    Ok(version)
}

fn read_workspace_version(root: &Path) -> Result<Version, String> {
    workspace_version(&read(root.join(ROOT_MANIFEST))?)
}

fn workspace_version(input: &str) -> Result<Version, String> {
    let value = crate::version_sources::workspace_version(input)?;
    parse_product_version(&value)
}

fn replace_workspace_version(input: &str, version: &Version) -> Result<String, String> {
    replace_section_value(input, "workspace.package", "version", &version.to_string())
}

fn replace_section_value(
    input: &str,
    section: &str,
    key: &str,
    value: &str,
) -> Result<String, String> {
    let (start, end) = section_bounds(input, section)?;
    let content = &input[start..end];
    let old_line = unique_line(content, |line| parse_quoted_assignment(line, key).is_some())?;
    let indent_len = old_line.len() - old_line.trim_start().len();
    let new_line = format!("{}{key} = \"{value}\"", &old_line[..indent_len]);
    let replaced = replace_once(
        content,
        old_line,
        &new_line,
        &format!("{key} in [{section}]"),
    )?;
    Ok(format!("{}{}{}", &input[..start], replaced, &input[end..]))
}

fn json_string_property(line: &str, key: &str) -> Option<String> {
    let prefix = format!("\"{key}\": \"");
    line.trim()
        .trim_end_matches(',')
        .strip_prefix(&prefix)?
        .strip_suffix('"')
        .map(str::to_string)
}

fn cargo_inventory_bounds(input: &str) -> Result<(usize, usize), String> {
    let marker = "\"cargoBundles\": [";
    let start = input
        .find(marker)
        .ok_or_else(|| "Cargo inventory has no cargoBundles array".to_string())?
        + marker.len();
    let end = input[start..]
        .find("\n  ]")
        .map(|offset| start + offset)
        .ok_or_else(|| "Cargo inventory has an unterminated cargoBundles array".to_string())?;
    Ok((start, end))
}

fn cargo_inventory_version(input: &str) -> Result<String, String> {
    let (start, end) = cargo_inventory_bounds(input)?;
    let line = unique_line(&input[start..end], |line| {
        json_string_property(line, "version").is_some()
    })?;
    json_string_property(line, "version")
        .ok_or_else(|| "Cargo inventory bundle has no version".to_string())
}

fn replace_cargo_inventory_version(input: &str, version: &Version) -> Result<String, String> {
    let (start, end) = cargo_inventory_bounds(input)?;
    let current = cargo_inventory_version(input)?;
    let replaced = replace_once(
        &input[start..end],
        &format!("\"version\": \"{current}\""),
        &format!("\"version\": \"{version}\""),
        "Cargo inventory version",
    )?;
    Ok(format!("{}{}{}", &input[..start], replaced, &input[end..]))
}

fn workspace_members(root_manifest: &str) -> Result<Vec<String>, String> {
    let (start, end) = section_bounds(root_manifest, "workspace")?;
    let section = &root_manifest[start..end];
    let list_start = section
        .find("members = [")
        .ok_or_else(|| "missing workspace members list".to_string())?;
    let list = &section[list_start + "members = [".len()..];
    let list_end = list
        .find(']')
        .ok_or_else(|| "unterminated workspace members list".to_string())?;
    let members: Vec<String> = list[..list_end]
        .lines()
        .filter_map(|line| {
            let value = line.trim().trim_end_matches(',');
            value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .map(str::to_string)
        })
        .collect();
    if members.is_empty() {
        return Err("workspace members list is empty".to_string());
    }
    Ok(members)
}

fn workspace_package_names(root: &Path, root_manifest: &str) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for member in workspace_members(root_manifest)? {
        let manifest_path = root.join(&member).join("Cargo.toml");
        let manifest = read(&manifest_path)?;
        let name = section_value(&manifest, "package", "name")?;
        let (start, end) = section_bounds(&manifest, "package")?;
        let version_inherited = manifest[start..end]
            .lines()
            .any(|line| line.trim() == "version.workspace = true");
        if !version_inherited {
            return Err(format!(
                "{} must inherit workspace.package.version",
                manifest_path.display()
            ));
        }
        names.push(name);
    }
    names.sort();
    names.dedup();
    Ok(names)
}

fn replace_lock_versions(
    input: &str,
    packages: &[&str],
    version: &Version,
) -> Result<String, String> {
    let expected: BTreeSet<&str> = packages.iter().copied().collect();
    let mut found = BTreeSet::new();
    let mut output = String::new();
    let mut blocks = input.split("[[package]]");
    output.push_str(blocks.next().unwrap_or_default());
    for block in blocks {
        output.push_str("[[package]]");
        let name = top_level_value(block, "name")?;
        if expected.contains(name.as_str()) {
            if !found.insert(name.clone()) {
                return Err(format!("duplicate workspace package {name} in Cargo.lock"));
            }
            let old_line = unique_line(block, |line| {
                parse_quoted_assignment(line, "version").is_some()
            })?;
            let new_line = format!("version = \"{version}\"");
            output.push_str(&replace_once(
                block,
                old_line,
                &new_line,
                &format!("version for {name} in Cargo.lock"),
            )?);
        } else {
            output.push_str(block);
        }
    }
    let missing: Vec<_> = expected
        .iter()
        .filter(|name| !found.contains(**name))
        .copied()
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "workspace packages missing from Cargo.lock: {}",
            missing.join(", ")
        ));
    }
    Ok(output)
}

fn lock_versions(input: &str, packages: &[String]) -> Result<BTreeMap<String, Version>, String> {
    let expected: BTreeSet<_> = packages.iter().map(String::as_str).collect();
    let mut versions = BTreeMap::new();
    for block in input.split("[[package]]").skip(1) {
        let name = top_level_value(block, "name")?;
        if !expected.contains(name.as_str()) {
            continue;
        }
        let raw = top_level_value(block, "version")?;
        let version = Version::parse(&raw)
            .map_err(|error| format!("invalid Cargo.lock version for {name}: {error}"))?;
        if versions.insert(name.clone(), version).is_some() {
            return Err(format!("duplicate package {name} in Cargo.lock"));
        }
    }
    Ok(versions)
}

fn top_level_value(block: &str, key: &str) -> Result<String, String> {
    block
        .lines()
        .take_while(|line| !line.trim_start().starts_with('['))
        .find_map(|line| parse_quoted_assignment(line, key))
        .ok_or_else(|| format!("missing {key} in package block"))
}

fn cut_changelog(
    input: &str,
    current: &Version,
    next: &Version,
    date: &str,
) -> Result<String, String> {
    if next <= current {
        return Err(format!("new version {next} must be greater than {current}"));
    }
    if !current.pre.is_empty() {
        if !same_release_line(current, next) {
            return Err(format!(
                "prerelease {current} must be promoted within the same X.Y.Z release line"
            ));
        }
        return promote_changelog(input, current, next, date);
    }
    let marker = "## [Unreleased]";
    let marker_start = input
        .find(marker)
        .ok_or_else(|| "CHANGELOG.md has no Unreleased section".to_string())?;
    if input[marker_start + marker.len()..].contains(marker) {
        return Err("CHANGELOG.md has duplicate Unreleased sections".to_string());
    }
    let body_start = marker_start + marker.len();
    let next_heading = input[body_start..]
        .find("\n## [")
        .map(|offset| body_start + offset)
        .ok_or_else(|| "CHANGELOG.md has no released section after Unreleased".to_string())?;
    if input[body_start..next_heading].trim().is_empty() {
        return Err("cannot prepare a release with an empty Unreleased section".to_string());
    }
    let release_heading = format!("{marker}\n\n## [{next}] - {date}");
    let mut output = replace_once(input, marker, &release_heading, "Unreleased heading")?;

    let old_link =
        format!("[Unreleased]: https://github.com/ViKodex/TcTidy/compare/v{current}...HEAD");
    let new_links = format!(
        "[Unreleased]: https://github.com/ViKodex/TcTidy/compare/v{next}...HEAD\n\
         [{next}]: https://github.com/ViKodex/TcTidy/releases/tag/v{next}"
    );
    output = replace_once(&output, &old_link, &new_links, "Unreleased comparison link")?;
    Ok(output)
}

fn same_release_line(left: &Version, right: &Version) -> bool {
    left.major == right.major && left.minor == right.minor && left.patch == right.patch
}

fn promote_changelog(
    input: &str,
    current: &Version,
    next: &Version,
    date: &str,
) -> Result<String, String> {
    let marker = "## [Unreleased]";
    let marker_start = input
        .find(marker)
        .ok_or_else(|| "CHANGELOG.md has no Unreleased section".to_string())?;
    let unreleased_start = marker_start + marker.len();
    let current_start = input[unreleased_start..]
        .find("\n## [")
        .map(|offset| unreleased_start + offset + 1)
        .ok_or_else(|| "CHANGELOG.md has no candidate section after Unreleased".to_string())?;
    let current_marker = format!("## [{current}] - ");
    if !input[current_start..].starts_with(&current_marker) {
        return Err(format!(
            "the first changelog section after Unreleased is not {current}"
        ));
    }
    let current_body_start = input[current_start..]
        .find('\n')
        .map(|offset| current_start + offset + 1)
        .ok_or_else(|| format!("unterminated changelog heading for {current}"))?;
    let older_start = input[current_body_start..]
        .find("\n## [")
        .map(|offset| current_body_start + offset)
        .ok_or_else(|| format!("CHANGELOG.md has no released section after {current}"))?;

    let unreleased = input[unreleased_start..current_start].trim();
    let candidate = input[current_body_start..older_start].trim();
    let merged = merge_changelog_categories(candidate, unreleased)?;
    let promoted = format!(
        "{}\n\n## [{next}] - {date}\n\n{merged}\n{}",
        &input[..unreleased_start],
        &input[older_start..]
    );

    let old_links = format!(
        "[Unreleased]: https://github.com/ViKodex/TcTidy/compare/v{current}...HEAD\n\
         [{current}]: https://github.com/ViKodex/TcTidy/releases/tag/v{current}"
    );
    let new_links = format!(
        "[Unreleased]: https://github.com/ViKodex/TcTidy/compare/v{next}...HEAD\n\
         [{next}]: https://github.com/ViKodex/TcTidy/releases/tag/v{next}"
    );
    replace_once(
        &promoted,
        &old_links,
        &new_links,
        "candidate changelog links",
    )
}

fn merge_changelog_categories(base: &str, delta: &str) -> Result<String, String> {
    let mut categories = parse_changelog_categories(base)?;
    for (name, content) in parse_changelog_categories(delta)? {
        if let Some((_, existing)) = categories.iter_mut().find(|(current, _)| current == &name) {
            if !content.is_empty() {
                if !existing.is_empty() {
                    existing.push('\n');
                }
                existing.push_str(&content);
            }
        } else {
            categories.push((name, content));
        }
    }
    if categories.is_empty() {
        return Err("candidate changelog section is empty".to_string());
    }
    Ok(categories
        .into_iter()
        .map(|(name, content)| format!("### {name}\n\n{content}"))
        .collect::<Vec<_>>()
        .join("\n\n"))
}

fn parse_changelog_categories(body: &str) -> Result<Vec<(String, String)>, String> {
    let mut categories = Vec::new();
    let mut current: Option<(String, String)> = None;
    let mut seen = BTreeSet::new();
    for line in body.trim().lines() {
        if let Some(name) = line.strip_prefix("### ") {
            if let Some((name, content)) = current.take() {
                categories.push((name, content.trim().to_string()));
            }
            if name.is_empty() || !seen.insert(name.to_string()) {
                return Err(format!("invalid or duplicate changelog category {name:?}"));
            }
            current = Some((name.to_string(), String::new()));
        } else if let Some((_, content)) = current.as_mut() {
            content.push_str(line);
            content.push('\n');
        } else if !line.trim().is_empty() {
            return Err("changelog entries must be grouped under ### categories".to_string());
        }
    }
    if let Some((name, content)) = current {
        categories.push((name, content.trim().to_string()));
    }
    Ok(categories)
}

fn expected_assets(manifest: &str, tag: &str, version: &Version) -> Result<Vec<String>, String> {
    let mut assets = Vec::new();
    let mut seen = BTreeSet::new();
    for line in manifest.lines() {
        let template = line.trim();
        if template.is_empty() || template.starts_with('#') {
            continue;
        }
        let asset = template
            .replace("{tag}", tag)
            .replace("{version}", &version.to_string());
        if asset.contains('{') || asset.contains('}') {
            return Err(format!(
                "unknown placeholder in asset template {template:?}"
            ));
        }
        if asset.contains('/') || asset.contains('\\') || asset == "." || asset == ".." {
            return Err(format!("asset must be a root-level filename: {asset:?}"));
        }
        if !seen.insert(asset.clone()) {
            return Err(format!("duplicate release asset {asset}"));
        }
        assets.push(asset);
    }
    if assets.is_empty() {
        return Err("release asset manifest is empty".to_string());
    }
    Ok(assets)
}

fn validate_installer_version_contract(input: &str) -> Result<(), String> {
    for required in [
        "AppVersion={#AppVersion}",
        "VersionInfoVersion={#NumericVersion}",
        "VersionInfoProductVersion={#NumericVersion}",
        "VersionInfoProductTextVersion={#AppVersion}",
        "OutputBaseFilename=TcTidySetup-{#AppVersion}",
    ] {
        if !input.contains(required) {
            return Err(format!(
                "installer is missing required version field {required}"
            ));
        }
    }
    Ok(())
}

fn validate_release_workflow_contract(input: &str) -> Result<(), String> {
    let normalized = input.replace("\r\n", "\n");
    let input = normalized.as_str();

    if input.contains("workflow_dispatch:") || input.contains("pull_request:") {
        return Err("release workflow must be triggered only by stable tags".to_string());
    }
    if !input.contains("tags:\n      - 'v[0-9]+.[0-9]+.[0-9]+'") {
        return Err("release workflow must declare the stable tag trigger".to_string());
    }

    let lint = workflow_job(input, "lint")?;
    if !lint.contains("cargo run -p xtask --locked -- keywords --check") {
        return Err("release lint must check the generated keyword table".to_string());
    }

    let linux_assets = workflow_job(input, "linux-assets")?;
    if !linux_assets.contains("scripts/release/build-linux.sh") {
        return Err("release must reuse the local Linux candidate builder".to_string());
    }
    let windows_assets = workflow_job(input, "windows-assets")?;
    if !windows_assets.contains("scripts/release/build-windows.ps1") {
        return Err("release must reuse the local Windows candidate builder".to_string());
    }
    let validation = workflow_job(input, "validate-assets")?;
    if !validation.contains("scripts/release/validate-candidate.sh") {
        return Err("release must reuse the local candidate validator".to_string());
    }

    let validation_needs = workflow_needs(validation)?;
    for required in ["lint", "linux-assets", "windows-assets"] {
        if !validation_needs.contains(required) {
            return Err(format!(
                "release asset validation must depend on {required}"
            ));
        }
    }
    let publish_needs = workflow_needs(workflow_job(input, "publish")?)?;
    if !publish_needs.contains("validate-assets") {
        return Err("release publication must depend on validated assets".to_string());
    }
    Ok(())
}

fn workflow_job<'a>(input: &'a str, name: &str) -> Result<&'a str, String> {
    let header = format!("  {name}:\n");
    let start = if input.starts_with(&header) {
        0
    } else {
        input
            .find(&format!("\n{header}"))
            .map(|index| index + 1)
            .ok_or_else(|| format!("release workflow has no {name} job"))?
    };
    let body_start = start + header.len();
    let mut cursor = body_start;
    let mut end = input.len();
    for line in input[body_start..].split_inclusive('\n') {
        if line.starts_with("  ") && !line.starts_with("    ") && line.trim_end().ends_with(':') {
            end = cursor;
            break;
        }
        cursor += line.len();
    }
    Ok(&input[start..end])
}

fn workflow_needs(job: &str) -> Result<BTreeSet<&str>, String> {
    let line = unique_line(job, |line| line.trim_start().starts_with("needs: ["))?;
    let value = line
        .trim()
        .strip_prefix("needs: [")
        .and_then(|value| value.strip_suffix(']'))
        .ok_or_else(|| "release workflow needs must use an inline list".to_string())?;
    Ok(value.split(',').map(str::trim).collect())
}

fn check_repo(root: &Path, tag: Option<&str>) -> Result<Version, String> {
    let root_manifest = read(root.join(ROOT_MANIFEST))?;
    let version = workspace_version(&root_manifest)?;
    let package_names = workspace_package_names(root, &root_manifest)?;
    let lock = read(root.join(LOCKFILE))?;
    let locked = lock_versions(&lock, &package_names)?;
    for package in &package_names {
        match locked.get(package) {
            Some(locked_version) if locked_version == &version => {}
            Some(locked_version) => {
                return Err(format!(
                    "Cargo.lock has {package} {locked_version}, expected {version}"
                ));
            }
            None => return Err(format!("Cargo.lock is missing workspace package {package}")),
        }
    }

    let projection = VersionProjection::from_semver(&version)?;
    for (label, relative) in [
        ("XAE 17 VSIX manifest version", VSIX_MANIFEST),
        ("XAE 15 VSIX manifest version", VSIX_MANIFEST_XAE15),
    ] {
        let vsix = read(root.join(relative))?;
        require_equal(
            label,
            &attribute_value(&vsix, "<Identity ", "Version")?,
            &projection.system,
        )?;
    }
    let assembly_info = read(root.join(ASSEMBLY_INFO))?;
    require_equal(
        "AssemblyVersion",
        &call_value(&assembly_info, "AssemblyVersion(\"")?,
        &projection.assembly,
    )?;
    require_equal(
        "AssemblyFileVersion",
        &call_value(&assembly_info, "AssemblyFileVersion(\"")?,
        &projection.assembly,
    )?;
    require_equal(
        "AssemblyInformationalVersion",
        &call_value(&assembly_info, "AssemblyInformationalVersion(\"")?,
        &projection.full,
    )?;
    let package_source = read(root.join(PACKAGE_SOURCE))?;
    require_equal(
        "InstalledProductRegistration version",
        &installed_product_version(&package_source)?,
        &projection.full,
    )?;
    for (label, relative) in [
        ("XAE 17 Cargo inventory version", THIRD_PARTY_INVENTORY),
        (
            "XAE 15 Cargo inventory version",
            THIRD_PARTY_INVENTORY_XAE15,
        ),
    ] {
        let cargo_inventory = read(root.join(relative))?;
        require_equal(
            label,
            &cargo_inventory_version(&cargo_inventory)?,
            &projection.full,
        )?;
    }
    let installer = read(root.join(INSTALLER_SOURCE))?;
    validate_installer_version_contract(&installer)?;
    let changelog = read(root.join(CHANGELOG))?;
    if !changelog.contains(&format!("## [{version}] - ")) {
        return Err(format!("CHANGELOG.md has no release heading for {version}"));
    }
    validate_release_workflow_contract(&read(root.join(RELEASE_WORKFLOW))?)?;

    if let Some(tag) = tag {
        let tagged = tag
            .strip_prefix('v')
            .ok_or_else(|| format!("release tag must start with v: {tag}"))?;
        let tag_version = parse_product_version(tagged)?;
        if tag_version != version {
            return Err(format!(
                "tag {tag} certifies {tag_version}, but Cargo.toml contains {version}"
            ));
        }
        if !version.pre.is_empty() {
            return Err(
                "prereleases are CI artifacts only and cannot trigger a GitHub Release".to_string(),
            );
        }
    }
    Ok(version)
}

fn require_equal(label: &str, actual: &str, expected: &str) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{label} is {actual:?}, expected {expected:?}"))
    }
}

fn prepare(root: &Path, raw_version: &str) -> Result<(), String> {
    ensure_clean(root)?;
    let current = check_repo(root, None)?;
    let next = parse_product_version(raw_version)?;
    if next <= current {
        return Err(format!("new version {next} must be greater than {current}"));
    }
    let root_manifest = read(root.join(ROOT_MANIFEST))?;
    let package_names = workspace_package_names(root, &root_manifest)?;
    let package_refs: Vec<_> = package_names.iter().map(String::as_str).collect();
    let date = utc_date()?;
    let projection = VersionProjection::from_semver(&next)?;

    let mut updates = vec![
        (
            ROOT_MANIFEST,
            replace_workspace_version(&root_manifest, &next)?,
        ),
        (
            LOCKFILE,
            replace_lock_versions(&read(root.join(LOCKFILE))?, &package_refs, &next)?,
        ),
        (
            CHANGELOG,
            cut_changelog(&read(root.join(CHANGELOG))?, &current, &next, &date)?,
        ),
        (
            THIRD_PARTY_INVENTORY,
            replace_cargo_inventory_version(&read(root.join(THIRD_PARTY_INVENTORY))?, &next)?,
        ),
        (
            THIRD_PARTY_INVENTORY_XAE15,
            replace_cargo_inventory_version(&read(root.join(THIRD_PARTY_INVENTORY_XAE15))?, &next)?,
        ),
    ];
    updates.extend(vsix_updates(root, &projection)?);

    for (relative, content) in updates {
        fs::write(root.join(relative), content)
            .map_err(|error| format!("cannot write {relative}: {error}"))?;
    }
    check_repo(root, None)?;
    println!(
        "prepared TcTidy {next} ({} for numeric VSIX metadata)",
        projection.system
    );
    Ok(())
}

fn ensure_clean(root: &Path) -> Result<(), String> {
    let output = isolated_git_command(root)
        .args(["status", "--porcelain"])
        .output()
        .map_err(|error| format!("cannot run git status: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    if !output.stdout.is_empty() {
        return Err("release prepare requires a clean worktree".to_string());
    }
    Ok(())
}

fn isolated_git_command(root: &Path) -> Command {
    let mut environment: BTreeSet<String> = GIT_REPOSITORY_LOCAL_ENVIRONMENT
        .iter()
        .map(|variable| (*variable).to_string())
        .collect();
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--local-env-vars"])
        .output()
    {
        if output.status.success() {
            environment.extend(
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .filter(|variable| variable.starts_with("GIT_"))
                    .map(str::to_string),
            );
        }
    }

    let mut command = Command::new("git");
    command.current_dir(root);
    for variable in environment {
        command.env_remove(variable);
    }
    command
}

fn utc_date() -> Result<String, String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?
        .as_secs();
    let days = i64::try_from(seconds / 86_400)
        .map_err(|_| "system date is outside the supported range".to_string())?;
    let (year, month, day) = civil_from_days(days);
    Ok(format!("{year:04}-{month:02}-{day:02}"))
}

// Howard Hinnant's civil-from-days algorithm, with day zero at 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month as u32, day as u32)
}

fn check_assets(root: &Path, directory: &Path, tag: &str) -> Result<(), String> {
    let tagged = tag
        .strip_prefix('v')
        .ok_or_else(|| format!("release tag must start with v: {tag}"))?;
    let version = parse_product_version(tagged)?;
    let manifest = read(root.join(ASSET_MANIFEST))?;
    let expected: BTreeSet<_> = expected_assets(&manifest, tag, &version)?
        .into_iter()
        .collect();
    let mut actual = BTreeSet::new();
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("cannot read {}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("cannot read release asset entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?;
        if !file_type.is_file() {
            return Err(format!(
                "release bundle contains non-file entry {}",
                entry.path().display()
            ));
        }
        let metadata = entry
            .metadata()
            .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?;
        if metadata.len() == 0 {
            return Err(format!("release asset {} is empty", entry.path().display()));
        }
        actual.insert(entry.file_name().to_string_lossy().into_owned());
    }
    if actual != expected {
        let missing: Vec<_> = expected.difference(&actual).cloned().collect();
        let extra: Vec<_> = actual.difference(&expected).cloned().collect();
        return Err(format!(
            "release asset set mismatch; missing: [{}]; extra: [{}]",
            missing.join(", "),
            extra.join(", ")
        ));
    }

    for checksum in expected.iter().filter(|name| name.ends_with(".sha256")) {
        validate_checksum_binding(directory, checksum)?;
    }
    println!("release asset bundle OK: {} files", expected.len());
    Ok(())
}

fn validate_checksum_binding(directory: &Path, checksum: &str) -> Result<(), String> {
    let asset = checksum
        .strip_suffix(".sha256")
        .ok_or_else(|| format!("checksum sidecar has an invalid name: {checksum}"))?;
    let contents = read(directory.join(checksum))?;
    let mut lines = contents.lines();
    let line = lines
        .next()
        .ok_or_else(|| format!("checksum sidecar is empty: {checksum}"))?;
    if lines.next().is_some() {
        return Err(format!(
            "checksum sidecar must contain exactly one entry: {checksum}"
        ));
    }
    if line.len() < 66 {
        return Err(format!("checksum sidecar is malformed: {checksum}"));
    }
    let digest = line
        .get(..64)
        .ok_or_else(|| format!("checksum sidecar has an invalid digest: {checksum}"))?;
    let remainder = &line[64..];
    if !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "checksum sidecar has an invalid digest: {checksum}"
        ));
    }
    let target = remainder
        .strip_prefix("  ")
        .or_else(|| remainder.strip_prefix(" *"))
        .ok_or_else(|| format!("checksum sidecar is malformed: {checksum}"))?;
    if target != asset {
        return Err(format!(
            "checksum sidecar {checksum} targets {target}, expected {asset}"
        ));
    }
    let actual = crate::tooling::file_digest(&directory.join(asset))?;
    if !digest.eq_ignore_ascii_case(&actual) {
        return Err(format!("checksum verification failed for {checksum}"));
    }
    Ok(())
}

fn read(path: impl AsRef<Path>) -> Result<String, String> {
    let path = path.as_ref();
    fs::read_to_string(path).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_CHILD_ROOT: &str = "TCTIDY_FIXTURE_CHILD_ROOT";

    #[test]
    fn version_policy_and_vsix_projection() {
        let stable = parse_product_version("0.3.3").unwrap();
        let stable_projection = VersionProjection::from_semver(&stable).unwrap();
        assert_eq!(stable_projection.system, "0.3.3");
        assert_eq!(stable_projection.assembly, "0.3.3.0");

        let preview = parse_product_version("0.4.0-rc.2").unwrap();
        let preview_projection = VersionProjection::from_semver(&preview).unwrap();
        assert_eq!(preview_projection.system, "0.4.0");
        assert_eq!(preview_projection.assembly, "0.4.0.0");
        assert_eq!(preview_projection.full, "0.4.0-rc.2");

        assert!(parse_product_version("0.3.1.5").is_err());
        assert!(parse_product_version("0.4.0-preview.1").is_err());
        assert!(parse_product_version("0.4.0+rebuilt").is_err());

        assert!(parse_product_version("65534.65534.65534").is_ok());
        let error = parse_product_version("65535.0.0").unwrap_err();
        assert!(error.contains("65534"), "{error}");
    }

    #[test]
    fn prerelease_prepare_and_check_keep_full_product_registration_version() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tctidy-prerelease-metadata-{nonce}"));
        write_release_metadata_fixture(&root, "0.3.2", "0.3.2");
        initialize_fixture_repository(&root);

        prepare(&root, "0.4.0-rc.1").unwrap();

        assert_eq!(check_repo(&root, None).unwrap().to_string(), "0.4.0-rc.1");
        let inventory = read(root.join(THIRD_PARTY_INVENTORY)).unwrap();
        assert!(inventory.contains("\"version\": \"0.4.0-rc.1\""));
        fs::write(
            root.join(THIRD_PARTY_INVENTORY),
            inventory.replacen("\"version\": \"0.4.0-rc.1\"", "\"version\": \"0.3.2\"", 1),
        )
        .unwrap();
        let error = check_repo(&root, None).unwrap_err();
        assert!(error.contains("Cargo inventory"), "{error}");
        fs::write(root.join(THIRD_PARTY_INVENTORY), &inventory).unwrap();
        assert_eq!(
            installed_product_version(&read(root.join(PACKAGE_SOURCE)).unwrap()).unwrap(),
            "0.4.0-rc.1"
        );

        fs::write(
            root.join(PACKAGE_SOURCE),
            "[InstalledProductRegistration(\"TcTidy for TwinCAT\", \"Format TwinCAT Structured Text with tctidy.\", \"0.4.0\")]\n",
        )
        .unwrap();
        let error = check_repo(&root, None).unwrap_err();
        assert!(error.contains("expected \"0.4.0-rc.1\""), "{error}");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn release_prepare_rejects_dirty_worktree_without_changing_versions() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        write_release_metadata_fixture(root, "0.3.2", "0.3.2");
        initialize_fixture_repository(root);
        fs::write(root.join("untracked.txt"), "work in progress\n").unwrap();
        let expected = read(root.join(ROOT_MANIFEST)).unwrap();

        assert!(prepare(root, "0.4.0-rc.1").is_err());
        assert_eq!(read(root.join(ROOT_MANIFEST)).unwrap(), expected);
        assert_eq!(check_repo(root, None).unwrap().to_string(), "0.3.2");
    }

    #[test]
    fn fixture_git_commands_ignore_hook_repository_environment() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tctidy-git-environment-{nonce}"));
        let hook_repository = root.join("hook-repository");
        let fixture_repository = root.join("fixture-repository");

        fs::create_dir_all(&hook_repository).unwrap();
        fs::write(hook_repository.join("tracked.txt"), "hook repository\n").unwrap();
        initialize_fixture_repository(&hook_repository);
        let hook_head = git_output(&hook_repository, &["rev-parse", "HEAD"]);

        fs::create_dir_all(&fixture_repository).unwrap();
        fs::write(
            fixture_repository.join("tracked.txt"),
            "fixture repository\n",
        )
        .unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "release::tests::fixture_repository_commit_is_isolated_child",
                "--nocapture",
            ])
            .env(FIXTURE_CHILD_ROOT, &fixture_repository)
            .env("GIT_DIR", hook_repository.join(".git"))
            .env("GIT_WORK_TREE", &hook_repository)
            .env("GIT_INDEX_FILE", hook_repository.join(".git/index"))
            .env("GIT_COMMON_DIR", hook_repository.join(".git"))
            .env("GIT_OBJECT_DIRECTORY", hook_repository.join(".git/objects"))
            .output()
            .unwrap();

        let fixture_has_repository = fixture_repository.join(".git").is_dir();
        let fixture_head =
            fixture_has_repository.then(|| git_output(&fixture_repository, &["rev-parse", "HEAD"]));
        let hook_head_after = git_output(&hook_repository, &["rev-parse", "HEAD"]);
        let hook_status = git_output(&hook_repository, &["status", "--porcelain"]);
        fs::remove_dir_all(root).unwrap();

        assert!(
            output.status.success(),
            "fixture child failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            fixture_has_repository,
            "fixture repository was not initialized"
        );
        assert!(fixture_head.is_some(), "fixture repository has no commit");
        assert_eq!(hook_head_after, hook_head, "hook repository HEAD changed");
        assert!(hook_status.is_empty(), "hook repository was modified");
    }

    #[test]
    fn fixture_repository_commit_is_isolated_child() {
        let Some(root) = std::env::var_os(FIXTURE_CHILD_ROOT) else {
            return;
        };
        initialize_fixture_repository(Path::new(&root));
    }

    fn git_output(root: &Path, args: &[&str]) -> String {
        let output = isolated_git_command(root).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "git command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    fn write_release_metadata_fixture(root: &Path, version: &str, registration: &str) {
        let system = version.split('-').next().unwrap();
        let files = [
            (
                ROOT_MANIFEST,
                format!(
                    "[workspace]\nmembers = [\n  \"crate\",\n]\n\n[workspace.package]\nversion = \"{version}\"\n"
                ),
            ),
            (
                LOCKFILE,
                format!(
                    "version = 4\n\n[[package]]\nname = \"fixture\"\nversion = \"{version}\"\n"
                ),
            ),
            (
                VSIX_MANIFEST,
                format!(
                    "<Identity Id=\"TcTidy\" Version=\"{system}\" Language=\"en-US\" />\n"
                ),
            ),
            (
                VSIX_MANIFEST_XAE15,
                format!(
                    "<Identity Id=\"TcTidy\" Version=\"{system}\" Language=\"en-US\" />\n"
                ),
            ),
            (
                ASSEMBLY_INFO,
                format!(
                    "[assembly: AssemblyVersion(\"{system}.0\")]\n[assembly: AssemblyFileVersion(\"{system}.0\")]\n[assembly: AssemblyInformationalVersion(\"{version}\")]\n"
                ),
            ),
            (
                PACKAGE_SOURCE,
                format!(
                    "[InstalledProductRegistration(\"TcTidy for TwinCAT\", \"Format TwinCAT Structured Text with tctidy.\", \"{registration}\")]\n"
                ),
            ),
            (
                INSTALLER_SOURCE,
                "AppVersion={#AppVersion}\nVersionInfoVersion={#NumericVersion}\nVersionInfoProductVersion={#NumericVersion}\nVersionInfoProductTextVersion={#AppVersion}\nOutputBaseFilename=TcTidySetup-{#AppVersion}\n".to_string(),
            ),
            (
                THIRD_PARTY_INVENTORY,
                format!(
                    "{{\n  \"schema\": 1,\n  \"cargoBundles\": [\n    {{\n      \"package\": \"tctidy-cli\",\n      \"version\": \"{version}\"\n    }}\n  ],\n  \"nugetFiles\": [\n    {{\n      \"package\": \"Example\",\n      \"version\": \"15.0.551\"\n    }}\n  ]\n}}\n"
                ),
            ),
            (
                THIRD_PARTY_INVENTORY_XAE15,
                format!(
                    "{{\n  \"schema\": 1,\n  \"cargoBundles\": [\n    {{\n      \"package\": \"tctidy-cli\",\n      \"version\": \"{version}\"\n    }}\n  ],\n  \"nugetFiles\": [\n    {{\n      \"package\": \"Example\",\n      \"version\": \"15.0.551\"\n    }}\n  ]\n}}\n"
                ),
            ),
            (
                CHANGELOG,
                format!(
                    "# Changelog\n\n## [Unreleased]\n\n### Fixed\n\n- Candidate fix.\n\n## [{version}] - 2026-07-15\n\n[Unreleased]: https://github.com/ViKodex/TcTidy/compare/v{version}...HEAD\n[{version}]: https://github.com/ViKodex/TcTidy/releases/tag/v{version}\n"
                ),
            ),
            (
                RELEASE_WORKFLOW,
                "on:\n  push:\n    tags:\n      - 'v[0-9]+.[0-9]+.[0-9]+'\njobs:\n  lint:\n    steps:\n      - run: cargo run -p xtask --locked -- keywords --check\n  linux-assets:\n    steps:\n      - run: scripts/release/build-linux.sh v0.4.0 dist notices\n  windows-assets:\n    steps:\n      - run: scripts/release/build-windows.ps1 -Tag v0.4.0\n  validate-assets:\n    needs: [preflight, lint, linux-assets, windows-assets]\n    steps:\n      - run: scripts/release/validate-candidate.sh v0.4.0 dist\n  publish:\n    needs: [preflight, validate-assets]\n".to_string(),
            ),
            (
                "crate/Cargo.toml",
                "[package]\nname = \"fixture\"\nversion.workspace = true\n".to_string(),
            ),
        ];

        for (relative, content) in files {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
    }

    fn initialize_fixture_repository(root: &Path) {
        for args in [
            vec!["init"],
            vec!["add", "."],
            vec![
                "-c",
                "user.name=TcTidy Tests",
                "-c",
                "user.email=tests@example.invalid",
                "commit",
                "-m",
                "fixture",
            ],
        ] {
            let output = isolated_git_command(root).args(args).output().unwrap();
            assert!(
                output.status.success(),
                "git fixture setup failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[test]
    fn installer_source_requires_text_and_numeric_version_fields() {
        let source = r#"AppVersion={#AppVersion}
VersionInfoVersion={#NumericVersion}
VersionInfoProductVersion={#NumericVersion}
VersionInfoProductTextVersion={#AppVersion}
OutputBaseFilename=TcTidySetup-{#AppVersion}
"#;
        validate_installer_version_contract(source).unwrap();

        for marker in [
            "AppVersion={#AppVersion}",
            "VersionInfoVersion={#NumericVersion}",
            "VersionInfoProductVersion={#NumericVersion}",
            "VersionInfoProductTextVersion={#AppVersion}",
            "OutputBaseFilename=TcTidySetup-{#AppVersion}",
        ] {
            assert!(
                validate_installer_version_contract(&source.replace(marker, "missing")).is_err()
            );
        }

        let preview = parse_product_version("0.4.0-rc.1").unwrap();
        assert_eq!(preview.to_string(), "0.4.0-rc.1");
        assert_eq!(
            VersionProjection::from_semver(&preview).unwrap().assembly,
            "0.4.0.0"
        );
    }

    #[test]
    fn assembly_metadata_keeps_the_full_prerelease_version() {
        let version = parse_product_version("0.4.0-rc.2").unwrap();
        let numeric = VersionProjection::from_semver(&version).unwrap().assembly;
        let source = r#"[assembly: AssemblyVersion("0.3.2.0")]
[assembly: AssemblyFileVersion("0.3.2.0")]
[assembly: AssemblyInformationalVersion("0.3.2")]"#;
        let updated = replace_call_value(
            &replace_call_value(
                &replace_call_value(source, "AssemblyVersion(\"", &numeric).unwrap(),
                "AssemblyFileVersion(\"",
                &numeric,
            )
            .unwrap(),
            "AssemblyInformationalVersion(\"",
            &version.to_string(),
        )
        .unwrap();

        assert_eq!(call_value(&updated, "AssemblyVersion(\"").unwrap(), numeric);
        assert_eq!(
            call_value(&updated, "AssemblyInformationalVersion(\"").unwrap(),
            "0.4.0-rc.2"
        );
    }

    #[test]
    fn workspace_version_replacement_is_scoped() {
        let input = r#"[workspace.package]
version = "0.3.2"

[workspace.dependencies]
some-dependency = "0.3.2"
"#;
        assert_eq!(workspace_version(input).unwrap(), Version::new(0, 3, 2));
        let output = replace_workspace_version(input, &Version::new(0, 3, 3)).unwrap();
        assert!(output.contains("[workspace.package]\nversion = \"0.3.3\""));
        assert!(output.contains("some-dependency = \"0.3.2\""));
    }

    #[test]
    fn cargo_inventory_version_replacement_is_scoped() {
        let input = r#"{
  "cargoBundles": [
    {
      "version": "0.3.2"
    }
  ],
  "nugetFiles": [
    {
      "version": "0.3.2"
    }
  ]
}
"#;
        let output = replace_cargo_inventory_version(input, &Version::new(0, 3, 3)).unwrap();
        assert!(output.contains("\"cargoBundles\": [\n    {\n      \"version\": \"0.3.3\""));
        assert!(output.contains("\"nugetFiles\": [\n    {\n      \"version\": \"0.3.2\""));
    }

    #[test]
    fn lockfile_updates_only_the_requested_workspace_packages() {
        let input = r#"version = 4

[[package]]
name = "external"
version = "0.3.2"

[[package]]
name = "tctidy"
version = "0.3.2"

[[package]]
name = "xtask"
version = "0.3.2"
"#;
        let output =
            replace_lock_versions(input, &["tctidy", "xtask"], &Version::new(0, 3, 3)).unwrap();
        assert!(output.contains("name = \"external\"\nversion = \"0.3.2\""));
        assert!(output.contains("name = \"tctidy\"\nversion = \"0.3.3\""));
        assert!(output.contains("name = \"xtask\"\nversion = \"0.3.3\""));
    }

    #[test]
    fn changelog_cut_moves_unreleased_content_and_advances_links() {
        let input = r#"# Changelog

## [Unreleased]

### Fixed

- Preserve comments.

## [0.3.2] - 2026-07-06

[Unreleased]: https://github.com/ViKodex/TcTidy/compare/v0.3.2...HEAD
[0.3.2]: https://github.com/ViKodex/TcTidy/releases/tag/v0.3.2
"#;
        let output = cut_changelog(
            input,
            &Version::new(0, 3, 2),
            &Version::new(0, 3, 3),
            "2026-07-15",
        )
        .unwrap();
        assert!(output.contains("## [Unreleased]\n\n## [0.3.3] - 2026-07-15\n\n### Fixed"));
        assert!(output.contains("compare/v0.3.3...HEAD"));
        assert!(output.contains("[0.3.3]: https://github.com/ViKodex/TcTidy/releases/tag/v0.3.3"));
        assert!(output.contains("[0.3.2]:"));
    }

    #[test]
    fn changelog_cut_rejects_an_empty_unreleased_section() {
        let input = "## [Unreleased]\n\n## [0.3.2] - 2026-07-06\n";
        assert!(cut_changelog(
            input,
            &Version::new(0, 3, 2),
            &Version::new(0, 3, 3),
            "2026-07-15"
        )
        .is_err());
    }

    #[test]
    fn changelog_promotes_a_candidate_to_stable_without_losing_its_notes() {
        let input = r#"# Changelog

## [Unreleased]

### Fixed

- Candidate fix.

## [0.3.2] - 2026-07-06

[Unreleased]: https://github.com/ViKodex/TcTidy/compare/v0.3.2...HEAD
[0.3.2]: https://github.com/ViKodex/TcTidy/releases/tag/v0.3.2
"#;
        let rc_version = parse_product_version("0.4.0-rc.1").unwrap();
        let candidate =
            cut_changelog(input, &Version::new(0, 3, 2), &rc_version, "2026-07-15").unwrap();
        let candidate = candidate.replacen(
            "## [Unreleased]\n",
            "## [Unreleased]\n\n### Fixed\n\n- Fix after candidate.\n",
            1,
        );
        let stable = cut_changelog(
            &candidate,
            &rc_version,
            &Version::new(0, 4, 0),
            "2026-07-16",
        )
        .unwrap();

        assert!(stable.contains("## [0.4.0] - 2026-07-16\n\n### Fixed"));
        assert!(stable.contains("- Candidate fix."));
        assert!(stable.contains("- Fix after candidate."));
        assert_eq!(stable.matches("### Fixed").count(), 1);
        assert!(!stable.contains("## [0.4.0-rc.1]"));
        assert!(!stable.contains("[0.4.0-rc.1]:"));
        assert!(stable.contains("compare/v0.4.0...HEAD"));
        assert!(stable.contains("[0.4.0]: https://github.com/ViKodex/TcTidy/releases/tag/v0.4.0"));
    }

    #[test]
    fn asset_manifest_expands_to_exact_release_names() {
        let manifest = "tctidy-{tag}-x86_64-unknown-linux-gnu.tar.gz\n\
                        tctidy-{tag}-x86_64-unknown-linux-gnu.tar.gz.sha256\n\
                        TcTidySetup-{version}.exe\n\
                        TcTidySetup-{version}.exe.sha256\n";
        let version = Version::new(0, 3, 3);
        let assets = expected_assets(manifest, "v0.3.3", &version).unwrap();
        assert_eq!(
            assets,
            vec![
                "tctidy-v0.3.3-x86_64-unknown-linux-gnu.tar.gz",
                "tctidy-v0.3.3-x86_64-unknown-linux-gnu.tar.gz.sha256",
                "TcTidySetup-0.3.3.exe",
                "TcTidySetup-0.3.3.exe.sha256",
            ]
        );

        let preview = Version::parse("0.4.0-rc.1").unwrap();
        let preview_assets = expected_assets(manifest, "v0.4.0-rc.1", &preview).unwrap();
        assert_eq!(
            preview_assets,
            vec![
                "tctidy-v0.4.0-rc.1-x86_64-unknown-linux-gnu.tar.gz",
                "tctidy-v0.4.0-rc.1-x86_64-unknown-linux-gnu.tar.gz.sha256",
                "TcTidySetup-0.4.0-rc.1.exe",
                "TcTidySetup-0.4.0-rc.1.exe.sha256",
            ]
        );
    }

    #[test]
    fn release_workflow_contract_rejects_missing_blocking_guards() {
        let workflow = r#"on:
  push:
    tags:
      - 'v[0-9]+.[0-9]+.[0-9]+'
jobs:
  lint:
    steps:
      - run: cargo run -p xtask --locked -- keywords --check
  linux-assets:
    steps:
      - run: scripts/release/build-linux.sh v0.3.3 dist notices
  windows-assets:
    steps:
      - run: scripts/release/build-windows.ps1 -Tag v0.3.3
  validate-assets:
    needs: [preflight, lint, linux-assets, windows-assets]
    steps:
      - run: scripts/release/validate-candidate.sh v0.3.3 dist
  publish:
    needs: [preflight, validate-assets]
"#;
        validate_release_workflow_contract(workflow).unwrap();

        for guard in [
            "tags:\n      - 'v[0-9]+.[0-9]+.[0-9]+'",
            "cargo run -p xtask --locked -- keywords --check",
            "scripts/release/build-linux.sh",
            "scripts/release/build-windows.ps1",
            "scripts/release/validate-candidate.sh",
            "needs: [preflight, lint, linux-assets, windows-assets]",
            "needs: [preflight, validate-assets]",
        ] {
            let incomplete = workflow.replace(guard, "missing-required-guard");
            assert!(
                validate_release_workflow_contract(&incomplete).is_err(),
                "removing {guard:?} must invalidate the release workflow"
            );
        }

        assert!(validate_release_workflow_contract(
            &workflow.replace("jobs:\n", "workflow_dispatch:\n\njobs:\n")
        )
        .is_err());
    }

    #[test]
    fn release_workflow_contract_accepts_windows_line_endings() {
        let workflow = r#"on:
  push:
    tags:
      - 'v[0-9]+.[0-9]+.[0-9]+'
jobs:
  lint:
    steps:
      - run: cargo run -p xtask --locked -- keywords --check
  linux-assets:
    steps:
      - run: scripts/release/build-linux.sh v0.3.3 dist notices
  windows-assets:
    steps:
      - run: scripts/release/build-windows.ps1 -Tag v0.3.3
  validate-assets:
    needs: [preflight, lint, linux-assets, windows-assets]
    steps:
      - run: scripts/release/validate-candidate.sh v0.3.3 dist
  publish:
    needs: [preflight, validate-assets]
"#
        .replace('\n', "\r\n");

        validate_release_workflow_contract(&workflow).unwrap();
    }

    #[test]
    fn civil_date_conversion_matches_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_649), (2026, 7, 15));
    }

    #[test]
    fn asset_check_rejects_any_file_outside_the_manifest() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tctidy-release-assets-{nonce}"));
        let dist = root.join("dist");
        fs::create_dir_all(root.join(".github")).unwrap();
        fs::create_dir_all(&dist).unwrap();
        fs::write(
            root.join(ASSET_MANIFEST),
            "bundle-{tag}.bin\nbundle-{tag}.bin.sha256\n",
        )
        .unwrap();
        let asset = "bundle-v0.3.3.bin";
        fs::write(dist.join(asset), b"candidate").unwrap();
        let digest = crate::tooling::file_digest(&dist.join(asset)).unwrap();
        fs::write(
            dist.join(format!("{asset}.sha256")),
            format!("{digest}  {asset}\n"),
        )
        .unwrap();

        check_assets(&root, &dist, "v0.3.3").unwrap();
        fs::write(dist.join("unexpected.txt"), b"no").unwrap();
        assert!(check_assets(&root, &dist, "v0.3.3").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn asset_check_rejects_checksum_bound_to_the_wrong_asset() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tctidy-release-assets-binding-{nonce}"));
        let dist = root.join("dist");
        fs::create_dir_all(root.join(".github")).unwrap();
        fs::create_dir_all(&dist).unwrap();
        fs::write(
            root.join(ASSET_MANIFEST),
            "first-{tag}.bin\nfirst-{tag}.bin.sha256\n\
             second-{tag}.bin\nsecond-{tag}.bin.sha256\n",
        )
        .unwrap();

        let first = "first-v0.3.3.bin";
        let second = "second-v0.3.3.bin";
        fs::write(dist.join(first), b"first candidate").unwrap();
        fs::write(dist.join(second), b"second candidate").unwrap();
        let digest = crate::tooling::file_digest(&dist.join(first)).unwrap();
        let sidecar = format!("{digest}  {first}\n");
        fs::write(dist.join(format!("{first}.sha256")), &sidecar).unwrap();
        fs::write(dist.join(format!("{second}.sha256")), sidecar).unwrap();

        assert!(check_assets(&root, &dist, "v0.3.3").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checksum_validation_accepts_gnu_formats_and_rejects_malformed_or_changed_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let asset = "bundle.bin";
        let checksum = "bundle.bin.sha256";
        fs::write(temp.path().join(asset), "candidate").unwrap();
        let digest = crate::tooling::file_digest(&temp.path().join(asset)).unwrap();
        for contents in [
            format!("{digest}  {asset}"),
            format!("{digest}  {asset}\n"),
            format!("{digest} *{asset}\r\n"),
            format!("{}  {asset}\n", digest.to_uppercase()),
        ] {
            fs::write(temp.path().join(checksum), contents).unwrap();
            validate_checksum_binding(temp.path(), checksum).unwrap();
        }
        for contents in [
            String::new(),
            format!("{digest}  {asset}\n\n"),
            format!("{digest}  {asset}\n{digest}  {asset}\n"),
            format!("{digest} {asset}"),
            format!("{digest}  ../{asset}"),
            format!("{digest}  {asset} "),
            format!("{}  {asset}", "g".repeat(64)),
            format!("{}  {asset}", "a".repeat(63)),
            format!("{}é  {asset}", "a".repeat(63)),
            format!("{}  {asset}", "0".repeat(64)),
        ] {
            fs::write(temp.path().join(checksum), contents).unwrap();
            assert!(validate_checksum_binding(temp.path(), checksum).is_err());
        }
        fs::write(temp.path().join(checksum), format!("{digest}  {asset}\n")).unwrap();
        fs::write(temp.path().join(asset), "changed candidate").unwrap();
        assert!(validate_checksum_binding(temp.path(), checksum).is_err());
        fs::remove_file(temp.path().join(asset)).unwrap();
        assert!(validate_checksum_binding(temp.path(), checksum).is_err());
    }
}
