//! Exact runtime dependency inventories and license evidence for shipped payloads.

use super::{
    array, digest, file_digest, files, json, parse_json, payload_file, read, relative, string,
    text, write, write_json, Result,
};
use clap::{Args, Subcommand};
use regex::Regex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const GENERATED_FILES: &[&str] = &[
    "[Content_Types].xml",
    "catalog.json",
    "extension.vsixmanifest",
    "manifest.json",
    "TcTidy.VsExtension.pkgdef",
];

/// The tracked, human-readable attribution of the extension's NuGet components. The Cargo
/// notice is generated per target and also ships in the CLI archives, so it cannot list them.
pub(super) const EXTENSION_NOTICE: &str = "THIRD-PARTY-NOTICES-EXTENSION.md";

#[derive(Subcommand)]
pub(super) enum Command {
    Generate(Generate),
    Compare {
        #[arg(long)]
        supplied_inventory: PathBuf,
        #[arg(long)]
        supplied_notice: PathBuf,
        #[arg(long)]
        generated_inventory: PathBuf,
        #[arg(long)]
        generated_notice: PathBuf,
    },
    Vsix(Vsix),
    Installer {
        #[arg(long)]
        reference_payload: PathBuf,
        #[arg(long)]
        installed_payload: PathBuf,
    },
    Notice {
        #[arg(long)]
        payload: PathBuf,
        #[arg(long, default_value = "THIRD-PARTY-NOTICES.md")]
        notice: String,
    },
}

#[derive(Args)]
pub(super) struct Generate {
    #[arg(long, default_value = "crates/tctidy-cli/Cargo.toml")]
    manifest: PathBuf,
    #[arg(long, default_value = "tctidy-cli")]
    package: String,
    #[arg(long, default_value = "x86_64-pc-windows-msvc")]
    target: String,
    #[arg(
        long,
        default_value = "editors/vs/TcTidy.VsExtension/CARGO-DEPENDENCIES.json"
    )]
    inventory: PathBuf,
    #[arg(
        long,
        default_value = "editors/vs/TcTidy.VsExtension/THIRD-PARTY-NOTICES.md"
    )]
    notice: PathBuf,
    #[arg(long, default_value = "target/tctidy-cli-windows-metadata.json")]
    metadata_output: PathBuf,
    #[arg(long, default_value = "target/tctidy-cli-windows-about.json")]
    about_output: PathBuf,
    #[arg(long, default_value = "target/tctidy-cli-windows-tree.txt")]
    tree_output: PathBuf,
}

#[derive(Args)]
pub(super) struct Vsix {
    #[arg(long)]
    payload: PathBuf,
    #[arg(long)]
    inventory: PathBuf,
    /// The tracked license files, each compared to its namesake in the payload.
    #[arg(long, required = true)]
    license: Vec<PathBuf>,
    #[arg(long)]
    project_assets: PathBuf,
    #[arg(long)]
    cargo_metadata: PathBuf,
    #[arg(long)]
    cargo_about: PathBuf,
    #[arg(long)]
    cargo_tree: PathBuf,
}

pub(super) fn run(command: Command) -> Result {
    match command {
        Command::Generate(args) => generate(&args)?,
        Command::Compare {
            supplied_inventory,
            supplied_notice,
            generated_inventory,
            generated_notice,
        } => {
            compare(&supplied_inventory, &generated_inventory)?;
            compare(&supplied_notice, &generated_notice)?;
        }
        Command::Vsix(args) => validate_vsix(&args)?,
        Command::Installer {
            reference_payload,
            installed_payload,
        } => installer(&reference_payload, &installed_payload)?,
        Command::Notice { payload, notice } => {
            notice_references(&payload, &payload_file(&payload, &notice)?)?
        }
    }
    println!("license evidence and payload checks passed");
    Ok(())
}

fn compare(supplied: &Path, generated: &Path) -> Result {
    if read(supplied)? != read(generated)? {
        return Err(format!(
            "supplied evidence {} does not match the checked-out runtime graph",
            supplied.display()
        ));
    }
    Ok(())
}

