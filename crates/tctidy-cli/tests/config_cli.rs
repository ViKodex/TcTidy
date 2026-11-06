//! `.tctidy.toml` end to end: `--config`, discovery, `--print-config`, and the
//! handling of an invalid configuration.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_tctidy");

const MESSY: &str = "if a then\nb:=1;\nend_if;\n";

/// A fresh, empty directory unique to this test.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tctidy-config-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str], cwd: &Path, stdin: &str) -> Output {
    let mut child = Command::new(BIN)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tctidy");
    // A run that fails before reading stdin closes the pipe early; that broken
    // pipe is expected, the exit status is what the tests check.
    let _ = child.stdin.take().unwrap().write_all(stdin.as_bytes());
    child.wait_with_output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8(o.stderr.clone()).unwrap()
}

#[test]
fn explicit_config_applies_to_stdin() {
    let dir = scratch("explicit");
    let cfg = dir.join("style.toml");
    std::fs::write(&cfg, "indent_style = \"tabs\"\nkeyword_case = \"lower\"\n").unwrap();

    let out = run(&["-", "--config", cfg.to_str().unwrap()], &dir, MESSY);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out), "if a then\n\tb := 1;\nend_if;\n");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn stdin_discovers_the_config_from_the_current_directory() {
    let dir = scratch("stdin");
    std::fs::write(dir.join(".tctidy.toml"), "indent_width = 2\n").unwrap();
    let nested = dir.join("sub");
    std::fs::create_dir_all(&nested).unwrap();

    let out = run(&["-"], &nested, MESSY);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out), "IF a THEN\n  b := 1;\nEND_IF;\n");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn stdin_filepath_picks_the_config_of_that_file() {
    let dir = scratch("stdin-filepath");
    let (project, elsewhere) = (dir.join("project").join("plc"), dir.join("elsewhere"));
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(
        dir.join("project").join(".tctidy.toml"),
        "indent_width = 2\n",
    )
    .unwrap();
    std::fs::write(elsewhere.join(".tctidy.toml"), "keyword_case = \"lower\"\n").unwrap();
    // The file need not exist: an editor formats unsaved text for it.
    let pou = project.join("FB_Unsaved.TcPOU");

    let out = run(
        &["-", "--stdin-filepath", pou.to_str().unwrap()],
        &elsewhere,
        MESSY,
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out), "IF a THEN\n  b := 1;\nEND_IF;\n");

    let out = run(
        &["--print-config", "--stdin-filepath", pou.to_str().unwrap()],
        &elsewhere,
        "",
    );
    assert!(
        stdout(&out).contains("indent_width = 2\n"),
        "{}",
        stdout(&out)
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn files_use_their_nearest_config() {
    let dir = scratch("nearest");
    let (a, b) = (dir.join("a"), dir.join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    std::fs::write(dir.join(".tctidy.toml"), "indent_width = 2\n").unwrap();
    std::fs::write(a.join(".tctidy.toml"), "keyword_case = \"lower\"\n").unwrap();
    std::fs::write(a.join("x.st"), MESSY).unwrap();
    std::fs::write(b.join("y.st"), MESSY).unwrap();

    // Run from elsewhere: discovery follows each file, not the cwd.
    let out = run(
        &[
            a.join("x.st").to_str().unwrap(),
            b.join("y.st").to_str().unwrap(),
        ],
        &std::env::temp_dir(),
        "",
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    // The nearest file wins outright (no merging with the parent's).
    assert_eq!(
        std::fs::read_to_string(a.join("x.st")).unwrap(),
        "if a then\n    b := 1;\nend_if;\n"
    );
    assert_eq!(
        std::fs::read_to_string(b.join("y.st")).unwrap(),
        "IF a THEN\n  b := 1;\nEND_IF;\n"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_invalid_discovered_config_leaves_files_untouched() {
    let dir = scratch("invalid");
    std::fs::write(
        dir.join(".tctidy.toml"),
        "indent_width = 2\nkeyword_cse = \"lower\"\n",
    )
    .unwrap();
    let file = dir.join("x.st");
    std::fs::write(&file, MESSY).unwrap();

    let out = run(&[file.to_str().unwrap()], &dir, "");
    assert_eq!(out.status.code(), Some(2));
    let err = stderr(&out);
    assert!(
        err.contains(
            ".tctidy.toml:2:1: unknown option `keyword_cse` (did you mean `keyword_case`?)"
        ),
        "{err}"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), MESSY);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_missing_explicit_config_is_a_usage_error() {
    let dir = scratch("missing");
    let out = run(&["-", "--config", "nope.toml"], &dir, MESSY);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("nope.toml: cannot read"),
        "{}",
        stderr(&out)
    );
    assert!(stdout(&out).is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn print_config_lists_every_setting_and_round_trips() {
    let dir = scratch("print");
    std::fs::write(dir.join(".tctidy.toml"), "line_width = 120\n").unwrap();

    let out = run(&["--print-config"], &dir, "");
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let printed = stdout(&out);
    assert!(printed.contains("line_width = 120\n"), "{printed}");
    assert!(printed.contains("indent_style = \"spaces\"\n"), "{printed}");
    assert!(printed.contains("blank_lines_max = 2\n"), "{printed}");

    // Feeding the printed config back produces the same formatting.
    let cfg = dir.join("printed.toml");
    std::fs::write(&cfg, &printed).unwrap();
    let a = run(&["-"], &dir, MESSY);
    let b = run(&["-", "--config", cfg.to_str().unwrap()], &dir, MESSY);
    assert_eq!(stdout(&a), stdout(&b));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn print_config_rejects_format_arguments() {
    let dir = scratch("print-args");
    let out = run(&["--print-config", "x.st"], &dir, "");
    assert_eq!(out.status.code(), Some(2));
    std::fs::remove_dir_all(&dir).unwrap();
}
