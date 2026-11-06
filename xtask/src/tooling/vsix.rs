//! Validate manifests, archive paths and the declared managed architecture.

use super::{relative, text, Result};
use clap::Subcommand;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use unicode_casefold::UnicodeCaseFold;

const NAMESPACE: &str = "http://schemas.microsoft.com/developer/vsx-schema/2011";
const REQUIRED: &[&str] = &[
    "TcTidy.VsExtension.dll",
    "TcTidy.VsExtension.Core.dll",
    "TcTidy.VsExtension.pkgdef",
    "LICENSE",
    "LICENSE-MIT",
    "LICENSE-APACHE",
    "CARGO-DEPENDENCIES.json",
    "THIRD-PARTY-NOTICES.md",
    "THIRD-PARTY-NOTICES-EXTENSION.md",
    "THIRD-PARTY-INVENTORY.json",
    "extension.vsixmanifest",
    "tctidy.exe",
];

#[derive(Subcommand)]
pub(super) enum Command {
    Check {
        #[arg(long, value_parser = ["15", "17"])]
        shell_version: String,
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long, required = true)]
        project: Vec<PathBuf>,
        #[arg(long)]
        vsix: Option<PathBuf>,
    },
}

pub(super) fn run(command: Command) -> Result {
    let Command::Check {
        shell_version,
        manifest,
        project,
        vsix,
    } = command;
    validate_manifest(&text(&manifest)?, &shell_version)?;
    for project in project {
        validate_project(&text(&project)?)?;
    }
    if let Some(vsix) = vsix {
        let file = File::open(&vsix).map_err(|e| format!("{}: {e}", vsix.display()))?;
        validate_archive(file, &shell_version)?;
    }
    println!("VSIX shell targets, archive paths and managed architectures are valid");
    Ok(())
}

pub(super) fn validate_manifest(source: &str, shell: &str) -> Result {
    let document = roxmltree::Document::parse(source).map_err(|e| e.to_string())?;
    if !document
        .root_element()
        .has_tag_name((NAMESPACE, "PackageManifest"))
    {
        return Err("unexpected VSIX schema namespace".into());
    }
    let installations: Vec<_> = document
        .root_element()
        .children()
        .filter(|node| node.has_tag_name((NAMESPACE, "Installation")))
        .collect();
    let mut targets = Vec::new();
    for installation in installations {
        for node in installation
            .children()
            .filter(|node| node.has_tag_name((NAMESPACE, "InstallationTarget")))
        {
            let architectures: Vec<_> = node
                .children()
                .filter(|node| node.has_tag_name((NAMESPACE, "ProductArchitecture")))
                .map(|node| node.text().unwrap_or_default().trim().to_lowercase())
                .collect();
            targets.push((
                node.attribute("Id").unwrap_or_default(),
                node.attribute("Version").unwrap_or_default(),
                architectures,
            ));
        }
    }
    let expected = match shell {
        "15" => vec![(
            "Microsoft.VisualStudio.IntegratedShell",
            "[15.0,16.0)",
            Vec::new(),
        )],
        "17" => vec![(
            "Microsoft.VisualStudio.IntegratedShell",
            "[17.0,18.0)",
            vec!["amd64".to_owned()],
        )],
        _ => return Err("shell must be 15 or 17".into()),
    };
    if targets != expected {
        return Err(format!(
            "installation targets must be exactly {expected:?}, got {targets:?}"
        ));
    }
    Ok(())
}

pub(super) fn validate_project(source: &str) -> Result {
    let document = roxmltree::Document::parse(source).map_err(|e| e.to_string())?;
    let targets: Vec<_> = document
        .descendants()
        .filter(|node| node.has_tag_name("PlatformTarget"))
        .map(|node| node.text().unwrap_or_default().trim())
        .collect();
    if targets.is_empty()
        || targets
            .iter()
            .any(|target| !target.eq_ignore_ascii_case("AnyCPU"))
    {
        return Err("every project requires explicit PlatformTarget=AnyCPU".into());
    }
    Ok(())
}

