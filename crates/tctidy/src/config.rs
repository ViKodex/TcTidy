//! `.tctidy.toml`: loading, validating, discovering, and writing the style
//! configuration.
//!
//! The file is a flat list of `key = value` pairs whose keys are the settings in
//! [`tctidy_format::options::style_options`]. Every key is optional; an absent
//! key keeps the house-style default, so an empty file (or no file at all)
//! formats exactly like the opinionated default.
//!
//! ```toml
//! indent_style = "tabs"
//! keyword_case = "lower"
//! line_width = 120
//! ```
//!
//! Validation is strict: an unknown key, a value of the wrong type, an unknown
//! choice or an out-of-range number is an error, never silently ignored, so a
//! typo cannot quietly fall back to the default. Every problem in the file is
//! reported at once, each with its line and column.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use tctidy_format::options::{
    find_option, style_options, OptionGroup, OptionKind, OptionValue, StyleOption,
};
use tctidy_format::StyleConfig;
use toml::Spanned;

/// The configuration file name looked up by [`find_config`].
pub const CONFIG_FILE_NAME: &str = ".tctidy.toml";

/// One problem found in a configuration file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDiagnostic {
    /// 1-based line, when the problem has a location.
    pub line: Option<usize>,
    /// 1-based column, when the problem has a location.
    pub column: Option<usize>,
    pub message: String,
}

/// A configuration file that could not be read or is invalid. Carries every
/// problem found, not just the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// The file, when the configuration came from one.
    pub path: Option<PathBuf>,
    pub diagnostics: Vec<ConfigDiagnostic>,
}

impl ConfigError {
    fn single(message: String) -> Self {
        ConfigError {
            path: None,
            diagnostics: vec![ConfigDiagnostic {
                line: None,
                column: None,
                message,
            }],
        }
    }

    fn with_path(mut self, path: &Path) -> Self {
        self.path = Some(path.to_path_buf());
        self
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let source = self
            .path
            .as_ref()
            .map_or_else(|| "config".to_owned(), |p| p.display().to_string());
        for (i, d) in self.diagnostics.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            match (d.line, d.column) {
                (Some(l), Some(c)) => write!(f, "{source}:{l}:{c}: {}", d.message)?,
                (Some(l), None) => write!(f, "{source}:{l}: {}", d.message)?,
                _ => write!(f, "{source}: {}", d.message)?,
            }
        }
        Ok(())
    }
}

impl std::error::Error for ConfigError {}

/// Parse and validate configuration text into a [`StyleConfig`], starting from
/// the house-style defaults.
pub fn parse_config(src: &str) -> Result<StyleConfig, ConfigError> {
    let table = parse_table(src)?;
    let mut style = StyleConfig::default();
    let mut diagnostics = Vec::new();
    // Report in file order, not key order.
    let mut entries: Vec<_> = table.iter().collect();
    entries.sort_by_key(|(k, _)| k.span().start);
    for (key, value) in entries {
        let at = |offset: usize, message: String| {
            let (line, column) = line_col(src, offset);
            ConfigDiagnostic {
                line: Some(line),
                column: Some(column),
                message,
            }
        };
        let Some(option) = find_option(key.get_ref()) else {
            let mut message = format!("unknown option `{}`", key.get_ref());
            if let Some(near) = closest_key(key.get_ref()) {
                message.push_str(&format!(" (did you mean `{near}`?)"));
            }
            diagnostics.push(at(key.span().start, message));
            continue;
        };
        let parsed = match value.get_ref() {
            toml::Value::Boolean(b) => Some(OptionValue::Bool(*b)),
            toml::Value::Integer(n) => Some(OptionValue::Integer(*n)),
            toml::Value::String(s) => Some(OptionValue::Choice(s.clone())),
            _ => None,
        };
        let result = match parsed {
            Some(v) => option.set(&mut style, &v).map_err(|e| e.to_string()),
            None => Err(format!("expected {}", describe_kind(option.kind))),
        };
        if let Err(e) = result {
            diagnostics.push(at(
                value.span().start,
                format!("invalid value for `{}`: {e}", option.key),
            ));
        }
    }

    if diagnostics.is_empty() {
        Ok(style)
    } else {
        Err(ConfigError {
            path: None,
            diagnostics,
        })
    }
}

