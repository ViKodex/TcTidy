//! Process exit codes.
//!
//! * `0` success (formatted, or already-formatted under `--check`).
//! * `1` `--check` found a file that would change (CI signal).
//! * `2` usage error (handled by clap, or an invalid glob pattern), or an
//!   invalid configuration file.
//! * `3` at least one file failed: it could not be read, parsed, formatted or
//!   written and was left untouched, or it was formatted but the write could
//!   not be made durable (stderr says which); or a literal path does not
//!   exist, or a directory or glob walk could not read part of its tree.
//!
//! A problem with one target (an invalid glob, a missing path, a walk error,
//! an invalid configuration, a parse error) does not stop a batch run: every
//! other target is still processed, and the run exits with the most severe
//! status seen, `2` over `3` over `1`. A valid glob matching nothing is not a
//! problem.

use std::process::ExitCode;

/// Success.
pub const OK: ExitCode = ExitCode::SUCCESS;

/// `--check` found an unformatted file.
pub fn check_failed() -> ExitCode {
    ExitCode::from(1)
}

/// A path argument is not valid glob syntax (the other targets were still
/// processed).
pub fn usage_failed() -> ExitCode {
    ExitCode::from(2)
}

/// A configuration file is missing or invalid (nothing was formatted with it).
pub fn config_failed() -> ExitCode {
    ExitCode::from(2)
}

/// A file could not be parsed, formatted or written (left untouched), its
/// write could not be made durable, a literal path does not exist, or part of
/// a directory tree could not be read.
pub fn format_failed() -> ExitCode {
    ExitCode::from(3)
}
