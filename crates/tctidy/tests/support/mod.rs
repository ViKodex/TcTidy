//! Shared by the corpus tests: where the corpus is, which files it holds, the
//! styles the invariants run under, and the public-corpus files that are known
//! to fail.
//!
//! The corpus is `TCTIDY_CORPUS` (absolute, or relative to the repository root)
//! when it is set, else the public corpus that `cargo xtask corpus fetch` checks out
//! under `target/public-corpus`. With neither, the corpus tests skip.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use tctidy::{format_twincat_file, FormatOptions, FormatOutput, StyleConfig};
use tctidy_format::options::{style_options, OptionKind, OptionValue};

#[path = "../../../../xtask/src/scramble.rs"]
pub mod scramble;

/// Public-corpus files that TcTidy refuses to format, grouped by cause. Remove a
/// file once it formats: the tests fail on a listed file that formats, so the list
/// stays accurate. Keys are paths relative to the corpus root.
const KNOWN_FAILURES: &[&str] = &[
    // Not a TcTidy bug: the source ends with an incomplete statement (`AsBool :=`).
    "tcopen/src/TcoCore/src/XaeTcoCore/TcoCore/POUs/Prototypes/TcoTask/TaskResult/TcoTaskResult.TcPOU",
];

/// The corpus directory, or `None` (after saying why) when there is none.
pub fn corpus_dir(test: &str) -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    match std::env::var_os("TCTIDY_CORPUS") {
        Some(value) if !value.is_empty() => {
            let dir = root.join(value);
            assert!(
                dir.is_dir(),
                "TCTIDY_CORPUS={} is not a directory",
                dir.display()
            );
            Some(dir)
        }
        _ => {
            let dir = root.join("target/public-corpus");
            if dir.is_dir() {
                Some(dir)
            } else {
                eprintln!(
                    "{test}: skipped, no corpus. Run `cargo xtask corpus fetch`, or set \
                     TCTIDY_CORPUS to a folder of TwinCAT files."
                );
                None
            }
        }
    }
}

/// Every TwinCAT object file under `dir` with its key (its path relative to `dir`,
/// `/`-separated), sorted by key.
pub fn twincat_files(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    collect(dir, &mut files);
    let mut keyed: Vec<(PathBuf, String)> = files
        .into_iter()
        .map(|path| {
            let key = path
                .strip_prefix(dir)
                .unwrap_or(&path)
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            (path, key)
        })
        .collect();
    keyed.sort_by(|a, b| a.1.cmp(&b.1));
    assert!(
        !keyed.is_empty(),
        "no TwinCAT files under {}",
        dir.display()
    );
    keyed
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != ".git") {
                collect(&path, out);
            }
        } else if path.extension().is_some_and(|e| {
            let e = e.to_string_lossy().to_ascii_lowercase();
            ["tcpou", "tcgvl", "tcdut", "tcio"].contains(&e.as_str())
        }) {
            out.push(path);
        }
    }
}

/// A style the corpus invariants run under, named in failure messages.
pub struct Style {
    pub name: &'static str,
    pub options: FormatOptions,
}

/// The default style, then two non-default profiles built from the settings a
/// `.tctidy.toml` can set, so a setting joins them as soon as it is
/// configurable: `low` gives every setting its first non-default value (the
/// minimum, for a number), `high` its last (the maximum).
///
/// With `canonical`, the `preserve` choices are left out: they keep the input's
/// keyword case, which the canonicalization test scrambles on purpose.
pub fn styles(canonical: bool) -> Vec<Style> {
    let profile = |pick_last: bool| {
        let mut style = StyleConfig::default();
        for option in style_options() {
            let default = option.default_value();
            let mut values: Vec<OptionValue> = match option.kind {
                OptionKind::Bool => vec![OptionValue::Bool(false), OptionValue::Bool(true)],
                OptionKind::Integer { min, max } => vec![
                    OptionValue::Integer(min.into()),
                    OptionValue::Integer(max.into()),
                ],
                OptionKind::Choice(names) => names
                    .iter()
                    .filter(|name| !(canonical && **name == "preserve"))
                    .map(|name| OptionValue::Choice((*name).to_owned()))
                    .collect(),
            };
            values.retain(|value| *value != default);
            let value = if pick_last {
                values.last()
            } else {
                values.first()
            };
            if let Some(value) = value {
                option
                    .set(&mut style, value)
                    .expect("a boundary value is accepted");
            }
        }
        FormatOptions { style }
    };
    vec![
        Style {
            name: "default",
            options: FormatOptions::default(),
        },
        Style {
            name: "low",
            options: profile(false),
        },
        Style {
            name: "high",
            options: profile(true),
        },
    ]
}

/// Tallies one invariant over the corpus, under every style, and fails the test
/// with every offending file.
pub struct Report {
    name: &'static str,
    checked: usize,
    known: usize,
    failures: Vec<String>,
}

impl Report {
    pub fn new(name: &'static str) -> Self {
        Report {
            name,
            checked: 0,
            known: 0,
            failures: Vec::new(),
        }
    }

    /// Format `bytes` in `style`. A refusal is a failure unless the file is a
    /// known failure; a known failure that formats is a failure too.
    pub fn format(&mut self, style: &Style, key: &str, bytes: &[u8]) -> Option<FormatOutput> {
        let known = KNOWN_FAILURES.contains(&key);
        match format_twincat_file(bytes, &style.options) {
            Ok(out) => {
                if known {
                    self.failures.push(format!(
                        "{key} ({}): formats now, remove it from KNOWN_FAILURES",
                        style.name
                    ));
                }
                Some(out)
            }
            Err(_) if known => {
                self.known += 1;
                None
            }
            Err(error) => {
                self.fail(style, key, &format!("refused to format: {error}"));
                None
            }
        }
    }

    pub fn pass(&mut self) {
        self.checked += 1;
    }

    pub fn fail(&mut self, style: &Style, key: &str, why: &str) {
        self.checked += 1;
        self.failures.push(format!("{key} ({}): {why}", style.name));
    }

    pub fn finish(self) {
        eprintln!(
            "{}: {} file checks, {} failed, {} known failures skipped",
            self.name,
            self.checked,
            self.failures.len(),
            self.known
        );
        assert!(
            self.failures.is_empty(),
            "{}: {} of {} file checks fail:\n  {}",
            self.name,
            self.failures.len(),
            self.checked,
            self.failures.join("\n  ")
        );
    }
}