/// The file's `key = value` pairs, with where each key and value stands.
type Table = BTreeMap<Spanned<String>, Spanned<toml::Value>>;

fn parse_table(src: &str) -> Result<Table, ConfigError> {
    toml::from_str(src).map_err(|e| {
        let (line, column) = e
            .span()
            .map(|s| line_col(src, s.start))
            .map_or((None, None), |(l, c)| (Some(l), Some(c)));
        ConfigError {
            path: None,
            diagnostics: vec![ConfigDiagnostic {
                line,
                column,
                message: format!("invalid TOML: {}", e.message().trim_end()),
            }],
        }
    })
}

/// Read and validate a configuration file.
pub fn load_config(path: &Path) -> Result<StyleConfig, ConfigError> {
    let src = std::fs::read_to_string(path)
        .map_err(|e| ConfigError::single(format!("cannot read: {e}")).with_path(path))?;
    parse_config(&src).map_err(|e| e.with_path(path))
}

/// Find the configuration that applies to files in `dir`: the nearest
/// [`CONFIG_FILE_NAME`] in `dir` or any of its ancestors.
pub fn find_config(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .map(|d| d.join(CONFIG_FILE_NAME))
        .find(|candidate| candidate.is_file())
}

/// Render `style` as `.tctidy.toml` text, grouped by section. With
/// `include_defaults`, every setting is written; otherwise only the ones that
/// differ from the house style (an all-default style renders as just the
/// header). The output always parses back to `style` via [`parse_config`].
pub fn render_config(style: &StyleConfig, include_defaults: bool) -> String {
    let mut out = String::from(
        "# TcTidy formatting configuration.\n\
         # Settings left out use the TcTidy defaults.\n",
    );
    for &group in OptionGroup::ALL {
        let lines: Vec<String> = style_options()
            .iter()
            .filter(|o| o.group == group)
            .filter(|o| include_defaults || o.get(style) != o.default_value())
            .map(|o| render_entry(o, style))
            .collect();
        if !lines.is_empty() {
            out.push_str(&format!("\n# {}\n", group.label()));
            for line in lines {
                out.push_str(&line);
                out.push('\n');
            }
        }
    }
    out
}

fn render_entry(option: &StyleOption, style: &StyleConfig) -> String {
    format!("{} = {}", option.key, option.get(style))
}

/// Rewrite the configuration text `src` so that it describes `style`, keeping
/// what a person wrote in it.
///
/// A file that is exactly what [`render_config`] writes for the style it holds
/// (with or without the defaults) is rendered afresh in the same form, and so
/// is an empty one. Any other file is edited in place, so its comments, order
/// and layout survive: a changed value is replaced where it stands, a setting
/// set back to its default is removed with its line, and a newly set one is
/// appended at the end. Line endings follow `src`.
///
/// Fails like [`parse_config`] when `src` is not a valid configuration.
pub fn update_config(src: &str, style: &StyleConfig) -> Result<String, ConfigError> {
    let current = parse_config(src)?;
    let eol = if src.contains("\r\n") { "\r\n" } else { "\n" };
    let unix = src.replace("\r\n", "\n");
    let generated = if unix.trim().is_empty() {
        Some(false)
    } else {
        [false, true]
            .into_iter()
            .find(|&all| unix == render_config(&current, all))
    };
    if let Some(include_defaults) = generated {
        return Ok(render_config(style, include_defaults).replace('\n', eol));
    }

    // A valid file is a flat table of known settings.
    let table = parse_table(src)?;
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for (key, value) in &table {
        let Some(option) = find_option(key.get_ref()) else {
            continue;
        };
        let wanted = option.get(style);
        if wanted == option.get(&current) {
            continue;
        }
        if wanted == option.default_value() {
            let start = src[..key.span().start].rfind('\n').map_or(0, |i| i + 1);
            let end = src[value.span().end..]
                .find('\n')
                .map_or(src.len(), |i| value.span().end + i + 1);
            edits.push((start..end, String::new()));
        } else {
            edits.push((value.span(), wanted.to_string()));
        }
    }

    let mut out = src.to_owned();
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    for (range, text) in edits {
        out.replace_range(range, &text);
    }
    let present = |key: &str| table.keys().any(|k| k.get_ref() == key);
    let added: Vec<String> = style_options()
        .iter()
        .filter(|o| !present(o.key) && o.get(style) != o.default_value())
        .map(|o| render_entry(o, style))
        .collect();
    if !added.is_empty() {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(eol);
        }
        for line in added {
            out.push_str(&line);
            out.push_str(eol);
        }
    }
    Ok(out)
}

