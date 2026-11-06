//! Pipeline performance benchmarks.
//!
//! Each formatting stage is timed in isolation as far as the public API allows,
//! plus the two end-to-end paths. The stage map (see `BENCHMARKS.md`):
//!
//! | group              | times                                   | stage(s)                    |
//! |--------------------|-----------------------------------------|-----------------------------|
//! | `lex`              | `tokens::lex`                           | lexer                       |
//! | `parse`            | `parse_declaration` / `_implementation` | lex + parse                 |
//! | `format`           | `format_declaration` / `_implementation`| attach + lower + print      |
//! | `format_st`        | `format_st`                             | end-to-end raw ST           |
//! | `xml_locate`       | `locate_st_spans`                       | CDATA discovery             |
//! | `xml_reformat`     | `reformat_document` (identity vs real)  | splice (real − identity)    |
//! | `format_file`      | `format_twincat_file`                   | end-to-end TwinCAT file     |
//!
//! `attach`, `lower`, and `print` are not separately reachable through the public
//! API (the trivia module is private and a `Doc`-returning seam is intentionally
//! deferred); they are measured together as the `format` group. Subtracted figures
//! (`parse − lex`, `format − parse`, `real − identity`) are derived point
//! estimates, not criterion confidence intervals.
//!
//! Inputs: committed synthetic ST fixtures under `inputs/` (a CI tripwire, *not*
//! optimization targets), plus representative real documents from the corpus when
//! there is one (the surface to actually optimize against): `TCTIDY_CORPUS`
//! (absolute, or relative to the repository root) when it is set, else the public
//! corpus that `cargo xtask corpus fetch` checks out under `target/public-corpus`.

use std::error::Error;
use std::hint::black_box;
use std::path::Path;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use tctidy::{format_st, format_twincat_file, FormatOptions, StSection};
use tctidy_format::{format_declaration, format_implementation, StyleConfig};
use tctidy_pretty::LineEnding;
use tctidy_syntax::tokens::lex;
use tctidy_syntax::{parse_declaration, parse_implementation};
use tctidy_twincat::{locate_st_spans, reformat_document, Encoding, StContext, StSectionKind};

// --- inputs ------------------------------------------------------------------

const DECL_BLOCK: &str = include_str!("inputs/decl_block.st");
const IMPL_BODY: &str = include_str!("inputs/impl_body.st");
const IMPL_COMMENTS: &str = include_str!("inputs/impl_comments.st");

/// A raw-ST input and which grammar it belongs to.
struct StInput {
    name: String,
    src: String,
    section: StSection,
}

/// A whole-TwinCAT-file input.
struct FileInput {
    name: String,
    bytes: Vec<u8>,
}

/// Wrap a declaration + implementation body in a minimal `.TcPOU` envelope with
/// the given end-of-line, mirroring the real container the locator expects.
fn wrap_pou(decl: &str, body: &str, eol: &str) -> String {
    let d = decl.trim_end().replace('\n', eol);
    let i = body.trim_end().replace('\n', eol);
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>{eol}\
<TcPlcObject Version=\"1.1.0.1\" ProductVersion=\"3.1.4024.0\">{eol}\
  <POU Name=\"FB_Conveyor\" Id=\"{{0a0a0a0a-0000-0000-0000-000000000000}}\" SpecialFunc=\"None\">{eol}\
    <Declaration><![CDATA[{d}]]></Declaration>{eol}\
    <Implementation>{eol}\
      <ST><![CDATA[{i}]]></ST>{eol}\
    </Implementation>{eol}\
  </POU>{eol}\
</TcPlcObject>{eol}"
    )
}

/// The committed synthetic raw-ST inputs. Each must parse cleanly, otherwise the
/// `format`/`format_st` groups would time the error-return path, so this asserts it.
fn synthetic_st_inputs() -> Vec<StInput> {
    let inputs = vec![
        StInput {
            name: "decl_block".into(),
            src: DECL_BLOCK.into(),
            section: StSection::Declaration,
        },
        StInput {
            name: "impl_body".into(),
            src: IMPL_BODY.into(),
            section: StSection::Implementation,
        },
        StInput {
            name: "impl_comments".into(),
            src: IMPL_COMMENTS.into(),
            section: StSection::Implementation,
        },
    ];
    for inp in &inputs {
        let clean = match inp.section {
            StSection::Declaration => parse_declaration(&inp.src).errors().is_empty(),
            _ => parse_implementation(&inp.src).errors().is_empty(),
        };
        assert!(
            clean,
            "synthetic fixture `{}` does not parse cleanly",
            inp.name
        );
    }
    inputs
}

