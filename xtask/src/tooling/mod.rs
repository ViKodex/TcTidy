//! Release checks shared by local builds and CI. None execute payload assemblies.

#[cfg(test)]
mod contracts;
pub(crate) mod integrity;
mod licenses;
mod vsix;

use clap::{Parser, Subcommand};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub(super) type Result<T = ()> = std::result::Result<T, String>;

#[derive(Parser)]
#[command(name = "cargo xtask")]
struct Cli {
    #[command(subcommand)]
    command: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Generate or validate the exact licenses of distributed files.
    Licenses {
        #[command(subcommand)]
        command: licenses::Command,
    },
    /// Check extension source and archive compatibility.
    Vsix {
        #[command(subcommand)]
        command: vsix::Command,
    },
}

pub(crate) fn run(args: &[String]) -> Result {
    let cli = Cli::parse_from(std::iter::once("xtask".to_owned()).chain(args.iter().cloned()));
    match cli.command {
        Task::Licenses { command } => licenses::run(command),
        Task::Vsix { command } => vsix::run(command),
    }
}

#[cfg(test)]
pub(super) fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned()
}

pub(super) fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

pub(super) fn text(path: &Path) -> Result<String> {
    String::from_utf8(read(path)?)
        .map(|text| text.trim_start_matches('\u{feff}').to_owned())
        .map_err(|error| format!("invalid UTF-8 in {}: {error}", path.display()))
}

pub(super) fn json(path: &Path) -> Result<Value> {
    parse_json(&text(path)?, &path.display().to_string())
}

pub(super) fn parse_json(text: &str, label: &str) -> Result<Value> {
    serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|error| format!("invalid JSON in {label}: {error}"))
}

pub(super) fn array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{key} must be an array"))
}

pub(super) fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("{key} must be a non-empty string"))
}

pub(super) fn write(path: &Path, bytes: &[u8]) -> Result {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, bytes).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

pub(super) fn write_json(path: &Path, value: &Value) -> Result {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    write(path, &bytes)
}

pub(super) fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Lower-case hexadecimal, as in `.sha256` files.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

pub(super) fn file_digest(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex(&hasher.finalize()))
}

/// Apply Windows-safe rules even when the validation host is Linux.
pub(super) fn relative(value: &str) -> Result<String> {
    let path = value.replace('\\', "/");
    if path.is_empty()
        || path.contains(':')
        || path.chars().any(char::is_control)
        || path.split('/').any(|part| {
            part.is_empty() || part == "." || part == ".." || part.ends_with(['.', ' '])
        })
    {
        return Err(format!("unsafe relative path: {value:?}"));
    }
    Ok(path)
}

pub(super) fn payload_file(payload: &Path, name: &str) -> Result<PathBuf> {
    let path = payload.join(relative(name)?);
    if !path.is_file()
        || fs::symlink_metadata(&path)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
    {
        return Err(format!("missing or unsafe payload file: {name}"));
    }
    // Reject an ancestor symlink that could redirect a license reference outside the payload.
    let base = payload.canonicalize().map_err(|e| e.to_string())?;
    if !path
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(&base)
    {
        return Err(format!("payload reference escapes its directory: {name}"));
    }
    Ok(path)
}

pub(super) fn files(directory: &Path) -> Result<BTreeSet<String>> {
    fn visit(root: &Path, directory: &Path, found: &mut BTreeSet<String>) -> Result {
        for entry in fs::read_dir(directory).map_err(|e| format!("{}: {e}", directory.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                return Err(format!("symlink in payload: {}", entry.path().display()));
            }
            if kind.is_dir() {
                visit(root, &entry.path(), found)?;
            } else if kind.is_file() {
                let name = entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                relative(&name)?;
                found.insert(name);
            } else {
                return Err("non-file entry in payload".into());
            }
        }
        Ok(())
    }
    let mut found = BTreeSet::new();
    visit(directory, directory, &mut found)?;
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn digest_is_lower_case_hex() {
        assert_eq!(
            digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
    #[test]
    fn references_are_safe_on_both_platforms() {
        for name in [
            "",
            "../LICENSE",
            "/LICENSE",
            "C:\\LICENSE",
            "a/../b",
            "a//b",
            "a./b",
            "a /b",
            "a\0b",
        ] {
            assert!(relative(name).is_err(), "{name:?}");
        }
        assert_eq!(
            relative("licenses\\NOTICE.txt").unwrap(),
            "licenses/NOTICE.txt"
        );
        assert_eq!(
            relative("[Content_Types].xml").unwrap(),
            "[Content_Types].xml"
        );
    }
}
