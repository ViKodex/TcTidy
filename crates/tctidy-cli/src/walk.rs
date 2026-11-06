//! Batch file discovery: expand globs and recurse directories, filtering to
//! TwinCAT object files (`.TcPOU`/`.TcGVL`/`.TcDUT`/`.TcIO`). An explicitly named
//! file is always included; directories and globs keep only TwinCAT files.

use std::path::{Path, PathBuf};

const TWINCAT_EXTS: &[&str] = &["TcPOU", "TcGVL", "TcDUT", "TcIO"];

/// Whether `p` has a TwinCAT object-file extension (case-insensitive).
pub fn is_twincat_path(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| TWINCAT_EXTS.iter().any(|t| e.eq_ignore_ascii_case(t)))
}

/// The expanded path arguments: the concrete files, and the discovery problems
/// met on the way. Each problem is already reported to stderr; the run still
/// processes `files`, but must not exit 0.
#[derive(Default)]
pub struct Targets {
    pub files: Vec<PathBuf>,
    /// A literal path named something that does not exist (exit 3).
    pub missing: bool,
    /// A directory or glob walk could not read part of its tree (exit 3).
    pub walk_error: bool,
    /// A glob pattern is not valid glob syntax: a usage error (exit 2).
    pub invalid_glob: bool,
}

/// Expand the path arguments into a concrete file list. Missing paths, invalid
/// globs and walk errors are reported to stderr and flagged in [`Targets`]
/// (never panics); every other target is still collected. A valid glob
/// matching nothing is not an error, even when its literal directory prefix
/// does not exist; a nonexistent literal path is. A walk error names the path
/// it could not read, or else the walk root.
///
/// An existing file or directory is taken **literally**, even when its name
/// contains a glob metacharacter (`[`, `]`, `*`, `?` are all legal on Windows, so
/// a `[NN]-Name` folder must not be mistaken for a pattern). Only a path that does
/// not exist on disk is expanded as a glob.
///
/// Caveat: that fallback means a *typo* in a path containing a metacharacter
/// (`[02]-Nmae/X.TcPOU`) is expanded as a glob, matches nothing and is not
/// flagged missing. Only metacharacter-free typos get the exit-3 treatment.
pub fn collect_targets(paths: &[String]) -> Targets {
    let mut targets = Targets::default();
    for arg in paths {
        let p = Path::new(arg);
        if p.is_file() {
            targets.files.push(p.to_path_buf());
        } else if p.is_dir() {
            walk_dir(p, &mut targets);
        } else if is_glob(arg) {
            expand_glob(arg, &mut targets);
        } else {
            eprintln!("tctidy: no such file or directory: {arg}");
            targets.missing = true;
        }
    }
    // Overlapping arguments (a file named literally and matched by a glob or
    // directory) must format each target exactly once. Identity is the
    // canonical path, so spelling differences collapse too.
    let mut seen = std::collections::HashSet::new();
    targets
        .files
        .retain(|p| seen.insert(p.canonicalize().unwrap_or_else(|_| p.clone())));
    targets
}

fn is_glob(s: &str) -> bool {
    s.contains(['*', '?', '['])
}

fn walk_dir(dir: &Path, targets: &mut Targets) {
    drain_walk(dir, walk(dir), |_| true, targets);
}

fn expand_glob(pattern: &str, targets: &mut Targets) {
    let glob = match globset::Glob::new(pattern) {
        Ok(glob) => glob,
        Err(e) => {
            eprintln!("tctidy: {pattern}: invalid glob pattern: {}", e.kind());
            targets.invalid_glob = true;
            return;
        }
    };
    let matcher = glob.compile_matcher();
    let base = glob_base(pattern);
    // A literal prefix that does not exist holds no matches, which is not an
    // error for a glob. Any other failure to read it is a walk error.
    if let Ok(false) = base.try_exists() {
        return;
    }
    let keep = |p: &Path| {
        let name_match = p.file_name().is_some_and(|n| matcher.is_match(n));
        matcher.is_match(p) || name_match
    };
    drain_walk(&base, walk(&base), keep, targets);
}

/// Every path under `root`, hidden and git-ignored files included, or the
/// error that cut off part of the walk.
fn walk(root: &Path) -> impl Iterator<Item = Result<PathBuf, ignore::Error>> {
    ignore::WalkBuilder::new(root)
        .standard_filters(false)
        .build()
        .map(|entry| entry.map(ignore::DirEntry::into_path))
}