fn notice_references(payload: &Path, notice: &Path) -> Result {
    let references = Regex::new(r"(?i)\b(?:see|stored in)\s+`([^`]+)`").unwrap();
    for reference in references.captures_iter(&text(notice)?) {
        payload_file(payload, &reference[1])?;
    }
    Ok(())
}

type PackageKey = (String, String);

pub(super) fn evidence(
    metadata: &Value,
    about: &Value,
    tree: &str,
    package: &str,
    target: &str,
) -> Result<(Value, String)> {
    let members: BTreeSet<_> = array(metadata, "workspace_members")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| "workspace member ID must be a string".to_owned())
        })
        .collect::<Result<_>>()?;
    let workspace: Vec<_> = array(metadata, "packages")?
        .iter()
        .filter(|entry| entry["id"].as_str().is_some_and(|id| members.contains(id)))
        .collect();
    let roots: Vec<_> = workspace
        .iter()
        .filter(|entry| entry["name"] == package)
        .collect();
    if roots.len() != 1 {
        return Err(format!(
            "expected one Cargo workspace package named {package}"
        ));
    }
    let root = *roots[0];
    let internal: BTreeSet<_> = workspace
        .iter()
        .map(|entry| key(entry))
        .collect::<Result<_>>()?;
    let mut tree_packages = BTreeSet::new();
    for line in tree.lines() {
        let mut words = line.split_whitespace();
        let name = words.next().ok_or("empty cargo tree line")?;
        let version = words
            .next()
            .and_then(|v| v.strip_prefix('v'))
            .ok_or("invalid cargo tree version")?;
        tree_packages.insert((name.to_owned(), version.to_owned()));
    }
    if !tree_packages.contains(&key(root)?) {
        return Err(format!("cargo tree omits root package {package}"));
    }
    let mut about_packages = BTreeMap::new();
    for entry in array(about, "crates")? {
        let about_package = &entry["package"];
        if about_packages
            .insert(key(about_package)?, about_package)
            .is_some()
        {
            return Err("duplicate cargo-about crate identity".into());
        }
    }
    let mut runtime = Vec::new();
    let mut ids = BTreeSet::new();
    for key in tree_packages.difference(&internal) {
        let entry = about_packages
            .get(key)
            .ok_or_else(|| format!("cargo-about omits runtime crate {} {}", key.0, key.1))?;
        ids.insert(string(entry, "id")?.to_owned());
        string(entry, "license")?;
        runtime.push(*entry);
    }
    let (notice, licenses) = render_notice(about, &ids)?;
    let packages: Vec<_> = runtime
        .iter()
        .map(|entry| {
            Ok(json!({
                "name": string(entry, "name")?, "version": string(entry, "version")?,
                "source": string(entry, "source")?, "license": string(entry, "license")?,
                "noticeLicenses": licenses[string(entry, "id")?],
            }))
        })
        .collect::<Result<_>>()?;
    Ok((
        json!({"schema":1,"root":{"package":string(root,"name")?,"version":string(root,"version")?,"license":string(root,"license")?,"target":target},"packages":packages,"noticeSha256":digest(notice.as_bytes())}),
        notice,
    ))
}

fn key(entry: &Value) -> Result<PackageKey> {
    Ok((
        string(entry, "name")?.to_owned(),
        string(entry, "version")?.to_owned(),
    ))
}