/// The committed synthetic whole-file inputs: LF, CRLF, and a UTF-8 BOM variant.
fn synthetic_file_inputs() -> Vec<FileInput> {
    let lf = wrap_pou(DECL_BLOCK, IMPL_BODY, "\n");
    let crlf = wrap_pou(DECL_BLOCK, IMPL_BODY, "\r\n");
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(lf.as_bytes());
    vec![
        FileInput {
            name: "pou_lf".into(),
            bytes: lf.into_bytes(),
        },
        FileInput {
            name: "pou_crlf".into(),
            bytes: crlf.into_bytes(),
        },
        FileInput {
            name: "pou_bom".into(),
            bytes: bom,
        },
    ]
}

/// Representative real files from the corpus, if there is one: the smallest,
/// median, and largest `.Tc*` documents by size (bounded so the run stays fast).
/// Also yields a few real ST spans (from the largest file) for the ST-level
/// groups. Real files are the surface to optimize against; the synthetic panel is
/// only a CI tripwire.
fn corpus_inputs() -> (Vec<FileInput>, Vec<StInput>) {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = match std::env::var_os("TCTIDY_CORPUS") {
        Some(value) if !value.is_empty() => repo.join(value),
        _ => repo.join("target/public-corpus"),
    };
    if !root.is_dir() {
        return (Vec::new(), Vec::new());
    }
    let dir = root.display();
    let mut files = Vec::new();
    collect_twincat_files(&root, &mut files);
    let mut sized: Vec<(usize, std::path::PathBuf)> = files
        .into_iter()
        .filter_map(|p| std::fs::metadata(&p).ok().map(|m| (m.len() as usize, p)))
        .filter(|(len, _)| *len > 0)
        .collect();
    if sized.is_empty() {
        eprintln!("the corpus {dir} held no readable .Tc* files");
        return (Vec::new(), Vec::new());
    }
    sized.sort_by_key(|(len, _)| *len);
    let picks = [0, sized.len() / 2, sized.len() - 1];
    let labels = ["corpus_small", "corpus_median", "corpus_large"];

    let mut file_inputs = Vec::new();
    let mut st_inputs = Vec::new();
    for (i, &idx) in picks.iter().enumerate() {
        let (_, path) = &sized[idx];
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        // Pull real ST spans from the largest file for the ST-level groups.
        if labels[i] == "corpus_large" {
            st_inputs.extend(extract_clean_st(&bytes));
        }
        file_inputs.push(FileInput {
            name: labels[i].into(),
            bytes,
        });
    }
    (file_inputs, st_inputs)
}

/// Up to one declaration and one implementation span from a real file, included
/// only if they parse cleanly (so the `format` group stays on the happy path).
fn extract_clean_st(bytes: &[u8]) -> Vec<StInput> {
    let (body, _enc) = Encoding::detect_and_strip_bom(bytes);
    let Ok(spans) = locate_st_spans(body) else {
        return Vec::new();
    };
    let mut out: Vec<StInput> = Vec::new();
    for span in &spans {
        let Ok(st) = std::str::from_utf8(&body[span.content.clone()]) else {
            continue;
        };
        match span.kind {
            StSectionKind::Declaration
                if !out.iter().any(|s| s.section == StSection::Declaration)
                    && parse_declaration(st).errors().is_empty() =>
            {
                out.push(StInput {
                    name: "corpus_large/decl".into(),
                    src: st.to_string(),
                    section: StSection::Declaration,
                });
            }
            StSectionKind::Implementation
                if !out.iter().any(|s| s.section == StSection::Implementation)
                    && parse_implementation(st).errors().is_empty() =>
            {
                out.push(StInput {
                    name: "corpus_large/impl".into(),
                    src: st.to_string(),
                    section: StSection::Implementation,
                });
            }
            _ => {}
        }
    }
    out
}

fn collect_twincat_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_twincat_files(&path, out);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("TcPOU" | "TcGVL" | "TcDUT" | "TcIO")
        ) {
            out.push(path);
        }
    }
}

// --- callbacks for the XML splice groups -------------------------------------

type CbErr = Box<dyn Error + Send + Sync>;

/// The real formatting callback: the cost `reformat_document` adds on top of the
/// identity callback is the parse+format of every span.
fn real_format(opts: &FormatOptions) -> impl FnMut(&str, StContext) -> Result<String, CbErr> + '_ {
    move |st, cx| {
        let section = match cx.kind {
            StSectionKind::Declaration => StSection::Declaration,
            StSectionKind::Implementation => StSection::Implementation,
        };
        format_st(st, section, opts).map_err(|e| -> CbErr { e.to_string().into() })
    }
}

fn identity(st: &str, _cx: StContext) -> Result<String, CbErr> {
    Ok(st.to_string())
}