/// Check names before reading or extracting any entry, independently of host OS.
fn archive_names(names: &[(String, bool)]) -> Result<BTreeMap<String, usize>> {
    let mut seen = BTreeMap::new();
    let mut parents = BTreeSet::new();
    let mut files = BTreeMap::new();
    for (index, (raw, directory)) in names.iter().enumerate() {
        let normalized = raw.replace('\\', "/");
        let name = if *directory {
            normalized.strip_suffix('/').unwrap_or(&normalized)
        } else {
            &normalized
        };
        relative(name)?;
        let key: String = name.case_fold().collect();
        if seen.contains_key(&key) {
            return Err(format!(
                "case-fold collision or file/directory conflict: {raw}"
            ));
        }
        let components: Vec<_> = key.split('/').collect();
        for end in 1..components.len() {
            let parent = components[..end].join("/");
            if seen.get(&parent) == Some(&false) {
                return Err(format!("file used as a parent: {raw}"));
            }
            parents.insert(parent);
        }
        if !directory && parents.contains(&key) {
            return Err(format!("file used as a parent: {raw}"));
        }
        seen.insert(key, *directory);
        if !directory {
            files.insert(name.to_owned(), index);
        }
    }
    Ok(files)
}

// ZipArchive indexes entries by name and silently replaces exact duplicates.
// Comparing the original directory count preserves the duplicate-entry guard.
fn directory_entries(reader: &mut (impl Read + Seek)) -> Result<u64> {
    let length = reader.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    let start = length.saturating_sub(65_557);
    reader
        .seek(SeekFrom::Start(start))
        .map_err(|e| e.to_string())?;
    let mut tail = Vec::new();
    reader.read_to_end(&mut tail).map_err(|e| e.to_string())?;
    let end = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&offset| {
            tail[offset..].starts_with(b"PK\x05\x06")
                && offset + 22 + u16::from_le_bytes([tail[offset + 20], tail[offset + 21]]) as usize
                    == tail.len()
        })
        .ok_or("missing ZIP end-of-directory record")?;
    let count = u16::from_le_bytes([tail[end + 10], tail[end + 11]]);
    if count != u16::MAX {
        return Ok(u64::from(count));
    }
    let locator = end.checked_sub(20).ok_or("missing ZIP64 locator")?;
    if !tail[locator..].starts_with(b"PK\x06\x07") {
        return Err("missing ZIP64 locator".into());
    }
    let offset = u64::from_le_bytes(tail[locator + 8..locator + 16].try_into().unwrap());
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut record = [0; 40];
    reader.read_exact(&mut record).map_err(|e| e.to_string())?;
    if !record.starts_with(b"PK\x06\x06") {
        return Err("invalid ZIP64 directory record".into());
    }
    Ok(u64::from_le_bytes(record[32..40].try_into().unwrap()))
}

fn validate_archive(mut reader: impl Read + Seek, shell: &str) -> Result {
    let entry_count = directory_entries(&mut reader)?;
    reader.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| e.to_string())?;
    if entry_count != archive.len() as u64 {
        return Err("duplicate archive entries or inconsistent directory count".into());
    }
    let mut names = Vec::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|e| e.to_string())?;
        if file.is_symlink() {
            return Err(format!("symlink in VSIX: {}", file.name()));
        }
        names.push((file.name().to_owned(), file.is_dir()));
    }
    let files = archive_names(&names)?;
    validate_payload_names(&files)?;
    let mut read_entry = |name: &str| -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        archive
            .by_index(files[name])
            .map_err(|e| e.to_string())?
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        Ok(bytes)
    };
    let manifest =
        String::from_utf8(read_entry("extension.vsixmanifest")?).map_err(|e| e.to_string())?;
    validate_manifest(&manifest, shell)?;
    for name in ["TcTidy.VsExtension.dll", "TcTidy.VsExtension.Core.dll"] {
        managed_architecture(&read_entry(name)?)?;
    }
    Ok(())
}

