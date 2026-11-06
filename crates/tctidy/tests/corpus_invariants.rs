//! Corpus invariant: formatting real-world files is **idempotent**,
//! `format(format(x)) == format(x)`. The first pass may change layout; the second
//! must change nothing, and must accept its own output. A violation means the
//! formatter has no fixpoint, which would make it unsafe to run on save.
//!
//! Every file of the corpus must also format at all: a refusal fails the test
//! unless the file is a known failure (see `support`). Runs at the default style
//! and at the non-default profiles of `support::styles`; skips when there is no
//! corpus.

mod support;

use tctidy::format_twincat_file;
use tctidy_format::options::style_options;

/// Runs without a corpus: both non-default profiles move every configurable
/// setting off its default, so no setting escapes the corpus invariants.
#[test]
fn non_default_profiles_cover_every_setting() {
    for canonical in [false, true] {
        for style in support::styles(canonical).iter().skip(1) {
            for option in style_options() {
                assert_ne!(
                    option.get(&style.options.style),
                    option.default_value(),
                    "`{}` keeps its default in the {} profile",
                    option.key,
                    style.name
                );
            }
        }
    }
}

#[test]
fn formatting_is_idempotent_over_the_corpus() {
    let Some(root) = support::corpus_dir("idempotence") else {
        return;
    };
    let files = support::twincat_files(&root);
    let mut report = support::Report::new("idempotence");
    for style in support::styles(false) {
        for (path, key) in &files {
            let bytes = std::fs::read(path).expect("read a corpus file");
            let Some(once) = report.format(&style, key, &bytes) else {
                continue;
            };
            match format_twincat_file(&once.bytes, &style.options) {
                Ok(twice) if twice.bytes == once.bytes => report.pass(),
                Ok(_) => report.fail(&style, key, "the second format pass changed bytes"),
                Err(error) => {
                    report.fail(&style, key, &format!("formatted output refused: {error}"))
                }
            }
        }
    }
    report.finish();
}