fn render_notice(
    about: &Value,
    ids: &BTreeSet<String>,
) -> Result<(String, BTreeMap<String, BTreeSet<String>>)> {
    let mut package_licenses: BTreeMap<_, BTreeSet<String>> =
        ids.iter().map(|id| (id.clone(), BTreeSet::new())).collect();
    // Preserve cargo-about's license order so overview and license sections agree.
    let mut overview: Vec<(String, BTreeSet<String>)> = Vec::new();
    let mut sections = Vec::new();
    for license in array(about, "licenses")? {
        let name = string(license, "name")?;
        let id = string(license, "id")?;
        let license_text = string(license, "text")?;
        let mut selected = BTreeMap::new();
        for usage in array(license, "used_by")? {
            let entry = &usage["crate"];
            if let Some(package_id) = entry["id"].as_str().filter(|id| ids.contains(*id)) {
                selected.insert(package_id.to_owned(), entry);
            }
        }
        if selected.is_empty() {
            continue;
        }
        for package_id in selected.keys() {
            package_licenses
                .get_mut(package_id)
                .unwrap()
                .insert(id.to_owned());
        }
        let index = overview
            .iter()
            .position(|entry| entry.0 == name)
            .unwrap_or_else(|| {
                overview.push((name.to_owned(), BTreeSet::new()));
                overview.len() - 1
            });
        overview[index].1.extend(selected.keys().cloned());
        let mut crates: Vec<_> = selected.into_values().collect();
        for entry in &crates {
            key(entry)?;
        }
        crates.sort_by_key(|entry| {
            (
                entry["name"].as_str().unwrap(),
                entry["version"].as_str().unwrap(),
                entry["source"].as_str().unwrap_or_default(),
            )
        });
        sections.push((name, license_text, crates));
    }
    for (package, licenses) in &package_licenses {
        if licenses.is_empty() {
            return Err(format!(
                "cargo-about has no selected license text for runtime crate {package}"
            ));
        }
    }
    let mut output = String::from("# Third-party licenses\n\nTcTidy is distributed under the MIT license (see `LICENSE-MIT`) or the Apache-2.0\nlicense (see `LICENSE-APACHE`), at your option. The TcTidy binary statically\nlinks the third-party crates listed below; their license texts are reproduced in\nfull, grouped by license, as required for redistribution.\n\nThe binary also statically links the Rust standard library, which is available under\nthe MIT or the Apache-2.0 license; it is redistributed here under Apache-2.0, whose\ntext is in `LICENSE-APACHE`.\n\nThe exact package, version and license inventory used to produce this notice is\nstored in `CARGO-DEPENDENCIES.json`.\n\n## Overview\n\n");
    for (name, packages) in overview {
        output.push_str(&format!("- {name}, {} crate(s)\n", packages.len()));
    }
    for (name, license_text, crates) in sections {
        output.push_str(&format!("\n## {name}\n\nUsed by:\n\n"));
        for entry in crates {
            output.push_str(&format!(
                "- {} {}",
                string(entry, "name")?,
                string(entry, "version")?
            ));
            if let Some(repository) = entry["repository"].as_str().filter(|s| !s.is_empty()) {
                output.push_str(&format!(" ({repository})"));
            }
            output.push('\n');
        }
        output.push_str(&format!(
            "\n````text\n{}\n````\n",
            license_text
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .trim_end()
        ));
    }
    Ok((output, package_licenses))
}

fn cargo(args: &[String]) -> Result<String> {
    let executable = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = std::process::Command::new(executable)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
}

fn generate(args: &Generate) -> Result {
    generate_with(args, cargo)
}