fn describe_kind(kind: OptionKind) -> String {
    match kind {
        OptionKind::Bool => "a boolean".to_owned(),
        OptionKind::Integer { min, max } => format!("an integer between {min} and {max}"),
        OptionKind::Choice(_) => "a string".to_owned(),
    }
}

/// 1-based line and column (in characters) of a byte offset.
fn line_col(src: &str, offset: usize) -> (usize, usize) {
    let before = &src[..offset.min(src.len())];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, before[line_start..].chars().count() + 1)
}

/// The known key closest to a misspelled one, if any is reasonably close.
fn closest_key(key: &str) -> Option<&'static str> {
    style_options()
        .iter()
        .map(|o| (edit_distance(key, o.key), o.key))
        .filter(|(d, k)| *d <= (k.len() / 3).max(2))
        .min_by_key(|(d, _)| *d)
        .map(|(_, k)| k)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tctidy_format::style::KeywordCase;
    use tctidy_pretty::IndentStyle;

    #[test]
    fn empty_config_is_the_house_style() {
        assert_eq!(parse_config("").unwrap(), StyleConfig::default());
        assert_eq!(
            parse_config("# just a comment\n").unwrap(),
            StyleConfig::default()
        );
    }

    #[test]
    fn values_are_applied() {
        let style = parse_config(
            "indent_style = \"tabs\"\nkeyword_case = \"lower\"\nline_width = 120\nspace_after_comma = false\n",
        )
        .unwrap();
        assert_eq!(style.indent_style, IndentStyle::Tabs);
        assert_eq!(style.keyword_case, KeywordCase::Lower);
        assert_eq!(style.line_width, 120);
        assert!(!style.space_after_comma);
    }

    #[test]
    fn every_problem_is_reported_with_its_location() {
        let err = parse_config(
            "keyword_cse = \"lower\"\nindent_width = 0\nkeyword_case = \"camel\"\nspace_after_comma = \"yes\"\nline_width = 1.5\n",
        )
        .unwrap_err();
        let lines: Vec<_> = err.to_string().lines().map(str::to_owned).collect();
        assert_eq!(
            lines,
            [
                "config:1:1: unknown option `keyword_cse` (did you mean `keyword_case`?)",
                "config:2:16: invalid value for `indent_width`: expected an integer between 1 and 16",
                "config:3:16: invalid value for `keyword_case`: expected one of \"upper\", \"lower\", \"preserve\"",
                "config:4:21: invalid value for `space_after_comma`: expected a boolean",
                "config:5:14: invalid value for `line_width`: expected an integer between 20 and 500",
            ]
        );
    }

    #[test]
    fn internal_fields_are_not_configurable() {
        // In StyleConfig, but not a setting: keeping the source's breaks would
        // break idempotence (see tctidy_format::options).
        let err = parse_config("preserve_line_breaks = true").unwrap_err();
        assert!(err
            .to_string()
            .contains("unknown option `preserve_line_breaks`"));
    }

    #[test]
    fn tables_and_arrays_are_rejected() {
        let err = parse_config("[format]\nindent_width = 2\n").unwrap_err();
        assert!(err.to_string().contains("unknown option `format`"), "{err}");
        let err = parse_config("indent_width = [2]\n").unwrap_err();
        assert!(err.to_string().contains("expected an integer"), "{err}");
    }

    #[test]
    fn syntax_errors_are_located() {
        let err = parse_config("indent_width = \n").unwrap_err();
        let d = &err.diagnostics[0];
        assert_eq!(d.line, Some(1));
        assert!(d.message.starts_with("invalid TOML"), "{}", d.message);
    }

    #[test]
    fn render_round_trips() {
        let mut style = StyleConfig::default();
        assert_eq!(parse_config(&render_config(&style, true)).unwrap(), style);
        assert_eq!(parse_config(&render_config(&style, false)).unwrap(), style);

        style.indent_style = IndentStyle::Tabs;
        style.blank_lines_max = 0;
        style.wrap_before_operator = false;
        let minimal = render_config(&style, false);
        assert_eq!(
            minimal,
            "# TcTidy formatting configuration.\n# Settings left out use the TcTidy defaults.\n\n# Indentation & width\nindent_style = \"tabs\"\n\n# Line wrapping\nwrap_before_operator = false\n\n# Blank lines\nblank_lines_max = 0\n"
        );
        assert_eq!(parse_config(&minimal).unwrap(), style);
        assert_eq!(parse_config(&render_config(&style, true)).unwrap(), style);
    }

    fn tabs_and_lower() -> StyleConfig {
        StyleConfig {
            indent_style: IndentStyle::Tabs,
            keyword_case: KeywordCase::Lower,
            ..StyleConfig::default()
        }
    }

    #[test]
    fn update_renders_a_generated_file_afresh() {
        let before = StyleConfig {
            line_width: 120,
            ..StyleConfig::default()
        };
        let style = tabs_and_lower();
        for all in [false, true] {
            let src = render_config(&before, all);
            assert_eq!(
                update_config(&src, &style).unwrap(),
                render_config(&style, all)
            );
        }
        assert_eq!(
            update_config("", &style).unwrap(),
            render_config(&style, false)
        );
        assert_eq!(
            update_config("\r\n", &style).unwrap(),
            render_config(&style, false).replace('\n', "\r\n")
        );
    }

    #[test]
    fn update_keeps_what_a_person_wrote() {
        let src = "# Team style\nline_width = 120 # wide screens\n\n# Agreed in review\nindent_style = \"spaces\"\nkeyword_case = \"upper\"\n";
        let mut style = tabs_and_lower();
        style.space_after_comma = false;
        let out = update_config(src, &style).unwrap();
        assert_eq!(
            out,
            "# Team style\n\n# Agreed in review\nindent_style = \"tabs\"\nkeyword_case = \"lower\"\nspace_after_comma = false\n"
        );
        assert_eq!(parse_config(&out).unwrap(), style);

        // Nothing to change: byte for byte the same.
        assert_eq!(update_config(&out, &style).unwrap(), out);
    }

    #[test]
    fn update_follows_the_file_line_endings() {
        let src = "# ours\r\nline_width = 120\r\nindent_width = 2";
        let style = StyleConfig {
            indent_width: 2,
            blank_lines_max: 1,
            ..StyleConfig::default()
        };
        assert_eq!(
            update_config(src, &style).unwrap(),
            "# ours\r\nindent_width = 2\r\nblank_lines_max = 1\r\n"
        );
    }

    #[test]
    fn update_refuses_an_invalid_file() {
        assert!(update_config("indent_width = 99\n", &StyleConfig::default()).is_err());
    }

    #[test]
    fn discovery_walks_up_to_the_nearest_file() {
        let root = std::env::temp_dir().join(format!("tctidy-config-{}", std::process::id()));
        let nested = root.join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(find_config(&nested).filter(|p| p.starts_with(&root)), None);

        std::fs::write(root.join(CONFIG_FILE_NAME), "").unwrap();
        assert_eq!(find_config(&nested), Some(root.join(CONFIG_FILE_NAME)));

        std::fs::write(root.join("a").join(CONFIG_FILE_NAME), "").unwrap();
        assert_eq!(
            find_config(&nested),
            Some(root.join("a").join(CONFIG_FILE_NAME))
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn load_reports_the_path() {
        let path = std::env::temp_dir().join(format!("tctidy-load-{}.toml", std::process::id()));
        std::fs::write(&path, "indent_width = 99\n").unwrap();
        let err = load_config(&path).unwrap_err();
        assert!(
            err.to_string()
                .starts_with(&format!("{}:1:16:", path.display())),
            "{err}"
        );
        std::fs::remove_file(&path).unwrap();

        let err = load_config(&path).unwrap_err();
        assert!(err.to_string().contains("cannot read"), "{err}");
    }
}