/// Collect the TwinCAT files of a walk that `keep` accepts. Each walk error is
/// reported and flagged, and the walk goes on with the rest of the tree.
fn drain_walk(
    root: &Path,
    walk: impl IntoIterator<Item = Result<PathBuf, ignore::Error>>,
    keep: impl Fn(&Path) -> bool,
    targets: &mut Targets,
) {
    for entry in walk {
        match entry {
            Ok(p) => {
                if p.is_file() && is_twincat_path(&p) && keep(&p) {
                    targets.files.push(p);
                }
            }
            Err(err) => {
                eprintln!("tctidy: {}", walk_error_line(root, &err));
                targets.walk_error = true;
            }
        }
    }
}

/// `<path>: <cause>` for a walk error: the path the walker could not read, or
/// the walk root when the error names none, then the underlying cause alone
/// (the walker's own wrapping repeats the path).
fn walk_error_line(root: &Path, err: &ignore::Error) -> String {
    let (path, cause) = split_walk_error(err);
    format!("{}: {cause}", path.unwrap_or(root).display())
}

fn split_walk_error(err: &ignore::Error) -> (Option<&Path>, String) {
    match err {
        ignore::Error::WithPath { path, err } => {
            let (inner, cause) = split_walk_error(err);
            (inner.or(Some(path)), cause)
        }
        ignore::Error::WithDepth { err, .. } => split_walk_error(err),
        ignore::Error::Io(io) => {
            let mut cause: &(dyn std::error::Error + 'static) = io;
            while let Some(next) = cause.source() {
                cause = next;
            }
            (None, cause.to_string())
        }
        other => (None, other.to_string()),
    }
}

/// The literal directory prefix before the first glob component (the walk root).
fn glob_base(pattern: &str) -> PathBuf {
    let mut base = PathBuf::new();
    for comp in Path::new(pattern).components() {
        if is_glob(&comp.as_os_str().to_string_lossy()) {
            break;
        }
        base.push(comp);
    }
    if base.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twincat_extensions_match_case_insensitively() {
        for name in [
            "a.TcPOU", "a.tcpou", "a.TCPOU", "a.TcGVL", "a.tcgvl", "a.TcDUT", "a.tcdut", "a.TcIO",
        ] {
            assert!(is_twincat_path(Path::new(name)), "{name} must match");
        }
        for name in ["a.txt", "a.st", "a.xml", "TcPOU", "a", "a.TcPOUx"] {
            assert!(!is_twincat_path(Path::new(name)), "{name} must not match");
        }
    }

    #[test]
    fn glob_base_is_the_literal_prefix() {
        assert_eq!(glob_base("sub/dir/*.TcPOU"), PathBuf::from("sub/dir"));
        assert_eq!(glob_base("*.TcPOU"), PathBuf::from("."));
        assert_eq!(glob_base("sub/*/deep.TcPOU"), PathBuf::from("sub"));
    }

    /// A scratch directory tree with TwinCAT and non-TwinCAT files.
    fn scratch_tree(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("tctidy-walk-{tag}-{}", std::process::id()));
        let sub = root.join("nested");
        std::fs::create_dir_all(&sub).unwrap();
        for name in ["A.TcPOU", "B.TcGVL", "lower.tcdut", "skip.txt", "skip.st"] {
            std::fs::write(root.join(name), "x").unwrap();
        }
        std::fs::write(sub.join("Deep.TcDUT"), "x").unwrap();
        root
    }

    fn file_names(files: &[PathBuf]) -> Vec<String> {
        let mut names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn directory_argument_recurses_and_keeps_only_twincat_files() {
        let root = scratch_tree("dir");
        let targets = collect_targets(&[root.display().to_string()]);
        assert!(!targets.missing);
        assert_eq!(
            file_names(&targets.files),
            ["A.TcPOU", "B.TcGVL", "Deep.TcDUT", "lower.tcdut"],
            "recursion must pick TwinCAT files (case-insensitive) and skip the rest"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn explicitly_named_non_twincat_file_is_included() {
        let root = scratch_tree("explicit");
        let txt = root.join("skip.txt");
        let targets = collect_targets(&[txt.display().to_string()]);
        assert_eq!(targets.files, vec![txt]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn glob_expands_relative_to_its_base_directory() {
        let root = scratch_tree("glob");
        // Forward slashes so the pattern is separator-portable for globset.
        let pattern = format!("{}/*.TcPOU", root.display().to_string().replace('\\', "/"));
        let targets = collect_targets(&[pattern]);
        assert!(!targets.missing);
        assert_eq!(file_names(&targets.files), ["A.TcPOU"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn glob_matching_nothing_is_empty_but_not_missing() {
        let root = scratch_tree("noglob");
        let pattern = format!(
            "{}/Nothing*.TcPOU",
            root.display().to_string().replace('\\', "/")
        );
        let targets = collect_targets(&[pattern]);
        assert!(targets.files.is_empty());
        assert!(!targets.missing, "an empty glob match is not an error");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn nonexistent_literal_path_is_flagged_missing() {
        let root = scratch_tree("missing");
        let gone = root.join("Gone.TcPOU");
        let real = root.join("A.TcPOU");
        let targets = collect_targets(&[gone.display().to_string(), real.display().to_string()]);
        assert!(
            targets.missing,
            "a nonexistent literal path must be flagged"
        );
        assert_eq!(
            file_names(&targets.files),
            ["A.TcPOU"],
            "the existing paths must still be collected"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mixed_argument_list_collects_each_target_once() {
        let root = scratch_tree("mixed");
        let file = root.join("A.TcPOU").display().to_string();
        let dir = root.join("nested").display().to_string();
        let pattern = format!("{}/*.TcGVL", root.display().to_string().replace('\\', "/"));
        let targets = collect_targets(&[file, dir, pattern]);
        assert!(!targets.missing);
        assert_eq!(
            file_names(&targets.files),
            ["A.TcPOU", "B.TcGVL", "Deep.TcDUT"],
            "one target per argument kind, nothing twice"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn invalid_glob_is_flagged_and_the_other_targets_are_still_collected() {
        let root = scratch_tree("badglob");
        let bad = format!("{}/[", root.display().to_string().replace('\\', "/"));
        let real = root.join("A.TcPOU").display().to_string();
        let targets = collect_targets(&[bad, real]);
        assert!(targets.invalid_glob, "an invalid glob must be flagged");
        assert!(!targets.missing && !targets.walk_error);
        assert_eq!(file_names(&targets.files), ["A.TcPOU"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn glob_under_a_missing_directory_is_no_problem() {
        let root = scratch_tree("globnodir");
        let pattern = format!(
            "{}/absent/*.TcPOU",
            root.display().to_string().replace('\\', "/")
        );
        let targets = collect_targets(&[pattern]);
        assert!(targets.files.is_empty());
        assert!(
            !targets.missing && !targets.walk_error && !targets.invalid_glob,
            "a valid glob matching nothing is not an error"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    fn denied() -> ignore::Error {
        ignore::Error::Io(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    }

    #[test]
    fn walk_error_is_flagged_and_the_readable_files_are_still_collected() {
        let root = scratch_tree("walkerr");
        let walk = vec![
            Ok(root.join("A.TcPOU")),
            Err(ignore::Error::WithPath {
                path: root.join("locked"),
                err: Box::new(denied()),
            }),
            Ok(root.join("skip.txt")),
            Err(denied()),
            Ok(root.join("B.TcGVL")),
        ];
        let mut targets = Targets::default();
        drain_walk(&root, walk, |_| true, &mut targets);
        assert!(targets.walk_error, "a walk error must be flagged");
        assert_eq!(
            file_names(&targets.files),
            ["A.TcPOU", "B.TcGVL"],
            "the entries around the errors must still be collected"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn walk_error_line_names_the_unreadable_path_once() {
        // The walker's error for an unreadable directory: its path, its depth,
        // and an I/O error whose own text repeats the path.
        let locked = PathBuf::from("root/locked");
        let wrapped = std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            Wrapped(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
        );
        let err = ignore::Error::WithPath {
            path: locked.clone(),
            err: Box::new(ignore::Error::WithDepth {
                depth: 1,
                err: Box::new(ignore::Error::Io(wrapped)),
            }),
        };
        assert_eq!(
            walk_error_line(Path::new("root"), &err),
            format!("{}: permission denied", locked.display())
        );
    }

    #[test]
    fn walk_error_line_falls_back_to_the_walk_root() {
        assert_eq!(
            walk_error_line(Path::new("root"), &denied()),
            "root: permission denied"
        );
    }

    /// Mimics `walkdir::Error`: its text repeats the path, its source is the
    /// plain I/O error.
    #[derive(Debug)]
    struct Wrapped(std::io::Error);

    impl std::fmt::Display for Wrapped {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "IO error for operation on root/locked: {}", self.0)
        }
    }

    impl std::error::Error for Wrapped {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    #[test]
    fn overlapping_arguments_yield_each_target_exactly_once() {
        // The same file named three ways: literally, via its directory, and via
        // a glob (whose walk yields a differently spelled path).
        let root = scratch_tree("overlap");
        let file = root.join("A.TcPOU").display().to_string();
        let dir = root.display().to_string();
        let pattern = format!("{}/A*.TcPOU", root.display().to_string().replace('\\', "/"));
        let targets = collect_targets(&[file, dir, pattern]);
        assert!(!targets.missing);
        assert_eq!(
            file_names(&targets.files),
            ["A.TcPOU", "B.TcGVL", "Deep.TcDUT", "lower.tcdut"],
            "A.TcPOU is named three times but must be collected once"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