fn validate_payload_names(files: &BTreeMap<String, usize>) -> Result {
    for name in REQUIRED {
        if !files.contains_key(*name) {
            return Err(format!("required payload missing: {name}"));
        }
    }
    if files
        .keys()
        .any(|name| name.to_lowercase().ends_with(".pdb"))
    {
        return Err("debug symbols leaked into VSIX".into());
    }
    if files
        .keys()
        .any(|name| name.eq_ignore_ascii_case("Microsoft.VisualStudio.DpiAwareness.dll"))
    {
        return Err("Microsoft.VisualStudio.DpiAwareness.dll is a host SDK assembly and must not be redistributed".into());
    }
    Ok(())
}

/// Read only the standard PE/CLR header fields describing platform compatibility.
/// This does not load assemblies or inspect executable instructions.
fn managed_architecture(data: &[u8]) -> Result {
    fn u16_at(data: &[u8], offset: usize) -> Result<u16> {
        let bytes = data
            .get(offset..offset.checked_add(2).ok_or("PE offset overflow")?)
            .ok_or("truncated PE metadata")?;
        Ok(u16::from_le_bytes(bytes.try_into().unwrap()))
    }
    fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
        let bytes = data
            .get(offset..offset.checked_add(4).ok_or("PE offset overflow")?)
            .ok_or("truncated PE metadata")?;
        Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
    }
    if data.get(..2) != Some(b"MZ") {
        return Err("missing DOS/PE header".into());
    }
    let pe = u32_at(data, 0x3c)? as usize;
    if data.get(pe..pe.saturating_add(4)) != Some(b"PE\0\0") {
        return Err("missing PE signature".into());
    }
    let machine = u16_at(data, pe + 4)?;
    let count = u16_at(data, pe + 6)? as usize;
    let optional_size = u16_at(data, pe + 20)? as usize;
    let optional = pe + 24;
    if machine != 0x14c || u16_at(data, optional)? != 0x10b {
        return Err("AnyCPU DLL must be PE32/I386".into());
    }
    if optional_size < 216 || u32_at(data, optional + 92)? <= 14 {
        return Err("missing CLR data directory".into());
    }
    let rva = u32_at(data, optional + 208)?;
    let size = u32_at(data, optional + 212)?;
    if rva == 0 || size < 0x48 {
        return Err("invalid CLR data directory".into());
    }
    let sections = optional + optional_size;
    for index in 0..count {
        let section = sections + index * 40;
        let address = u32_at(data, section + 12)?;
        let raw_size = u32_at(data, section + 16)?;
        let Some(delta) = rva.checked_sub(address).filter(|delta| *delta < raw_size) else {
            continue;
        };
        let raw = u32_at(data, section + 20)? as usize;
        let offset = raw
            .checked_add(delta as usize)
            .ok_or("CLR offset overflow")?;
        data.get(offset..offset.checked_add(0x48).ok_or("CLR offset overflow")?)
            .ok_or("truncated CLR metadata")?;
        let flags = u32_at(data, offset + 16)?;
        return managed_flags(flags);
    }
    Err("CLR directory is not mapped by a PE section".into())
}