fn generate_with(args: &Generate, mut run: impl FnMut(&[String]) -> Result<String>) -> Result {
    let manifest = args.manifest.to_string_lossy().into_owned();
    let command = |items: &[&str]| items.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    // Fetch every locked target before offline license resolution.
    run(&command(&[
        "fetch",
        "--locked",
        "--manifest-path",
        &manifest,
    ]))?;
    let metadata = parse_json(
        &run(&command(&[
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--locked",
            "--offline",
            "--manifest-path",
            &manifest,
        ]))?,
        "cargo metadata",
    )?;
    let temporary = tempfile::Builder::new()
        .prefix("tctidy-cargo-about-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let about_path = temporary.path().join("output.json");
    run(&command(&[
        "about",
        "generate",
        "--manifest-path",
        &manifest,
        "--target",
        &args.target,
        "--format",
        "json",
        "--locked",
        "--offline",
        "--output-file",
        &about_path.to_string_lossy(),
    ]))?;
    let about = json(&about_path)?;
    let tree = run(&command(&[
        "tree",
        "--package",
        &args.package,
        "--target",
        &args.target,
        "--edges",
        "normal,no-proc-macro",
        "--prefix",
        "none",
        "--format",
        "{p}",
        "--locked",
        "--offline",
        "--manifest-path",
        &manifest,
    ]))?;
    let (inventory, notice) = evidence(&metadata, &about, &tree, &args.package, &args.target)?;
    write_json(&args.inventory, &inventory)?;
    write(&args.notice, notice.as_bytes())?;
    write_json(&args.metadata_output, &metadata)?;
    write_json(&args.about_output, &about)?;
    write(&args.tree_output, tree.as_bytes())
}

fn target_package(assets: &Value, file: &str) -> Result<(String, String, String, String)> {
    let targets = assets["targets"]
        .as_object()
        .ok_or("NuGet assets have no targets")?;
    let mut candidates = BTreeSet::new();
    for target in targets.values().filter_map(Value::as_object) {
        for (key, package) in target {
            if package["type"] != "package" {
                continue;
            }
            for kind in ["runtime", "runtimeTargets"] {
                if let Some(entries) = package[kind].as_object() {
                    for asset in entries.keys() {
                        if asset
                            .rsplit('/')
                            .next()
                            .is_some_and(|name| name.eq_ignore_ascii_case(file))
                        {
                            let (name, version) =
                                key.rsplit_once('/').ok_or("invalid NuGet package key")?;
                            candidates.insert((
                                name.to_owned(),
                                version.to_owned(),
                                key.clone(),
                                asset.clone(),
                            ));
                        }
                    }
                }
            }
        }
    }
    if candidates.len() != 1 {
        return Err(format!(
            "{file} must have exactly one restored NuGet provider, found {}",
            candidates.len()
        ));
    }
    Ok(candidates.pop_first().unwrap())
}

fn package_directory(assets: &Value, key: &str) -> Result<(PathBuf, Vec<String>)> {
    let library = &assets["libraries"][key];
    if library["type"] != "package" {
        return Err(format!("missing NuGet library metadata: {key}"));
    }
    let relative = relative(string(library, "path")?)?;
    let files = array(library, "files")?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "invalid NuGet file list".to_owned())
        })
        .collect::<Result<Vec<_>>>()?;
    let folders = assets["packageFolders"]
        .as_object()
        .ok_or("NuGet assets have no packageFolders")?;
    for folder in folders.keys() {
        let directory = Path::new(folder).join(&relative);
        if directory.is_dir() {
            return Ok((directory, files));
        }
    }
    Err(format!("restored package directory is missing: {key}"))
}

fn nuspec(directory: &Path) -> Result<BTreeMap<String, String>> {
    let paths = fs::read_dir(directory)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    let paths: Vec<_> = paths
        .into_iter()
        .filter(|path| path.extension().is_some_and(|e| e == "nuspec"))
        .collect();
    if paths.len() != 1 {
        return Err("expected exactly one nuspec".into());
    }
    let source = text(&paths[0])?;
    let document = roxmltree::Document::parse(&source).map_err(|e| e.to_string())?;
    let metadata = document
        .descendants()
        .find(|n| n.has_tag_name("metadata"))
        .ok_or("nuspec metadata missing")?;
    let mut values = BTreeMap::new();
    for child in metadata.children().filter(|n| n.is_element()) {
        if let Some(value) = child.text() {
            values.insert(child.tag_name().name().to_owned(), value.trim().to_owned());
        }
        if child.has_tag_name("license") {
            if let Some(kind) = child.attribute("type") {
                values.insert("licenseType".into(), kind.into());
            }
        }
    }
    Ok(values)
}

fn license(
    payload: &Path,
    expected: &Value,
    actual: &BTreeMap<String, String>,
    package: &str,
) -> Result<Vec<String>> {
    let kind = string(expected, "type")?;
    let value = string(expected, "value")?;
    let valid = match kind {
        "expression" => {
            actual.get("licenseType").is_some_and(|s| s == kind)
                && actual.get("license").is_some_and(|s| s == value)
        }
        "url" => actual.get("licenseUrl").is_some_and(|s| s == value),
        _ => false,
    };
    if !valid {
        return Err(format!("license metadata disagrees for {package}"));
    }
    let mut names = Vec::new();
    if expected.get("files").is_some() {
        for name in array(expected, "files")? {
            let name = name.as_str().ok_or("license files must contain strings")?;
            payload_file(payload, name)?;
            names.push(relative(name)?);
        }
    }
    Ok(names)
}

