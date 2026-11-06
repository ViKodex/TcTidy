//! The public corpus: real TwinCAT projects pinned in `xtask/public-corpus.txt`.
//!
//! `cargo xtask corpus fetch` checks each one out at its pinned commit under
//! `target/public-corpus/<name>`, keeping only the TwinCAT object files (a shallow,
//! sparse, blob-filtered fetch, so only those files are downloaded). The corpus
//! tests and the corpus tools read `TCTIDY_CORPUS` when it is set, and this
//! directory otherwise.

use std::path::{Path, PathBuf};
use std::process::Command;

const SOURCES: &str = "xtask/public-corpus.txt";
const DEFAULT_DIR: &str = "target/public-corpus";
const PATTERNS: [&str; 4] = ["*.TcPOU", "*.TcGVL", "*.TcDUT", "*.TcIO"];

#[derive(Debug)]
struct Source {
    name: String,
    url: String,
    commit: String,
}

/// The corpus directory: `TCTIDY_CORPUS` (absolute, or relative to the repository
/// root) when set, else `target/public-corpus`.
pub fn dir(root: &Path) -> PathBuf {
    match std::env::var_os("TCTIDY_CORPUS") {
        Some(value) if !value.is_empty() => root.join(value),
        _ => root.join(DEFAULT_DIR),
    }
}

pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("fetch") if args.len() == 1 => fetch(root),
        _ => Err("usage: xtask corpus fetch".to_string()),
    }
}

fn fetch(root: &Path) -> Result<(), String> {
    let sources = parse_sources(
        &std::fs::read_to_string(root.join(SOURCES))
            .map_err(|error| format!("read {SOURCES}: {error}"))?,
    )?;
    let base = root.join(DEFAULT_DIR);
    std::fs::create_dir_all(&base)
        .map_err(|error| format!("create {}: {error}", base.display()))?;
    for source in &sources {
        let dest = base.join(&source.name);
        if head(&dest).as_deref() == Some(source.commit.as_str()) {
            println!("{}: up to date at {}", source.name, &source.commit[..12]);
            continue;
        }
        if dest.exists() {
            std::fs::remove_dir_all(&dest)
                .map_err(|error| format!("remove {}: {error}", dest.display()))?;
        }
        checkout(source, &dest)?;
        println!(
            "{}: {} files at {}",
            source.name,
            count_files(&dest),
            &source.commit[..12]
        );
    }
    Ok(())
}

fn parse_sources(text: &str) -> Result<Vec<Source>, String> {
    let mut sources = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [name, url, commit, _license] = fields[..] else {
            return Err(format!(
                "{SOURCES}:{}: expected `name repository commit license`",
                number + 1
            ));
        };
        let is_name = name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if name.is_empty() || !is_name {
            return Err(format!("{SOURCES}:{}: invalid name `{name}`", number + 1));
        }
        if commit.len() != 40 || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!(
                "{SOURCES}:{}: `{commit}` is not a full commit hash",
                number + 1
            ));
        }
        sources.push(Source {
            name: name.to_string(),
            url: url.to_string(),
            commit: commit.to_ascii_lowercase(),
        });
    }
    if sources.is_empty() {
        return Err(format!("{SOURCES} lists no source"));
    }
    Ok(sources)
}

fn checkout(source: &Source, dest: &Path) -> Result<(), String> {
    git(None, &["init", "--quiet", &dest.to_string_lossy()])?;
    // Keep the files byte for byte (no CRLF conversion) and allow long Windows paths.
    git(Some(dest), &["config", "core.autocrlf", "false"])?;
    git(Some(dest), &["config", "core.longpaths", "true"])?;
    git(Some(dest), &["remote", "add", "origin", &source.url])?;
    let mut sparse = vec!["sparse-checkout", "set", "--no-cone"];
    sparse.extend(PATTERNS);
    git(Some(dest), &sparse)?;
    git(
        Some(dest),
        &[
            "fetch",
            "--quiet",
            "--depth",
            "1",
            "--filter=blob:none",
            "origin",
            &source.commit,
        ],
    )?;
    git(
        Some(dest),
        &[
            "-c",
            "advice.detachedHead=false",
            "checkout",
            "--quiet",
            "FETCH_HEAD",
        ],
    )
}

fn head(dest: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dest)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git(dir: Option<&Path>, args: &[&str]) -> Result<(), String> {
    let mut command = Command::new("git");
    if let Some(dir) = dir {
        command.arg("-C").arg(dir);
    }
    let status = command
        .args(args)
        .status()
        .map_err(|error| format!("run git: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("git {} failed ({status})", args.join(" ")))
    }
}

fn count_files(dir: &Path) -> usize {
    let mut files = Vec::new();
    crate::collect_twincat_files(dir, &mut files);
    files.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_sources_parse() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let text = std::fs::read_to_string(root.join(SOURCES)).unwrap();
        let sources = parse_sources(&text).unwrap();
        assert!(sources.iter().any(|s| s.name == "tcopen"));
        assert!(sources.iter().any(|s| s.name == "tcunit"));
    }

    #[test]
    fn a_short_commit_is_rejected() {
        let error = parse_sources("x https://example.invalid/x 1234567 MIT\n").unwrap_err();
        assert!(error.contains("not a full commit hash"), "{error}");
    }

    #[test]
    fn a_missing_field_is_rejected() {
        let error = parse_sources("x https://example.invalid/x\n").unwrap_err();
        assert!(error.contains("expected"), "{error}");
    }
}