fn managed_flags(flags: u32) -> Result {
    if flags & 1 == 0 || flags & (2 | 0x20000) != 0 {
        return Err("AnyCPU DLL requires ILONLY without 32BITREQUIRED or 32BITPREFERRED".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn payload_requires_every_distributed_file_and_excludes_symbols_and_host_sdk() {
        let complete: BTreeMap<_, _> = REQUIRED
            .iter()
            .enumerate()
            .map(|(index, name)| ((*name).to_owned(), index))
            .collect();
        validate_payload_names(&complete).unwrap();
        for name in REQUIRED {
            let mut files = complete.clone();
            files.remove(*name);
            assert!(validate_payload_names(&files).is_err(), "{name}");
        }
        for name in [
            "TcTidy.VsExtension.pdb",
            "nested/DEBUG.PDB",
            "Microsoft.VisualStudio.DpiAwareness.dll",
            "microsoft.visualstudio.dpiawareness.DLL",
        ] {
            let mut files = complete.clone();
            files.insert(name.to_owned(), files.len());
            assert!(validate_payload_names(&files).is_err(), "{name}");
        }
    }
    #[test]
    fn directory_counts_ignore_signatures_inside_zip_comments() {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer.set_comment("comment containing PK\x05\x06 but no valid directory record");
        let mut archive = writer.finish().unwrap();
        assert_eq!(directory_entries(&mut archive).unwrap(), 0);
    }
    #[test]
    fn rejects_managed_flags_that_force_a_32_bit_process() {
        managed_flags(1).unwrap();
        managed_flags(1 | 8).unwrap();
        for flags in [0, 2, 1 | 2, 1 | 0x20000] {
            assert!(managed_flags(flags).is_err());
        }
    }
    #[test]
    fn source_contract_matches_each_supported_shell() {
        let root = super::super::root();
        for (shell, manifest) in [
            ("15", "source.xae15.extension.vsixmanifest"),
            ("17", "source.extension.vsixmanifest"),
        ] {
            validate_manifest(
                &text(&root.join("editors/vs/TcTidy.VsExtension").join(manifest)).unwrap(),
                shell,
            )
            .unwrap();
        }
        for project in ["TcTidy.VsExtension", "TcTidy.VsExtension.Core"] {
            validate_project(
                &text(&root.join(format!("editors/vs/{project}/{project}.csproj"))).unwrap(),
            )
            .unwrap();
        }
    }
    #[test]
    fn rejects_wrong_targets_and_project_architecture() {
        let source = text(
            &super::super::root()
                .join("editors/vs/TcTidy.VsExtension/source.extension.vsixmanifest"),
        )
        .unwrap();
        for source in [
            source.replace("[17.0,18.0)", "[15.0,18.0)"),
            source.replace("amd64", "x86"),
            source.replace(NAMESPACE, "wrong"),
        ] {
            assert!(validate_manifest(&source, "17").is_err());
        }
        for source in ["<Project/>", "<Project><PlatformTarget>x86</PlatformTarget></Project>", "<Project><PlatformTarget>AnyCPU</PlatformTarget><PlatformTarget>x64</PlatformTarget></Project>"] { assert!(validate_project(source).is_err()); }
    }
    #[test]
    fn rejects_unsafe_and_colliding_paths_in_either_order() {
        for name in [
            "../outside.dll",
            "nested/../../outside.dll",
            "/absolute.dll",
            "C:/drive.dll",
            "a//b",
            "a/./b",
            "a./b",
            "a /b",
            "a\0b",
        ] {
            assert!(archive_names(&[(name.into(), false)]).is_err(), "{name:?}");
        }
        for (a, b) in [
            ("A.dll", "a.dll"),
            ("Straße.txt", "STRASSE.txt"),
            ("a/b", "a\\b"),
            ("parent", "parent/file"),
            ("parent", "parent/"),
        ] {
            let entries = [
                (a.to_owned(), a.ends_with('/')),
                (b.to_owned(), b.ends_with('/')),
            ];
            assert!(archive_names(&entries).is_err());
            assert!(archive_names(&[entries[1].clone(), entries[0].clone()]).is_err());
        }
        archive_names(&[("parent/".into(), true), ("parent/file.txt".into(), false)]).unwrap();
    }
    #[test]
    fn malformed_archives_fail_without_extracting_files() {
        assert!(validate_archive(std::io::Cursor::new(b"not a zip"), "17").is_err());
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .start_file("../outside", zip::write::SimpleFileOptions::default())
            .unwrap();
        assert!(validate_archive(writer.finish().unwrap(), "17").is_err());
        assert!(managed_architecture(b"not a managed assembly").is_err());
    }
}
