//! Canonicalization invariant: a file and a scrambled copy of it format to the
//! **same** output, `format(scramble(x)) == format(x)`.
//!
//! The scramble (`xtask/src/scramble.rs`, shared with `cargo xtask scramble`)
//! perturbs only the *formatting* of the ST (horizontal whitespace, indentation,
//! keyword case) while preserving meaning and line-break counts. A correct
//! formatter therefore maps both versions to one canonical form: formatting must
//! not depend on the input's incidental whitespace. The scrambled copies are made
//! in memory, seeded by each file's path, so every run checks the same inputs.
//!
//! This is reference-independent: it stays a hard invariant whatever house-style
//! choices evolve, and any mismatch is a real bug. Runs at the default style and
//! at the non-default profiles of `support::styles` (without the `preserve`
//! keyword cases, which keep the scrambled case); skips when there is no corpus.

mod support;

use support::scramble::scramble_document;
use tctidy::format_twincat_file;

#[test]
fn scrambled_and_original_canonicalize_identically() {
    let Some(root) = support::corpus_dir("canonicalization") else {
        return;
    };
    let files = support::twincat_files(&root);
    let mut report = support::Report::new("canonicalization");
    for style in support::styles(true) {
        for (path, key) in &files {
            let bytes = std::fs::read(path).expect("read a corpus file");
            let Some(from_original) = report.format(&style, key, &bytes) else {
                continue;
            };
            let messy = match scramble_document(&bytes, key) {
                Ok((messy, _)) => messy,
                Err(error) => {
                    report.fail(&style, key, &format!("could not scramble: {error}"));
                    continue;
                }
            };
            match format_twincat_file(&messy, &style.options) {
                Ok(from_messy) if from_messy.bytes == from_original.bytes => report.pass(),
                Ok(_) => report.fail(&style, key, "formats differently once scrambled"),
                Err(error) => report.fail(&style, key, &format!("refused once scrambled: {error}")),
            }
        }
    }
    report.finish();
}
