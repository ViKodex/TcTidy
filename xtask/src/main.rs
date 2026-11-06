//! TcTidy developer tasks.
//!
//! The corpus tools read the corpus directory (see [`corpus::dir`]): `TCTIDY_CORPUS`
//! when it is set, else the public corpus that `corpus fetch` checks out.
//!
//! `cargo run -p xtask -- scramble [SRC] [DST]` mirrors every TwinCAT file under
//! `SRC` (default: the corpus) into `DST` (default `target/public-corpus-messy`),
//! rewriting only the Structured Text inside each file's CDATA into a
//! messy-but-equivalent form (see [`scramble`]). The XML around the ST is preserved
//! byte-for-byte by [`tctidy_twincat::reformat_document`], so a correct formatter
//! formats each `DST` file exactly like its `SRC` counterpart.
#![forbid(unsafe_code)]

mod corpus;
#[path = "../../crates/tctidy-syntax/keyword_codegen.rs"]
mod keyword_codegen;
mod perf;
mod release;
mod scramble;
mod tooling;
mod version_projection;
mod version_sources;
mod vsix_version;

use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("licenses" | "vsix") => {
            if let Err(error) = tooling::run(&args[1..]) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("corpus") => {
            if let Err(error) = corpus::run(&workspace_root(), &args[2..]) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("scramble") => {
            let src = args
                .get(2)
                .map_or_else(|| corpus::dir(&workspace_root()), PathBuf::from);
            let dst = args.get(3).map_or_else(
                || workspace_root().join("target/public-corpus-messy"),
                PathBuf::from,
            );
            run_scramble(&src, &dst);
        }
        Some("parsecheck") => {
            let dir = args
                .get(2)
                .map_or_else(|| corpus::dir(&workspace_root()), PathBuf::from);
            run_parsecheck(&dir);
        }
        Some("goldencheck") => {
            let dir = args
                .get(2)
                .map_or_else(|| corpus::dir(&workspace_root()), PathBuf::from);
            let filter = args.get(3).cloned();
            run_goldencheck(&dir, filter.as_deref());
        }
        Some("perf") => match args.get(2).map(String::as_str) {
            Some("cold") => perf::run_cold(),
            _ => {
                eprintln!("usage: xtask perf cold");
                std::process::exit(2);
            }
        },
        Some("keywords") => match args.get(2).map(String::as_str) {
            None => run_keywords(false),
            Some("--check") => run_keywords(true),
            _ => {
                eprintln!("usage: xtask keywords [--check]");
                std::process::exit(2);
            }
        },
        Some("vsix-version") => {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("xtask lives under the workspace root");
            if let Err(error) = vsix_version::run(root, &args[2..]) {
                eprintln!("{error}");
                std::process::exit(2);
            }
        }
        Some("release") => {
            if let Err(error) = release::run(&args[2..]) {
                eprintln!("release error: {error}");
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!(
                "usage: xtask <corpus fetch | scramble [SRC] [DST] | parsecheck [DIR] | \
                 goldencheck [DIR] [FILTER] | perf cold | keywords [--check] | \
                 vsix-version <--workspace | --tag TAG> | release <COMMAND> | \
                 licenses <COMMAND> | vsix check <OPTIONS>>"
            );
            std::process::exit(2);
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives under the workspace root")
        .to_path_buf()
}

/// A file's path relative to `root`, with `/` separators on every platform, so it
/// seeds the scramble identically everywhere (the corpus tests use the same key).
fn relative_key(root: &Path, file: &Path) -> String {
    let rel = file.strip_prefix(root).unwrap_or(file);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn run_keywords(check: bool) {
    let root = workspace_root();
    let syntax = root.join("crates/tctidy-syntax");
    let csv = std::fs::read_to_string(syntax.join("keywords.csv")).expect("read keywords.csv");
    let kinds = std::fs::read_to_string(syntax.join("src/kind.rs")).expect("read SyntaxKind");
    let generated = keyword_codegen::generate(&csv, &kinds)
        .unwrap_or_else(|error| panic!("invalid keyword inventory: {error}"));
    let destination = syntax.join("src/kind/keywords_generated.rs");

    if check {
        let current = std::fs::read_to_string(&destination).unwrap_or_default();
        if !same_ignoring_crlf(&current, &generated) {
            eprintln!(
                "{} is stale; run `cargo run -p xtask -- keywords`",
                destination.display()
            );
            std::process::exit(1);
        }
    } else {
        std::fs::write(&destination, generated).expect("write generated keyword table");
        println!("updated {}", destination.display());
    }
}

/// Equal once CRLF line endings are read as LF: Git on Windows (`core.autocrlf`)
/// checks the generated file out with CRLF, and that alone is not staleness.
fn same_ignoring_crlf(current: &str, generated: &str) -> bool {
    current.replace("\r\n", "\n") == generated.replace("\r\n", "\n")
}

/// Report how many ST sections in the corpus parse error-free and round-trip.
fn run_parsecheck(dir: &Path) {
    use tctidy_twincat::StSectionKind;

    let mut files = Vec::new();
    collect_twincat_files(dir, &mut files);
    files.sort();

    let (mut decl_ok, mut decl_err, mut impl_total) = (0u32, 0u32, 0u32);
    let (mut impl_ok, mut impl_err) = (0u32, 0u32);
    let mut files_with_decl_err = 0u32;
    let mut examples: Vec<String> = Vec::new();
    let mut impl_examples: Vec<String> = Vec::new();

    for file in &files {
        let rel = file.strip_prefix(dir).unwrap_or(file);
        let Ok(raw) = std::fs::read(file) else {
            continue;
        };
        let (bytes, _enc) = tctidy_twincat::Encoding::detect_and_strip_bom(&raw);
        let Ok(spans) = tctidy_twincat::locate_st_spans(bytes) else {
            continue;
        };
        let mut file_has_err = false;
        for span in &spans {
            let Ok(st) = std::str::from_utf8(&bytes[span.content.clone()]) else {
                continue;
            };
            match span.kind {
                StSectionKind::Declaration => {
                    let parse = tctidy_syntax::parse_declaration(st);
                    let lossless = parse.debug_text_matches(st);
                    if parse.errors().is_empty() && lossless {
                        decl_ok += 1;
                    } else {
                        decl_err += 1;
                        file_has_err = true;
                        if examples.len() < 15 {
                            let why = if !lossless {
                                "NOT LOSSLESS".to_string()
                            } else {
                                parse
                                    .errors()
                                    .first()
                                    .map(|e| e.message.clone())
                                    .unwrap_or_default()
                            };
                            examples.push(format!("  {} [{}]", rel.display(), why));
                        }
                    }
                }
                StSectionKind::Implementation => {
                    impl_total += 1;
                    let parse = tctidy_syntax::parse_implementation(st);
                    let lossless = parse.debug_text_matches(st);
                    if parse.errors().is_empty() && lossless {
                        impl_ok += 1;
                    } else {
                        impl_err += 1;
                        if impl_examples.len() < 15 {
                            let (why, ctx) = if !lossless {
                                ("NOT LOSSLESS".to_string(), String::new())
                            } else {
                                let e = parse.errors().first();
                                let msg = e.map(|e| e.message.clone()).unwrap_or_default();
                                let at = e.map_or(0, |e| usize::from(e.range.start()));
                                let lo = at.saturating_sub(40).min(st.len());
                                let hi = (at + 20).min(st.len());
                                (msg, st[lo..hi].replace(['\r', '\n'], " "))
                            };
                            impl_examples.push(format!("  {} [{}]  …{ctx}…", rel.display(), why));
                        }
                    }
                }
            }
        }
        if file_has_err {
            files_with_decl_err += 1;
        }
    }

    println!("parsecheck over {}:", dir.display());
    println!("  declaration sections: {decl_ok} clean, {decl_err} with errors");
    println!(
        "  implementation sections: {impl_ok} clean, {impl_err} with errors (of {impl_total})"
    );
    println!("  files with a declaration error: {files_with_decl_err}");
    if !examples.is_empty() {
        println!("  first declaration failures:");
        for e in &examples {
            println!("{e}");
        }
    }
    if !impl_examples.is_empty() {
        println!("  first implementation failures:");
        for e in &impl_examples {
            println!("{e}");
        }
    }
}

/// **Informational gauge** of how close the formatter's *default* output stays to
/// the corpus as its authors wrote it: each file is scrambled in memory (see
/// [`scramble`]) and every formatted ST section is compared with the original one,
/// grouped by file extension.
///
/// This is NOT a correctness gate: the corpus follows its authors' style, not
/// TcTidy's (e.g. `//x` -> `// x`, tabs -> spaces), so a number below 100 % is
/// expected. Its value is spotting *unintended* drift. The hard correctness
/// guarantees live in the test suite: canonicalization
/// (`format(scramble(x)) == format(x)`), idempotence, semantic and comment
/// preservation.
fn run_goldencheck(dir: &Path, diff_ext: Option<&str>) {
    use tctidy::{format_st, FormatOptions, StSection};
    use tctidy_twincat::StSectionKind;

    let mut files = Vec::new();
    collect_twincat_files(dir, &mut files);
    files.sort();
    // Default style, measure the honest distance from the source as written,
    // intentional divergences included.
    let opts = FormatOptions::default();

    // Per-extension tallies: (decl sections, exact, mismatch, parse_err, fmt_err).
    use std::collections::BTreeMap;
    let mut by_ext: BTreeMap<String, [u32; 5]> = BTreeMap::new();
    let mut shown = 0u32;
    let mut trailing_ws_only = 0u32;

    for file in &files {
        let rel = file.strip_prefix(dir).unwrap_or(file);
        let ext = rel
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("?")
            .to_string();
        let Ok(vraw) = std::fs::read(file) else {
            continue;
        };
        let Ok((mraw, _)) = scramble::scramble_document(&vraw, &relative_key(dir, file)) else {
            continue;
        };
        let (mb, _) = tctidy_twincat::Encoding::detect_and_strip_bom(&mraw);
        let (vb, _) = tctidy_twincat::Encoding::detect_and_strip_bom(&vraw);
        let (Ok(mspans), Ok(vspans)) = (
            tctidy_twincat::locate_st_spans(mb),
            tctidy_twincat::locate_st_spans(vb),
        ) else {
            continue;
        };
        if mspans.len() != vspans.len() {
            continue;
        }

        for (ms, vs) in mspans.iter().zip(&vspans) {
            let (Ok(mst), Ok(vst)) = (
                std::str::from_utf8(&mb[ms.content.clone()]),
                std::str::from_utf8(&vb[vs.content.clone()]),
            ) else {
                continue;
            };
            let (section, decl) = match ms.kind {
                StSectionKind::Declaration => (StSection::Declaration, true),
                StSectionKind::Implementation => (StSection::Implementation, false),
            };
            let key = format!("{ext}:{}", if decl { "decl" } else { "impl" });
            let tally = by_ext.entry(key).or_default();
            tally[0] += 1;

            // Skip sections the parser can't yet handle cleanly (counted as
            // parse_err so the score reflects only what the tool claims to format).
            let parse_err = if decl {
                !tctidy_syntax::parse_declaration(mst).errors().is_empty()
            } else {
                !tctidy_syntax::parse_implementation(mst).errors().is_empty()
            };
            if parse_err {
                tally[3] += 1;
                continue;
            }
            match format_st(mst, section, &opts) {
                Ok(out) if out == vst => tally[1] += 1,
                Ok(out) => {
                    tally[2] += 1;
                    let tws = trimmed_each_line(&out) == trimmed_each_line(vst);
                    if tws {
                        trailing_ws_only += 1;
                    }
                    // Full side-by-side dump when the filter is a path substring.
                    if let Some(sub) = diff_ext.filter(|x| x.contains('/')) {
                        if rel.to_string_lossy().contains(sub) && shown == 0 {
                            shown += 1;
                            eprintln!("=== FULL DUMP {} ===", rel.display());
                            let ev: Vec<&str> = vst.lines().collect();
                            let av: Vec<&str> = out.lines().collect();
                            for k in 0..ev.len().max(av.len()) {
                                let e = ev.get(k).copied().unwrap_or("∅");
                                let a = av.get(k).copied().unwrap_or("∅");
                                let mark = if e == a { " " } else { ">" };
                                eprintln!("{mark}{:3} E|{e}", k + 1);
                                if e != a {
                                    eprintln!("    A|{a}");
                                }
                            }
                        }
                    } else if shown < 12 && diff_ext.is_none_or(|x| x == ext) {
                        shown += 1;
                        print_first_diff(rel, vst, &out);
                    }
                }
                Err(_) => tally[4] += 1,
            }
        }
    }

    println!("\ngoldencheck, informational: closeness of default output to the corpus as written");
    println!(
        "(the corpus follows its authors' style; correctness lives in the test suite, not here)"
    );
    println!(
        "  {:<12} {:>6} {:>6} {:>6} {:>8} {:>7}",
        "ext:kind", "total", "exact", "diff", "parseEr", "fmtEr"
    );
    let mut tot = [0u32; 5];
    for (ext, t) in &by_ext {
        println!(
            "  {:<12} {:>6} {:>6} {:>6} {:>8} {:>7}",
            ext, t[0], t[1], t[2], t[3], t[4]
        );
        for i in 0..5 {
            tot[i] += t[i];
        }
    }
    println!(
        "  {:<12} {:>6} {:>6} {:>6} {:>8} {:>7}",
        "ALL", tot[0], tot[1], tot[2], tot[3], tot[4]
    );
    println!(
        "  of the {} diffs, {trailing_ws_only} differ only by trailing whitespace",
        tot[2]
    );
    let denom = tot[1] + tot[2]; // sections actually attempted
    if denom > 0 {
        println!(
            "  exact among attempted (parse-clean) sections: {}/{} = {:.1}%",
            tot[1],
            denom,
            100.0 * tot[1] as f64 / denom as f64
        );
    }
}

/// Each line with trailing spaces/tabs removed, rejoined, for classifying
/// "differs only by trailing whitespace" mismatches.
fn trimmed_each_line(s: &str) -> String {
    s.lines()
        .map(|l| l.trim_end_matches([' ', '\t']))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Print the first differing line between expected and actual, with context.
fn print_first_diff(rel: &Path, expected: &str, actual: &str) {
    let e: Vec<&str> = expected.lines().collect();
    let a: Vec<&str> = actual.lines().collect();
    let n = e.len().max(a.len());
    for i in 0..n {
        let el = e.get(i).copied().unwrap_or("<none>");
        let al = a.get(i).copied().unwrap_or("<none>");
        if el != al {
            eprintln!("--- diff {} @ line {} ---", rel.display(), i + 1);
            eprintln!("  expected: {el:?}");
            eprintln!("  actual:   {al:?}");
            return;
        }
    }
}

fn run_scramble(src: &Path, dst: &Path) {
    let mut files = Vec::new();
    collect_twincat_files(src, &mut files);
    files.sort();

    let (mut changed, mut unchanged, mut fallback, mut safety) = (0u32, 0u32, 0u32, 0u32);

    for file in &files {
        let rel = file.strip_prefix(src).unwrap_or(file);
        let bytes = match std::fs::read(file) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("skip {}: {e}", file.display());
                continue;
            }
        };

        let out_bytes = match scramble::scramble_document(&bytes, &relative_key(src, file)) {
            Ok((b, kept)) => {
                if b == bytes {
                    unchanged += 1;
                } else {
                    changed += 1;
                }
                safety += kept;
                b
            }
            Err(_) => {
                fallback += 1;
                bytes.clone()
            }
        };

        let target = dst.join(rel);
        if let Some(parent) = target.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("mkdir {}: {e}", parent.display());
                continue;
            }
        }
        if let Err(e) = std::fs::write(&target, &out_bytes) {
            eprintln!("write {}: {e}", target.display());
        }
    }

    println!(
        "scrambled {} files into {}: {changed} changed, {unchanged} unchanged, \
         {fallback} fallback-copied (XML unparsable), {safety} sections kept (safety net)",
        files.len(),
        dst.display(),
    );
}

/// The TwinCAT object files (`.TcPOU`, `.TcGVL`, `.TcDUT`, `.TcIO`) under `dir`,
/// skipping `.git` directories.
fn collect_twincat_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != ".git") {
                collect_twincat_files(&path, out);
            }
        } else if path.extension().is_some_and(|e| {
            ["tcpou", "tcgvl", "tcdut", "tcio"]
                .contains(&e.to_string_lossy().to_ascii_lowercase().as_str())
        }) {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::same_ignoring_crlf;

    #[test]
    fn keyword_check_ignores_crlf_line_endings() {
        assert!(same_ignoring_crlf("a\r\nb\r\n", "a\nb\n"));
        assert!(!same_ignoring_crlf("a\r\nc\r\n", "a\nb\n"));
    }
}
