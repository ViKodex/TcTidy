//! The `tctidy -` path: read from stdin, format, and emit per the mode.
//!
//! Auto-detects TwinCAT XML (a leading `<`) and formats it like a file, locating
//! and reformatting the ST inside while preserving every byte outside it. Otherwise
//! the input is raw ST, formatted as `section` (declaration or implementation).

use std::io::{Read, Write};
use std::process::ExitCode;

use tctidy::{format_st, format_twincat_file, FormatOptions, StSection};

use crate::{diff, exit};

/// Read stdin and format it, dispatching on content: TwinCAT XML vs raw ST.
/// * `--diff`  → print a unified diff to stdout (no write);
/// * `--check` → write nothing, exit non-zero if it would change;
/// * otherwise → write the formatted output to stdout.
pub fn run(opts: &FormatOptions, section: StSection, check: bool, diff_mode: bool) -> ExitCode {
    let mut bytes = Vec::new();
    if let Err(e) = std::io::stdin().read_to_end(&mut bytes) {
        eprintln!("tctidy: failed to read stdin: {e}");
        return exit::format_failed();
    }

    if is_twincat_xml(&bytes) {
        run_xml(&bytes, opts, check, diff_mode)
    } else {
        run_st(bytes, opts, section, check, diff_mode)
    }
}

/// A TwinCAT XML document: the first non-whitespace byte (after an optional UTF-8
/// BOM) is `<`. Mirrors the content check the file path uses in `main`.
fn is_twincat_xml(bytes: &[u8]) -> bool {
    let head = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    head.iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|&b| b == b'<')
}

/// Format a whole TwinCAT XML document, preserving every byte outside the ST.
fn run_xml(bytes: &[u8], opts: &FormatOptions, check: bool, diff_mode: bool) -> ExitCode {
    let out = match format_twincat_file(bytes, opts) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("tctidy: {e}");
            return exit::format_failed();
        }
    };

    if diff_mode {
        if out.changed {
            print!(
                "{}",
                diff::unified(
                    "<stdin>",
                    &String::from_utf8_lossy(bytes),
                    &String::from_utf8_lossy(&out.bytes),
                )
            );
        }
    } else if !check {
        if let Err(e) = std::io::stdout().write_all(&out.bytes) {
            eprintln!("tctidy: failed to write stdout: {e}");
            return exit::format_failed();
        }
    }

    if check && out.changed {
        exit::check_failed()
    } else {
        exit::OK
    }
}

/// Format raw ST (the original stdin behaviour), as `section`.
fn run_st(
    bytes: Vec<u8>,
    opts: &FormatOptions,
    section: StSection,
    check: bool,
    diff_mode: bool,
) -> ExitCode {
    let src = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("tctidy: not valid UTF-8: {e}");
            return exit::format_failed();
        }
    };

    let out = match format_st(&src, section, opts) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("tctidy: {e}");
            return exit::format_failed();
        }
    };

    let changed = out != src;
    if diff_mode {
        if changed {
            print!("{}", diff::unified("<stdin>", &src, &out));
        }
    } else if !check {
        if let Err(e) = std::io::stdout().write_all(out.as_bytes()) {
            eprintln!("tctidy: failed to write stdout: {e}");
            return exit::format_failed();
        }
    }

    if check && changed {
        exit::check_failed()
    } else {
        exit::OK
    }
}