fn package_notice(
    payload: &Path,
    package_files: &[String],
    expected: &Value,
) -> Result<Option<String>> {
    let required = expected["required"]
        .as_bool()
        .ok_or("notice.required must be boolean")?;
    let notice_name =
        Regex::new(r"(?i)^(?:third[-_. ]?party[-_. ]?notices?|notices?)(?:[-_. ].*)?$").unwrap();
    let has_notice = package_files.iter().any(|name| {
        Path::new(name)
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| notice_name.is_match(s))
    });
    if has_notice && !required {
        return Err("restored package contains an omitted required notice".into());
    }
    if required {
        let name = string(expected, "file")?;
        notice_references(payload, &payload_file(payload, name)?)?;
        return Ok(Some(relative(name)?));
    }
    Ok(None)
}

fn register(expected: &mut BTreeSet<String>, name: &str) -> Result {
    let name = relative(name)?;
    if !expected.insert(name.clone()) {
        return Err(format!("duplicate payload path: {name}"));
    }
    Ok(())
}

/// The extension notice must name every redistributed NuGet package. Versions stay in the
/// inventory, so a package update does not have to touch the notice.
pub(super) fn nuget_attribution(notice: &str, entry: &Value) -> Result {
    let package = string(entry, "package")?;
    let attribution =
        Regex::new(&format!(r"(?:^|\s){}(?:$|[\s),;])", regex::escape(package))).unwrap();
    if !attribution.is_match(notice) {
        return Err(format!("{EXTENSION_NOTICE} does not attribute {package}"));
    }
    Ok(())
}

fn register_identity(identities: &mut BTreeSet<PackageKey>, entry: &Value) -> Result {
    let key = (
        string(entry, "package")?.to_lowercase(),
        string(entry, "version")?.to_owned(),
    );
    if !identities.insert(key.clone()) {
        return Err(format!("duplicate package identity: {} {}", key.0, key.1));
    }
    Ok(())
}

