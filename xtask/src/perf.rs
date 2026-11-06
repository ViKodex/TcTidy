//! `cargo run -p xtask -- perf cold`, measure the per-file process cost.
//!
//! The criterion suite (`cargo bench -p tctidy`) times the formatter *in process*.
//! It cannot see the cost the VS extension and pre-commit actually pay: spawning
//! the `tctidy` binary once **per file**. This harness measures that with
//! [hyperfine](https://github.com/sharkdp/hyperfine):
//!
//! * `version`, `tctidy --version`: the pure spawn + dynamic-link floor.
//! * `tiny` / `medium`, `tctidy <file>`: the marginal cost of formatting a real
//!   file on top of that floor. `tiny − version` shows how much startup dominates.
//!
//! The sample files are formatted once during hyperfine's warm-up, so the timed
//! runs re-format already-clean files (no write), the read+parse+format cost the
//! editor pays on every invocation.

use std::path::{Path, PathBuf};
use std::process::Command;

const DECL: &str =
    "FUNCTION_BLOCK FB_Sample\nVAR\n    a : INT;\n    b : LREAL := 1.0;\n    c : BOOL;\nEND_VAR";
const STEP: &str =
    "IF a > 0 THEN\n    b := b + 1.0;\n    c := NOT c;\nELSE\n    a := a + 1;\nEND_IF";

/// A larger but still-valid declaration: a wide VAR block.
fn big_decl(vars: usize) -> String {
    let mut body = String::from("FUNCTION_BLOCK FB_Sample\nVAR\n");
    for i in 0..vars {
        body.push_str(&format!("    v{i} : INT := {i};\n"));
    }
    body.push_str("END_VAR");
    body
}

/// Wrap declaration + implementation in a minimal `.TcPOU` envelope (LF).
fn wrap_pou(decl: &str, body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<TcPlcObject Version=\"1.1.0.1\" ProductVersion=\"3.1.4024.0\">\n\
  <POU Name=\"FB_Sample\" Id=\"{{0a0a0a0a-0000-0000-0000-000000000000}}\" SpecialFunc=\"None\">\n\
    <Declaration><![CDATA[{decl}]]></Declaration>\n\
    <Implementation>\n\
      <ST><![CDATA[{body}]]></ST>\n\
    </Implementation>\n\
  </POU>\n\
</TcPlcObject>\n"
    )
}

pub fn run_cold() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    // 1. Build the release binary the editor would ship.
    let status = Command::new(&cargo)
        .args(["build", "-p", "tctidy-cli", "--release"])
        .status();
    match status {
        Ok(s) if s.success() => {}
        _ => {
            eprintln!("perf cold: failed to build tctidy-cli --release");
            std::process::exit(1);
        }
    }
    let bin = Path::new("target/release/tctidy");
    if !bin.exists() {
        eprintln!("perf cold: {} not found after build", bin.display());
        std::process::exit(1);
    }

    // 2. hyperfine must be installed (it does the statistics).
    if Command::new("hyperfine").arg("--version").output().is_err() {
        eprintln!(
            "perf cold: hyperfine not found. Install it with `cargo install hyperfine` \
             (or your package manager), then re-run."
        );
        std::process::exit(1);
    }

    // 3. Materialize a tiny and a medium sample file.
    let dir = PathBuf::from("target/perf");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("perf cold: cannot create {}: {e}", dir.display());
        std::process::exit(1);
    }
    let tiny = dir.join("tiny.TcPOU");
    let medium = dir.join("medium.TcPOU");
    let big_body = vec![STEP; 30].join("\n");
    write_file(&tiny, &wrap_pou(DECL, STEP));
    write_file(&medium, &wrap_pou(&big_decl(40), &big_body));

    // 4. Run hyperfine: pure spawn floor vs. marginal format cost.
    let status = Command::new("hyperfine")
        .args([
            "--warmup",
            "5",
            "-N",
            "--export-markdown",
            "target/perf/cold.md",
        ])
        .args(["-n", "version", &format!("{} --version", bin.display())])
        .args([
            "-n",
            "tiny",
            &format!("{} {}", bin.display(), tiny.display()),
        ])
        .args([
            "-n",
            "medium",
            &format!("{} {}", bin.display(), medium.display()),
        ])
        .status();
    match status {
        Ok(s) if s.success() => {
            println!("\nperf cold: table written to target/perf/cold.md");
        }
        _ => {
            eprintln!("perf cold: hyperfine run failed");
            std::process::exit(1);
        }
    }
}

fn write_file(path: &Path, contents: &str) {
    if let Err(e) = std::fs::write(path, contents) {
        eprintln!("perf cold: cannot write {}: {e}", path.display());
        std::process::exit(1);
    }
}
