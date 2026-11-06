//! The `tctidy` binary: argv → the [`tctidy`] facade → IO. No formatting logic.
//!
//! Modes (set by `cli`): write-in-place (default), `--check` (no write; exit 1 if
//! anything would change), `--diff` (print a unified diff; no write). Writes go
//! through the atomic-write primitive, and nothing is ever written on error, the
//! non-destruction guarantee at the IO boundary.
#![forbid(unsafe_code)]

mod cli;
mod config;
mod diff;
mod exit;
mod io_atomic;
mod settings_server;
mod stdin;
mod walk;
mod xae_shell;

use std::io::Read;
use std::path::Path;
use std::process::ExitCode;
use std::rc::Rc;

use clap::Parser;
use io_atomic::WriteError;
use tctidy::{format_st, format_twincat_file, FormatOptions, StSection};

fn main() -> ExitCode {
    let args = cli::Cli::parse();

    if args.discover_xae_shells {
        return xae_shell::run();
    }
    if let Some(version) = args.clear_xae_cache.as_deref() {
        return xae_shell::clear_cache(version);
    }

    if let Some(dir) = args.settings.as_deref() {
        return settings_server::run(dir);
    }

    let Ok(mut configs) = config::ConfigResolver::new(args.config.as_deref()) else {
        return exit::config_failed();
    };
    if args.print_config {
        return match configs.for_stdin(args.stdin_filepath.as_deref().map(Path::new)) {
            Ok(opts) => {
                print!("{}", tctidy::config::render_config(&opts.style, true));
                exit::OK
            }
            Err(()) => exit::config_failed(),
        };
    }

    // The targets are the positional paths plus any read from `--files-from`. In
    // files-from mode, `-` names stdin as the *list* source, not ST input.
    let mut paths = args.paths.clone();
    if let Some(src) = args.files_from.as_deref() {
        match read_path_list(src) {
            Ok(extra) => paths.extend(extra),
            Err(e) => {
                eprintln!("tctidy: --files-from {src}: {e}");
                return ExitCode::from(2);
            }
        }
    }

    if paths.is_empty() {
        eprintln!("tctidy: no input paths (use `-` to read from stdin)");
        return ExitCode::from(2);
    }

    let section = if args.decl {
        StSection::Declaration
    } else if args.impl_ {
        StSection::Implementation
    } else {
        StSection::Auto
    };

    // The stdin-ST path is only the bare `tctidy -`; under `--files-from` the `-`
    // already meant "read the list from stdin" and has been consumed above.
    if args.files_from.is_none() && paths == ["-"] {
        return match configs.for_stdin(args.stdin_filepath.as_deref().map(Path::new)) {
            Ok(opts) => stdin::run(&opts, section, args.check, args.diff),
            Err(()) => exit::config_failed(),
        };
    }

    let walk::Targets {
        files,
        missing,
        walk_error,
        invalid_glob,
    } = walk::collect_targets(&paths);
    // Discovery problems never stop the run: every file found is still
    // processed. A literal path that does not exist, or a directory or glob
    // walk that could not read part of its tree, is a failure (exit 3), not a
    // silent no-op; an invalid glob is a usage error (exit 2). A file covered
    // by an invalid configuration is skipped (left untouched).
    let (mut any_error, mut any_changed, mut any_config_error) =
        (missing || walk_error, false, false);
    for file in files {
        let opts: Rc<FormatOptions> = match configs.for_file(&file) {
            Ok(opts) => opts,
            Err(()) => {
                any_config_error = true;
                continue;
            }
        };
        match process_file(&file, &opts, args.check, args.diff, section) {
            Ok(changed) => any_changed |= changed,
            Err(()) => any_error = true,
        }
    }

    // The most severe status wins (see `exit`): usage 2, failure 3, check 1.
    if invalid_glob {
        exit::usage_failed()
    } else if any_config_error {
        exit::config_failed()
    } else if any_error {
        exit::format_failed()
    } else if args.check && any_changed {
        exit::check_failed()
    } else {
        exit::OK
    }
}

/// Read a `--files-from` list: one path per line from `src` (a file, or stdin when
/// `src` is `-`). Blank lines and lines starting with `#` are skipped, so a list can
/// carry comments. Lets a caller hand over thousands of paths without hitting the OS
/// command-line length limit.
fn read_path_list(src: &str) -> std::io::Result<Vec<String>> {
    let raw = if src == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s
    } else {
        std::fs::read_to_string(src)?
    };
    Ok(raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect())
}

/// Format one file. Returns whether it changed, or `Err` after reporting why: it
/// could not be read, formatted or written (and is left untouched), or it was
/// written but the write may not survive a power loss.
fn process_file(
    path: &Path,
    opts: &FormatOptions,
    check: bool,
    diff_mode: bool,
    section: StSection,
) -> Result<bool, ()> {
    let bytes = std::fs::read(path).map_err(|e| eprintln!("tctidy: {}: {e}", path.display()))?;

    let (orig, formatted, changed) = if is_twincat(path, &bytes) {
        let out = format_twincat_file(&bytes, opts)
            .map_err(|e| eprintln!("tctidy: {}: {e}", path.display()))?;
        let orig = String::from_utf8_lossy(&bytes).into_owned();
        (orig, out.bytes, out.changed)
    } else {
        let src = String::from_utf8(bytes)
            .map_err(|e| eprintln!("tctidy: {}: not valid UTF-8: {e}", path.display()))?;
        let out = format_st(&src, section, opts)
            .map_err(|e| eprintln!("tctidy: {}: {e}", path.display()))?;
        let changed = out != src;
        (src, out.into_bytes(), changed)
    };

    if diff_mode {
        if changed {
            print!(
                "{}",
                diff::unified(
                    &path.display().to_string(),
                    &orig,
                    &String::from_utf8_lossy(&formatted),
                )
            );
        }
    } else if check {
        if changed {
            eprintln!("tctidy: would reformat {}", path.display());
        }
    } else if changed {
        match io_atomic::write_atomic(path, &formatted) {
            Ok(()) => eprintln!("tctidy: formatted {}", path.display()),
            Err(WriteError::NotWritten(e)) => {
                eprintln!("tctidy: {}: failed to write: {e}", path.display());
                return Err(());
            }
            // The new content is in place, but a power loss could still undo
            // it: say both, and fail the run rather than report a success.
            Err(e @ WriteError::NotDurable { .. }) => {
                eprintln!(
                    "tctidy: formatted {}, but the change may not survive a power loss: {e}",
                    path.display()
                );
                return Err(());
            }
        }
    }

    Ok(changed)
}

/// A TwinCAT XML object file: by extension, or by content (starts with `<`).
fn is_twincat(path: &Path, bytes: &[u8]) -> bool {
    if walk::is_twincat_path(path) {
        return true;
    }
    // Skip a UTF-8 BOM, then look for the opening angle bracket of the XML.
    let head = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    head.iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|&b| b == b'<')
}