fn validate_vsix(args: &Vsix) -> Result {
    let payload = &args.payload;
    let inventory = json(&args.inventory)?;
    compare(
        &payload_file(payload, "THIRD-PARTY-INVENTORY.json")?,
        &args.inventory,
    )?;
    if inventory["schema"].as_u64() != Some(1) {
        return Err("extension inventory schema must be 1".into());
    }
    let assets = json(&args.project_assets)?;
    let metadata = json(&args.cargo_metadata)?;
    let about = json(&args.cargo_about)?;
    let tree = text(&args.cargo_tree)?;
    let mut expected: BTreeSet<_> = [
        "THIRD-PARTY-INVENTORY.json".to_owned(),
        EXTENSION_NOTICE.to_owned(),
    ]
    .into();
    for source in &args.license {
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("license path must name a file")?;
        compare(&payload_file(payload, name)?, source)?;
        expected.insert(name.to_owned());
    }
    let extension_notice = payload_file(payload, EXTENSION_NOTICE)?;
    notice_references(payload, &extension_notice)?;
    let extension_notice = text(&extension_notice)?;
    let mut identities = BTreeSet::new();
    for entry in array(&inventory, "projectFiles")? {
        let name = entry.as_str().ok_or("projectFiles must contain strings")?;
        let path = payload_file(payload, name)?;
        if !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("TcTidy.")
        {
            return Err(format!(
                "projectFiles cannot classify third-party file {name}"
            ));
        }
        register(&mut expected, name)?;
    }
    for bundle in array(&inventory, "cargoBundles")? {
        register_identity(&mut identities, bundle)?;
        let name = string(bundle, "file")?;
        payload_file(payload, name)?;
        register(&mut expected, name)?;
        let (cargo_inventory, notice) = evidence(
            &metadata,
            &about,
            &tree,
            string(bundle, "package")?,
            string(bundle, "target")?,
        )?;
        if cargo_inventory["root"]["version"] != bundle["version"] {
            return Err("Cargo bundle version disagrees with resolved root".into());
        }
        let name = string(bundle, "inventoryFile")?;
        let path = payload_file(payload, name)?;
        register(&mut expected, name)?;
        if json(&path)? != cargo_inventory {
            return Err("Cargo dependency inventory disagrees with resolved runtime graph".into());
        }
        let actual_license = BTreeMap::from([
            ("licenseType".to_owned(), "expression".to_owned()),
            (
                "license".to_owned(),
                string(&cargo_inventory["root"], "license")?.to_owned(),
            ),
        ]);
        expected.extend(license(
            payload,
            &bundle["license"],
            &actual_license,
            string(bundle, "package")?,
        )?);
        let name = string(bundle, "noticeFile")?;
        let path = payload_file(payload, name)?;
        register(&mut expected, name)?;
        notice_references(payload, &path)?;
        if read(&path)? != notice.as_bytes() {
            return Err(
                "Cargo notice disagrees with resolved runtime graph or notice SHA-256".into(),
            );
        }
    }
    for entry in array(&inventory, "nugetFiles")? {
        register_identity(&mut identities, entry)?;
        nuget_attribution(&extension_notice, entry)?;
        let name = string(entry, "file")?;
        let path = payload_file(payload, name)?;
        register(&mut expected, name)?;
        let (package, version, key, runtime_asset) =
            target_package(&assets, &path.file_name().unwrap().to_string_lossy())?;
        if !package.eq_ignore_ascii_case(string(entry, "package")?)
            || version != string(entry, "version")?
        {
            return Err(format!(
                "{name} disagrees with restored package {package} {version}"
            ));
        }
        let (directory, package_files) = package_directory(&assets, &key)?;
        if !package_files.contains(&runtime_asset) {
            return Err("NuGet runtime asset is absent from package metadata".into());
        }
        if file_digest(&path)? != file_digest(&payload_file(&directory, &runtime_asset)?)? {
            return Err(format!(
                "redistributed {name} differs from NuGet runtime asset"
            ));
        }
        let actual = nuspec(&directory)?;
        if !actual
            .get("id")
            .is_some_and(|id| id.eq_ignore_ascii_case(&package))
            || actual.get("version") != Some(&version)
        {
            return Err("nuspec identity disagrees with restored package".into());
        }
        expected.extend(license(payload, &entry["license"], &actual, &package)?);
        if let Some(name) = package_notice(payload, &package_files, &entry["notice"])? {
            register(&mut expected, &name)?;
        }
    }
    let actual = files(payload)?;
    let missing: Vec<_> = expected.difference(&actual).collect();
    let unexpected: Vec<_> = actual
        .difference(&expected)
        .filter(|name| !GENERATED_FILES.contains(&name.as_str()))
        .collect();
    if !missing.is_empty() || !unexpected.is_empty() {
        return Err(format!(
            "payload mismatch; missing: {missing:?}; unmapped redistributed files: {unexpected:?}"
        ));
    }
    Ok(())
}

fn installer(reference: &Path, installed: &Path) -> Result {
    let expected = files(reference)?;
    if expected.is_empty() {
        return Err("reference payload is empty".into());
    }
    let actual = files(installed)?;
    let uninstaller = Regex::new(r"^unins[0-9]{3}\.(?:exe|dat)$").unwrap();
    let unexpected: Vec<_> = actual
        .difference(&expected)
        .filter(|name| name.as_str() != ".tctidy-install-target" && !uninstaller.is_match(name))
        .collect();
    if !expected.is_subset(&actual) || !unexpected.is_empty() {
        return Err(format!(
            "installer payload has missing or unexpected files: {unexpected:?}"
        ));
    }
    for name in expected {
        if file_digest(&reference.join(&name))? != file_digest(&installed.join(&name))? {
            return Err(format!("installer payload changed {name}"));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "licenses_tests.rs"]
mod tests;