// --- the benchmark -----------------------------------------------------------

fn pipeline(c: &mut Criterion) {
    let style = StyleConfig::default();
    let opts = FormatOptions::default();

    let (corpus_files, corpus_st) = corpus_inputs();
    let mut st_inputs = synthetic_st_inputs();
    st_inputs.extend(corpus_st);
    let mut file_inputs = synthetic_file_inputs();
    file_inputs.extend(corpus_files);

    // lex
    {
        let mut g = c.benchmark_group("lex");
        for inp in &st_inputs {
            g.throughput(Throughput::Bytes(inp.src.len() as u64));
            g.bench_function(BenchmarkId::from_parameter(&inp.name), |b| {
                b.iter(|| black_box(lex(black_box(&inp.src))))
            });
        }
        g.finish();
    }

    // parse (lex + parse)
    {
        let mut g = c.benchmark_group("parse");
        for inp in &st_inputs {
            g.throughput(Throughput::Bytes(inp.src.len() as u64));
            g.bench_function(BenchmarkId::from_parameter(&inp.name), |b| {
                match inp.section {
                    StSection::Declaration => {
                        b.iter(|| black_box(parse_declaration(black_box(&inp.src))))
                    }
                    _ => b.iter(|| black_box(parse_implementation(black_box(&inp.src)))),
                }
            });
        }
        g.finish();
    }

    // format (attach + lower + print); the parse tree is built once in setup.
    {
        let mut g = c.benchmark_group("format");
        for inp in &st_inputs {
            g.throughput(Throughput::Bytes(inp.src.len() as u64));
            match inp.section {
                StSection::Declaration => {
                    let parse = parse_declaration(&inp.src);
                    g.bench_function(BenchmarkId::from_parameter(&inp.name), |b| {
                        b.iter(|| {
                            black_box(format_declaration(
                                black_box(&parse),
                                &style,
                                LineEnding::Lf,
                            ))
                        })
                    });
                }
                _ => {
                    let parse = parse_implementation(&inp.src);
                    g.bench_function(BenchmarkId::from_parameter(&inp.name), |b| {
                        b.iter(|| {
                            black_box(format_implementation(
                                black_box(&parse),
                                &style,
                                LineEnding::Lf,
                            ))
                        })
                    });
                }
            }
        }
        g.finish();
    }

    // format_st (end-to-end raw ST)
    {
        let mut g = c.benchmark_group("format_st");
        for inp in &st_inputs {
            g.throughput(Throughput::Bytes(inp.src.len() as u64));
            g.bench_function(BenchmarkId::from_parameter(&inp.name), |b| {
                b.iter(|| black_box(format_st(black_box(&inp.src), inp.section, &opts)))
            });
        }
        g.finish();
    }

    // xml_locate
    {
        let mut g = c.benchmark_group("xml_locate");
        for inp in &file_inputs {
            g.throughput(Throughput::Bytes(inp.bytes.len() as u64));
            g.bench_function(BenchmarkId::from_parameter(&inp.name), |b| {
                b.iter(|| black_box(locate_st_spans(black_box(&inp.bytes))))
            });
        }
        g.finish();
    }

    // xml_reformat: identity vs real. (real − identity) ≈ pure parse+format of the
    // spans, isolating the splice/locate overhead in `identity`.
    {
        let mut g = c.benchmark_group("xml_reformat");
        for inp in &file_inputs {
            g.throughput(Throughput::Bytes(inp.bytes.len() as u64));
            g.bench_function(BenchmarkId::new("identity", &inp.name), |b| {
                b.iter(|| black_box(reformat_document(black_box(&inp.bytes), identity)))
            });
            // Only time the real path when the document formats cleanly, else this
            // would measure the error-return path.
            if format_twincat_file(&inp.bytes, &opts).is_ok() {
                g.bench_function(BenchmarkId::new("real", &inp.name), |b| {
                    b.iter(|| {
                        black_box(reformat_document(black_box(&inp.bytes), real_format(&opts)))
                    })
                });
            }
        }
        g.finish();
    }

    // format_file (end-to-end TwinCAT file)
    {
        let mut g = c.benchmark_group("format_file");
        for inp in &file_inputs {
            if format_twincat_file(&inp.bytes, &opts).is_err() {
                continue;
            }
            g.throughput(Throughput::Bytes(inp.bytes.len() as u64));
            g.bench_function(BenchmarkId::from_parameter(&inp.name), |b| {
                b.iter(|| black_box(format_twincat_file(black_box(&inp.bytes), &opts)))
            });
        }
        g.finish();
    }
}

criterion_group!(benches, pipeline);
criterion_main!(benches);
