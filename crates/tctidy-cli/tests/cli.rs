//! End-to-end CLI tests driving the built `tctidy` binary.

use std::io::Write;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_tctidy");

/// Run the binary with `args`, feeding `input` on stdin; return (stdout, exit code).
fn run_stdin(args: &[&str], input: &str) -> (String, i32) {
    let mut child = Command::new(BIN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn tctidy");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8(out.stdout).unwrap(),
        out.status.code().unwrap(),
    )
}

#[test]
fn installer_shell_discovery_does_not_require_a_format_path() {
    let output = Command::new(BIN)
        .arg("--discover-xae-shells")
        .output()
        .expect("run XAE Shell discovery");

    assert!(output.status.success());
}

#[test]
fn installer_shell_discovery_rejects_format_arguments() {
    let output = Command::new(BIN)
        .args(["--discover-xae-shells", "input.TcPOU"])
        .output()
        .expect("run XAE Shell discovery with a conflicting path");

    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn installer_cache_cleanup_rejects_format_arguments() {
    let output = Command::new(BIN)
        .args(["--clear-xae-cache", "15.0", "input.TcPOU"])
        .output()
        .expect("run XAE Shell cache cleanup with a conflicting path");

    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn installer_cache_cleanup_rejects_unknown_generations() {
    let output = Command::new(BIN)
        .args(["--clear-xae-cache", r"..\..\Victim"])
        .output()
        .expect("run XAE Shell cache cleanup with an unsafe generation");

    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn stdin_implementation_is_formatted() {
    let (out, code) = run_stdin(&["-", "--impl"], "x:=1;\nIF a THEN\nb:=2;\nEND_IF");
    assert_eq!(code, 0);
    assert_eq!(out, "x := 1;\nIF a THEN\n    b := 2;\nEND_IF");
}

#[test]
fn stdin_declaration_is_formatted() {
    let (out, code) = run_stdin(&["-", "--decl"], "VAR\na:INT;b:BOOL:=TRUE;\nEND_VAR");
    assert_eq!(code, 0);
    assert_eq!(out, "VAR\n    a : INT;\n    b : BOOL := TRUE;\nEND_VAR");
}

#[test]
fn check_reports_change_then_clean() {
    let (_o, code) = run_stdin(&["-", "--check", "--impl"], "x:=1;");
    assert_eq!(code, 1, "unformatted input should exit 1 under --check");
    let (_o, code) = run_stdin(&["-", "--check", "--impl"], "x := 1;");
    assert_eq!(
        code, 0,
        "already-formatted input should exit 0 under --check"
    );
}

#[test]
fn stdin_twincat_xml_is_formatted() {
    // A messy .TcDUT piped on stdin is detected as TwinCAT XML (leading `<`) and
    // reformatted, with the XML container preserved byte-for-byte.
    let messy = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                 <TcPlcObject Version=\"1.1.0.1\">\n\
                 \x20 <DUT Name=\"X\" Id=\"{00000000-0000-0000-0000-000000000000}\">\n\
                 \x20   <Declaration><![CDATA[tYpE X : sTrUcT\na:INT;\neNd_sTrUcT\neNd_tYpE]]></Declaration>\n\
                 \x20 </DUT>\n\
                 </TcPlcObject>\n";
    let (out, code) = run_stdin(&["-"], messy);
    assert_eq!(code, 0);
    assert!(out.contains("TYPE X : STRUCT"), "got:\n{out}");
    assert!(out.contains("    a : INT;"), "got:\n{out}");
    // Everything outside the ST is preserved.
    assert!(
        out.contains("Id=\"{00000000-0000-0000-0000-000000000000}\""),
        "got:\n{out}"
    );
    assert!(
        out.contains("<TcPlcObject Version=\"1.1.0.1\">"),
        "got:\n{out}"
    );

    // Idempotent: feeding the formatted XML back is clean under --check.
    let (_o, code) = run_stdin(&["-", "--check"], &out);
    assert_eq!(
        code, 0,
        "already-formatted XML should be clean under --check"
    );
}

#[test]
fn twincat_file_round_trip_is_idempotent() {
    let path = std::env::temp_dir().join(format!("tctidy-cli-{}.TcDUT", std::process::id()));
    let messy = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                 <TcPlcObject Version=\"1.1.0.1\">\n\
                 \x20 <DUT Name=\"X\" Id=\"{00000000-0000-0000-0000-000000000000}\">\n\
                 \x20   <Declaration><![CDATA[tYpE X : sTrUcT\na:INT;\neNd_sTrUcT\neNd_tYpE]]></Declaration>\n\
                 \x20 </DUT>\n\
                 </TcPlcObject>\n";
    std::fs::write(&path, messy).unwrap();

    // Format in place.
    let code = Command::new(BIN)
        .arg(&path)
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .code()
        .unwrap();
    assert_eq!(code, 0);

    let formatted = std::fs::read_to_string(&path).unwrap();
    assert!(formatted.contains("TYPE X : STRUCT"), "got:\n{formatted}");
    assert!(formatted.contains("    a : INT;"), "got:\n{formatted}");
    assert!(
        formatted.contains("END_STRUCT\nEND_TYPE"),
        "got:\n{formatted}"
    );
    // The XML container is preserved byte-for-byte.
    assert!(formatted.contains("Id=\"{00000000-0000-0000-0000-000000000000}\""));

    // Idempotent: a second --check passes.
    let code = Command::new(BIN)
        .args(["--check"])
        .arg(&path)
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .code()
        .unwrap();
    assert_eq!(code, 0, "formatted file should be clean under --check");

    let _ = std::fs::remove_file(&path);
}

// A messy .TcDUT that formats to "TYPE <name> : STRUCT …"; written to a temp path
// and returned, so a `--files-from` list can point at several of them.
fn write_messy_dut(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("tctidy-ff-{name}-{}.TcDUT", std::process::id()));
    let messy = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <TcPlcObject Version=\"1.1.0.1\">\n\
         \x20 <DUT Name=\"{name}\" Id=\"{{00000000-0000-0000-0000-000000000000}}\">\n\
         \x20   <Declaration><![CDATA[tYpE {name} : sTrUcT\na:INT;\neNd_sTrUcT\neNd_tYpE]]></Declaration>\n\
         \x20 </DUT>\n\
         </TcPlcObject>\n"
    );
    std::fs::write(&path, messy).unwrap();
    path
}

// Write the same messy .TcDUT at an explicit path (used to place a file under a
// directory whose name contains glob metacharacters).
fn write_messy_dut_at(path: &std::path::Path, name: &str) {
    let messy = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <TcPlcObject Version=\"1.1.0.1\">\n\
         \x20 <DUT Name=\"{name}\" Id=\"{{00000000-0000-0000-0000-000000000000}}\">\n\
         \x20   <Declaration><![CDATA[tYpE {name} : sTrUcT\na:INT;\neNd_sTrUcT\neNd_tYpE]]></Declaration>\n\
         \x20 </DUT>\n\
         </TcPlcObject>\n"
    );
    std::fs::write(path, messy).unwrap();
}

#[test]
fn files_from_file_formats_every_listed_path() {
    let a = write_messy_dut("FromFileA");
    let b = write_messy_dut("FromFileB");
    let list = std::env::temp_dir().join(format!("tctidy-ff-list-{}.txt", std::process::id()));
    // Blank lines and a `#` comment must be ignored.
    std::fs::write(
        &list,
        format!("# the batch\n{}\n\n{}\n", a.display(), b.display()),
    )
    .unwrap();

    let code = Command::new(BIN)
        .arg("--files-from")
        .arg(&list)
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .code()
        .unwrap();
    assert_eq!(code, 0);

    assert!(std::fs::read_to_string(&a)
        .unwrap()
        .contains("TYPE FromFileA : STRUCT"));
    assert!(std::fs::read_to_string(&b)
        .unwrap()
        .contains("TYPE FromFileB : STRUCT"));

    for p in [a, b, list] {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn files_from_stdin_reads_the_list_from_stdin() {
    let a = write_messy_dut("FromStdin");
    // `--files-from -` reads the *list* from stdin (not ST).
    let (_out, code) = run_stdin(&["--files-from", "-"], &format!("{}\n", a.display()));
    assert_eq!(code, 0);
    assert!(std::fs::read_to_string(&a)
        .unwrap()
        .contains("TYPE FromStdin : STRUCT"));
    let _ = std::fs::remove_file(a);
}

#[test]
fn formats_file_whose_path_contains_glob_chars() {
    // A `[NN]-Name` bracketed folder is a common layout; the brackets must not be
    // mistaken for a glob character class (which matches nothing → the file is
    // silently skipped and the base directory is walked per path). An existing file
    // path wins over glob interpretation.
    let root = std::env::temp_dir().join(format!("tctidy-brackets-{}", std::process::id()));
    let dir = root.join("[07]-proj"); // the bracket lives in a component under `root`
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("Bracketed.TcDUT");

    // (a) as a positional argument
    write_messy_dut_at(&file, "Bracketed");
    let code = Command::new(BIN)
        .arg(&file)
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .code()
        .unwrap();
    assert_eq!(code, 0);
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("TYPE Bracketed : STRUCT"),
        "a file under a bracketed path was not formatted (positional)"
    );

    // (b) via `--files-from -` (the extension's project-wide batch path)
    write_messy_dut_at(&file, "Bracketed");
    let (_out, code) = run_stdin(&["--files-from", "-"], &format!("{}\n", file.display()));
    assert_eq!(code, 0);
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("TYPE Bracketed : STRUCT"),
        "a file under a bracketed path was not formatted (--files-from)"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn bracketed_directory_is_recursed() {
    // A directory whose name contains brackets is recursed literally (`is_dir`
    // wins over glob interpretation), not expanded as a pattern.
    let root = std::env::temp_dir().join(format!("tctidy-brackets-dir-{}", std::process::id()));
    let dir = root.join("[09]-proj");
    std::fs::create_dir_all(&dir).unwrap();
    let inner = dir.join("Inner.TcDUT");
    write_messy_dut_at(&inner, "Inner");

    let code = Command::new(BIN)
        .arg(&dir)
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .code()
        .unwrap();
    assert_eq!(code, 0);
    assert!(
        std::fs::read_to_string(&inner)
            .unwrap()
            .contains("TYPE Inner : STRUCT"),
        "a file inside a bracketed directory was not formatted"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn glob_pattern_still_expands() {
    // Existence-first matching must not break real glob patterns: a non-existent
    // literal carrying a glob character is still expanded.
    let root = std::env::temp_dir().join(format!("tctidy-brackets-glob-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let a = root.join("GlobA.TcDUT");
    let b = root.join("GlobB.TcDUT");
    write_messy_dut_at(&a, "GlobA");
    write_messy_dut_at(&b, "GlobB");
    // Forward slashes so the pattern is separator-portable for globset.
    let pattern = format!("{}/*.TcDUT", root.display().to_string().replace('\\', "/"));

    let code = Command::new(BIN)
        .arg(&pattern)
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .code()
        .unwrap();
    assert_eq!(code, 0);
    assert!(std::fs::read_to_string(&a)
        .unwrap()
        .contains("TYPE GlobA : STRUCT"));
    assert!(std::fs::read_to_string(&b)
        .unwrap()
        .contains("TYPE GlobB : STRUCT"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn files_from_missing_file_exits_usage() {
    let missing =
        std::env::temp_dir().join(format!("tctidy-ff-missing-{}.txt", std::process::id()));
    let code = Command::new(BIN)
        .arg("--files-from")
        .arg(&missing)
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .code()
        .unwrap();
    assert_eq!(code, 2, "a missing --files-from file is a usage error");
}

#[test]
fn parse_error_leaves_the_file_byte_identical() {
    // The non-destruction guarantee at the IO boundary: a file whose ST cannot be
    // parsed (here a Unicode identifier the lexer rejects) is refused, the binary
    // exits 3 and never writes, so the original survives byte-for-byte.
    let path = std::env::temp_dir().join(format!("tctidy-refuse-{}.TcDUT", std::process::id()));
    let original = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n\
                    <TcPlcObject Version=\"1.1.0.1\">\r\n\
                    \x20 <DUT Name=\"X\" Id=\"{00000000-0000-0000-0000-000000000000}\">\r\n\
                    \x20   <Declaration><![CDATA[TYPE X : STRUCT\r\ncafé : INT;\r\nEND_STRUCT\r\nEND_TYPE]]></Declaration>\r\n\
                    \x20 </DUT>\r\n\
                    </TcPlcObject>\r\n";
    std::fs::write(&path, original).unwrap();

    let code = Command::new(BIN)
        .arg(&path)
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .code()
        .unwrap();
    assert_eq!(code, 3, "a file that fails to parse must exit 3");

    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        after, original,
        "a refused file must be left byte-for-byte unchanged"
    );

    let _ = std::fs::remove_file(&path);
}

/// Run the binary with `args`, feeding `input` on stdin; return (stderr, exit code).
/// Unlike `run_stdin`, this captures stderr, where diagnostics are written.
fn run_stdin_stderr(args: &[&str], input: &str) -> (String, i32) {
    let mut child = Command::new(BIN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tctidy");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8(out.stderr).unwrap(),
        out.status.code().unwrap(),
    )
}

/// Run the binary with `args` (no stdin); return (stdout, stderr, exit code).
fn run_args(args: &[&std::ffi::OsStr]) -> (String, String, i32) {
    let out = Command::new(BIN)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run tctidy");
    (
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
        out.status.code().unwrap(),
    )
}

fn temp_file(tag: &str, ext: &str, content: &[u8]) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("tctidy-{tag}-{}.{ext}", std::process::id()));
    std::fs::write(&path, content).unwrap();
    path
}

const MESSY_DUT: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                         <TcPlcObject Version=\"1.1.0.1\">\n\
                         \x20 <DUT Name=\"X\" Id=\"{00000000-0000-0000-0000-000000000000}\">\n\
                         \x20   <Declaration><![CDATA[tYpE X : sTrUcT\na:INT;\neNd_sTrUcT\neNd_tYpE]]></Declaration>\n\
                         \x20 </DUT>\n\
                         </TcPlcObject>\n";

#[test]
fn check_on_a_dirty_file_exits_1_and_writes_nothing() {
    let path = temp_file("check-dirty", "TcDUT", MESSY_DUT.as_bytes());
    let (_out, err, code) = run_args(&[std::ffi::OsStr::new("--check"), path.as_os_str()]);
    assert_eq!(
        code, 1,
        "a file that would change must exit 1 under --check"
    );
    assert!(err.contains("would reformat"), "got:\n{err}");
    assert_eq!(
        std::fs::read(&path).unwrap(),
        MESSY_DUT.as_bytes(),
        "--check must leave the file byte-identical"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn check_on_a_clean_file_exits_0() {
    let dirty = temp_file("check-clean", "TcDUT", MESSY_DUT.as_bytes());
    let (_o, _e, code) = run_args(&[dirty.as_os_str()]);
    assert_eq!(code, 0);
    let (_o, err, code) = run_args(&[std::ffi::OsStr::new("--check"), dirty.as_os_str()]);
    assert_eq!(code, 0, "an already-formatted file is clean under --check");
    assert!(!err.contains("would reformat"), "got:\n{err}");
    let _ = std::fs::remove_file(&dirty);
}

#[test]
fn diff_on_a_changed_file_prints_and_writes_nothing() {
    let path = temp_file("diff-dirty", "TcDUT", MESSY_DUT.as_bytes());
    let (out, _err, code) = run_args(&[std::ffi::OsStr::new("--diff"), path.as_os_str()]);
    assert_eq!(code, 0, "--diff reports, it does not fail the run");
    assert!(out.contains("---"), "unified header expected; got:\n{out}");
    assert!(out.contains("+++"), "unified header expected; got:\n{out}");
    assert!(
        out.lines()
            .any(|l| l.starts_with('+') && l.contains("TYPE X : STRUCT")),
        "the formatted side must appear; got:\n{out}"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        MESSY_DUT.as_bytes(),
        "--diff must not write the file"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn diff_on_a_clean_file_prints_nothing() {
    let path = temp_file("diff-clean", "TcDUT", MESSY_DUT.as_bytes());
    let (_o, _e, code) = run_args(&[path.as_os_str()]);
    assert_eq!(code, 0);
    let (out, _err, code) = run_args(&[std::ffi::OsStr::new("--diff"), path.as_os_str()]);
    assert_eq!(code, 0);
    assert_eq!(out, "", "a clean file produces an empty diff");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn raw_st_file_is_formatted_in_place() {
    // No XML container: the file goes down the raw-ST path (implementation
    // grammar by default).
    let path = temp_file("raw-impl", "st", b"x:=1;\nIF a THEN\nb:=2;\nEND_IF");
    let (_o, _e, code) = run_args(&[path.as_os_str()]);
    assert_eq!(code, 0);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "x := 1;\nIF a THEN\n    b := 2;\nEND_IF"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn raw_st_file_with_impl_flag_is_formatted_in_place() {
    let path = temp_file("raw-impl-flag", "st", b"x:=1;");
    let (_o, _e, code) = run_args(&[std::ffi::OsStr::new("--impl"), path.as_os_str()]);
    assert_eq!(code, 0);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "x := 1;");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn raw_st_file_with_decl_flag_uses_the_declaration_grammar() {
    // A VAR block only parses under the declaration grammar; --decl must reach
    // the file path, not just stdin.
    let path = temp_file("raw-decl", "st", b"VAR\na:INT;b:BOOL:=TRUE;\nEND_VAR");
    let (_o, _e, code) = run_args(&[std::ffi::OsStr::new("--decl"), path.as_os_str()]);
    assert_eq!(code, 0);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "VAR\n    a : INT;\n    b : BOOL := TRUE;\nEND_VAR"
    );
    let _ = std::fs::remove_file(&path);
}

/// Raw ST and its formatted form, by final line break: an LF or CRLF one is
/// kept in the input's style, and an input without one does not gain one.
const FINAL_NEWLINE_CASES: [(&str, &str, &str); 3] = [
    (
        "lf",
        "IF a THEN\nb:=1;\nEND_IF\n",
        "IF a THEN\n    b := 1;\nEND_IF\n",
    ),
    (
        "crlf",
        "IF a THEN\r\nb:=1;\r\nEND_IF\r\n",
        "IF a THEN\r\n    b := 1;\r\nEND_IF\r\n",
    ),
    (
        "none",
        "IF a THEN\nb:=1;\nEND_IF",
        "IF a THEN\n    b := 1;\nEND_IF",
    ),
];

#[test]
fn stdin_raw_st_keeps_its_final_newline() {
    for (eol, messy, formatted) in FINAL_NEWLINE_CASES {
        let (out, code) = run_stdin(&["-"], messy);
        assert_eq!(code, 0, "{eol}");
        assert_eq!(out, formatted, "{eol}");

        // Idempotent: the output formats to itself and is clean under --check.
        let (again, code) = run_stdin(&["-"], &out);
        assert_eq!((again.as_str(), code), (formatted, 0), "{eol}");
        let (_o, code) = run_stdin(&["-", "--check"], &out);
        assert_eq!(
            code, 0,
            "{eol}: formatted input must be clean under --check"
        );
    }
}

#[test]
fn raw_st_file_keeps_its_final_newline() {
    for (eol, messy, formatted) in FINAL_NEWLINE_CASES {
        // An already formatted file is clean: --check passes, --diff is empty.
        let clean = temp_file(&format!("eol-clean-{eol}"), "st", formatted.as_bytes());
        let (_o, err, code) = run_args(&[std::ffi::OsStr::new("--check"), clean.as_os_str()]);
        assert_eq!(code, 0, "{eol}: got:\n{err}");
        let (out, _e, code) = run_args(&[std::ffi::OsStr::new("--diff"), clean.as_os_str()]);
        assert_eq!((out.as_str(), code), ("", 0), "{eol}");
        let _ = std::fs::remove_file(&clean);

        // The diff of a messy file keeps the final newline on the new side.
        let path = temp_file(&format!("eol-messy-{eol}"), "st", messy.as_bytes());
        let (out, _e, code) = run_args(&[std::ffi::OsStr::new("--diff"), path.as_os_str()]);
        assert_eq!(code, 0, "{eol}");
        assert_eq!(
            out.contains("\\ No newline at end of file"),
            !messy.ends_with('\n'),
            "{eol}: got:\n{out}"
        );

        // Formatting in place writes it, and the result is then clean.
        let (_o, _e, code) = run_args(&[path.as_os_str()]);
        assert_eq!(code, 0, "{eol}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), formatted, "{eol}");
        let (_o, _e, code) = run_args(&[std::ffi::OsStr::new("--check"), path.as_os_str()]);
        assert_eq!(code, 0, "{eol}: the formatted file must pass --check");
        let _ = std::fs::remove_file(&path);
    }
}

#[test]
fn twincat_st_gains_no_final_newline() {
    // ST that ends right at `]]>` gains no line break there, although the
    // document itself ends with one.
    let (out, code) = run_stdin(&["-"], MESSY_DUT);
    assert_eq!(code, 0);
    assert_eq!(
        out,
        MESSY_DUT.replace(
            "tYpE X : sTrUcT\na:INT;\neNd_sTrUcT\neNd_tYpE]]>",
            "TYPE X : STRUCT\n    a : INT;\nEND_STRUCT\nEND_TYPE]]>"
        )
    );
}

#[test]
fn twincat_st_keeps_its_final_newline() {
    // A line break before `]]>` is an empty last line in the TwinCAT editor:
    // it is kept, in the document's EOL, in a declaration and an implementation.
    let messy = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                 <TcPlcObject Version=\"1.1.0.1\">\n\
                 \x20 <POU Name=\"P\">\n\
                 \x20   <Declaration><![CDATA[PROGRAM P\nVAR\nx:INT;\nEND_VAR\n]]></Declaration>\n\
                 \x20   <Implementation>\n\
                 \x20     <ST><![CDATA[IF a THEN\nx:=1;\nEND_IF\n]]></ST>\n\
                 \x20   </Implementation>\n\
                 \x20 </POU>\n\
                 </TcPlcObject>\n";
    let formatted = messy
        .replace("x:INT;", "    x : INT;")
        .replace("x:=1;", "    x := 1;");
    for eol in ["\n", "\r\n"] {
        let (messy, formatted) = (messy.replace('\n', eol), formatted.replace('\n', eol));
        let (out, code) = run_stdin(&["-"], &messy);
        assert_eq!((out.as_str(), code), (formatted.as_str(), 0), "{eol:?}");

        // Idempotent: the formatted document is clean under --check.
        let (_o, code) = run_stdin(&["-", "--check"], &out);
        assert_eq!(
            code, 0,
            "{eol:?}: formatted XML must be clean under --check"
        );
    }
}

#[test]
fn invalid_utf8_file_is_refused_untouched() {
    let bytes: &[u8] = b"x := 1;\xFF\xFEy := 2;";
    let path = temp_file("bad-utf8", "st", bytes);
    let (_out, err, code) = run_args(&[path.as_os_str()]);
    assert_eq!(code, 3, "invalid UTF-8 must fail the run");
    assert!(err.contains("not valid UTF-8"), "got:\n{err}");
    assert_eq!(
        std::fs::read(&path).unwrap(),
        bytes,
        "a refused file must be left byte-for-byte unchanged"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn no_paths_at_all_is_a_usage_error() {
    let (_out, err, code) = run_args(&[]);
    assert_eq!(code, 2, "no input paths is a usage error");
    assert!(
        err.contains("no input paths"),
        "the hint must name the problem; got:\n{err}"
    );
    assert!(
        err.contains("use `-` to read from stdin"),
        "the stdin hint must be present; got:\n{err}"
    );
}

#[test]
fn stdin_diff_prints_a_unified_diff_without_output_echo() {
    let (out, code) = run_stdin(&["-", "--diff", "--impl"], "x:=1;");
    assert_eq!(code, 0);
    assert!(out.contains("a/<stdin>"), "got:\n{out}");
    assert!(out.contains("b/<stdin>"), "got:\n{out}");
    assert!(out.contains("-x:=1;"), "got:\n{out}");
    assert!(out.contains("+x := 1;"), "got:\n{out}");

    // A clean input produces no diff at all.
    let (out, code) = run_stdin(&["-", "--diff", "--impl"], "x := 1;");
    assert_eq!(code, 0);
    assert_eq!(out, "", "clean stdin input must produce an empty diff");
}

#[test]
fn stdin_xml_diff_prints_a_unified_diff_without_output_echo() {
    // The XML-stdin branch of --diff: dirty ST inside the container diffs.
    // Starting with the diff header (rather than the document) proves the
    // output is a diff, not the formatted document echo.
    let messy = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                 <TcPlcObject Version=\"1.1.0.1\">\n\
                 \x20 <DUT Name=\"X\" Id=\"{00000000-0000-0000-0000-000000000000}\">\n\
                 \x20   <Declaration><![CDATA[tYpE X : sTrUcT\na:INT;\neNd_sTrUcT\neNd_tYpE]]></Declaration>\n\
                 \x20 </DUT>\n\
                 </TcPlcObject>\n";
    let (out, code) = run_stdin(&["-", "--diff"], messy);
    assert_eq!(code, 0);
    assert!(out.starts_with("--- a/<stdin>"), "got:\n{out}");
    assert!(out.contains("+++ b/<stdin>"), "got:\n{out}");
    assert!(out.contains("-a:INT;"), "got:\n{out}");
    assert!(out.contains("+    a : INT;"), "got:\n{out}");

    // Clean XML produces no diff at all.
    let clean = run_stdin(&["-"], messy).0;
    let (out, code) = run_stdin(&["-", "--diff"], &clean);
    assert_eq!(code, 0);
    assert_eq!(out, "", "clean XML stdin input must produce an empty diff");
}

#[test]
fn parse_error_reports_the_section_and_line() {
    // A dangling END_IF (no matching IF) must be diagnosed with its section and
    // source line, a compact summary, not an opaque count.
    let (err, code) = run_stdin_stderr(&["-", "--impl"], "x := 1;\nEND_IF\n");
    assert_eq!(code, 3, "a parse failure exits 3");
    assert_eq!(
        err.trim(),
        "tctidy: parse error in implementation: line 2",
        "got:\n{err}"
    );
    // No redundant `<stdin>` locator: the message is self-describing.
    assert!(
        !err.contains("<stdin>"),
        "stale <stdin> locator; got:\n{err}"
    );
}

/// Say that a test could not run here. libtest has no skip status, so this
/// writes to the real stderr (output capture only covers `eprintln!`), where
/// `cargo test` shows it even for a test that returns early.
fn skip(test: &str, why: &str) {
    let _ = writeln!(std::io::stderr(), "SKIPPED {test}: {why}");
}

/// Create a symbolic link at `link` whose literal target is `target`; false
/// when the platform refuses it (Windows without the symbolic link privilege).
fn symlink(target: &std::path::Path, link: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).unwrap();
        true
    }
    #[cfg(windows)]
    {
        const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;
        match std::os::windows::fs::symlink_file(target, link) {
            Ok(()) => true,
            Err(e) if e.raw_os_error() == Some(ERROR_PRIVILEGE_NOT_HELD) => false,
            Err(e) => panic!("cannot create symlink {}: {e}", link.display()),
        }
    }
}

#[test]
fn formatting_through_a_symlink_keeps_the_link() {
    // `ln -s target.st link.st; tctidy link.st --impl` must format target.st
    // and leave link.st a link to it, not replace the link with a copy.
    let dir = std::env::temp_dir().join(format!("tctidy-symlink-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (target, link) = (dir.join("target.st"), dir.join("link.st"));
    std::fs::write(&target, "x:=1;").unwrap();
    if !symlink(std::path::Path::new("target.st"), &link) {
        skip(
            "formatting_through_a_symlink_keeps_the_link",
            "no symlink privilege",
        );
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }

    let (_out, err, code) = run_args(&[link.as_os_str(), std::ffi::OsStr::new("--impl")]);

    assert_eq!(code, 0, "got:\n{err}");
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink(),
        "link.st must still be a symbolic link"
    );
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        std::path::Path::new("target.st")
    );
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "x := 1;");
    let _ = std::fs::remove_dir_all(&dir);
}

// Target discovery failures: an invalid glob is a usage error (exit 2), a
// directory or glob walk that cannot read part of its tree is a failure
// (exit 3), and neither stops the other targets from being processed.

/// Allow or deny listing `dir` itself (not its children). Returns whether the
/// change was applied.
#[cfg(windows)]
fn set_listable(dir: &std::path::Path, listable: bool) -> bool {
    // A deny entry for Everyone (S-1-1-0) on "list folder" applies to every
    // token, an elevated administrator's included. The owner keeps the
    // implicit right to edit the ACL, so the entry can always be removed.
    let args: &[&str] = if listable {
        &["/remove:d", "*S-1-1-0"]
    } else {
        &["/deny", "*S-1-1-0:(RD)"]
    };
    Command::new("icacls")
        .arg(dir)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(unix)]
fn set_listable(dir: &std::path::Path, listable: bool) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let mode = if listable { 0o755 } else { 0o000 };
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode)).is_ok()
}

/// A scratch tree holding one messy readable file and one subdirectory the
/// current user cannot list, so walking the tree fails part-way. Dropping it
/// lifts the lock and removes the tree, even when the test fails.
struct UnreadableTree {
    root: std::path::PathBuf,
    readable: std::path::PathBuf,
    locked: std::path::PathBuf,
}

impl UnreadableTree {
    /// `None` when this user can list the directory despite the lock (root on
    /// Unix bypasses permissions): no walk error can be produced, so the caller
    /// skips. On Windows the lock always holds, and a failure to apply it fails
    /// the test.
    fn new(tag: &str) -> Option<UnreadableTree> {
        let root =
            std::env::temp_dir().join(format!("tctidy-unreadable-{tag}-{}", std::process::id()));
        let tree = UnreadableTree {
            readable: root.join("Readable.TcDUT"),
            locked: root.join("locked"),
            root,
        };
        // A leftover from an aborted run with the same process id.
        set_listable(&tree.locked, true);
        let _ = std::fs::remove_dir_all(&tree.root);

        std::fs::create_dir_all(&tree.locked).unwrap();
        write_messy_dut_at(&tree.readable, "Readable");
        write_messy_dut_at(&tree.locked.join("Hidden.TcDUT"), "Hidden");
        assert!(
            set_listable(&tree.locked, false),
            "could not lock {}",
            tree.locked.display()
        );
        if std::fs::read_dir(&tree.locked).is_ok() {
            if cfg!(windows) {
                panic!("the lock has no effect on {}", tree.locked.display());
            }
            eprintln!(
                "skipped: this user can list {} despite mode 000 (running as root?)",
                tree.locked.display()
            );
            return None;
        }
        Some(tree)
    }

    /// The pattern `<root>/**/*.TcDUT`, with forward slashes for globset.
    fn glob(&self) -> String {
        format!(
            "{}/**/*.TcDUT",
            self.root.display().to_string().replace('\\', "/")
        )
    }

    fn readable_was_formatted(&self) -> bool {
        std::fs::read_to_string(&self.readable)
            .unwrap()
            .contains("TYPE Readable : STRUCT")
    }

    /// The diagnostic prefix that names the unreadable directory.
    fn diagnostic(&self) -> String {
        format!("tctidy: {}: ", self.locked.display())
    }
}

impl Drop for UnreadableTree {
    fn drop(&mut self) {
        set_listable(&self.locked, true);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn invalid_glob_is_a_usage_error() {
    let (_out, err, code) = run_args(&[std::ffi::OsStr::new("[")]);
    assert_eq!(code, 2, "an invalid glob is a usage error; stderr:\n{err}");
    assert!(
        err.contains("tctidy: [: invalid glob pattern: "),
        "the diagnostic must name the pattern; got:\n{err}"
    );
}

#[test]
fn invalid_glob_still_formats_the_other_targets() {
    let root = std::env::temp_dir().join(format!("tctidy-badglob-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("Fine.TcDUT");
    write_messy_dut_at(&file, "Fine");
    let pattern = format!("{}/[", root.display().to_string().replace('\\', "/"));

    let (_out, err, code) = run_args(&[std::ffi::OsStr::new(&pattern), file.as_os_str()]);
    assert_eq!(code, 2, "an invalid glob is a usage error; stderr:\n{err}");
    assert!(
        err.contains(&format!("tctidy: {pattern}: invalid glob pattern: ")),
        "got:\n{err}"
    );
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("TYPE Fine : STRUCT"),
        "the valid target must still be formatted"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn glob_under_a_missing_directory_still_exits_0() {
    // A valid glob whose literal prefix does not exist matches nothing: that
    // is a no-op, not a walk error.
    let gone = std::env::temp_dir().join(format!("tctidy-nodir-{}", std::process::id()));
    let pattern = format!("{}/*.TcDUT", gone.display().to_string().replace('\\', "/"));
    let (_out, err, code) = run_args(&[std::ffi::OsStr::new(&pattern)]);
    assert_eq!(code, 0, "a glob matching nothing is not an error");
    assert_eq!(err, "", "nothing to report");
}

#[test]
fn unreadable_subdirectory_fails_the_run_after_formatting_the_rest() {
    let Some(tree) = UnreadableTree::new("dir") else {
        return;
    };
    let (_out, err, code) = run_args(&[tree.root.as_os_str()]);
    assert_eq!(
        code, 3,
        "a directory walk error must fail the run; stderr:\n{err}"
    );
    assert!(
        err.contains(&tree.diagnostic()),
        "the diagnostic must name the unreadable directory; got:\n{err}"
    );
    assert_eq!(
        err.matches(&tree.locked.display().to_string()).count(),
        1,
        "the path is named once; got:\n{err}"
    );
    assert!(
        tree.readable_was_formatted(),
        "the readable file must still be formatted"
    );
}

#[test]
fn glob_walk_error_fails_the_run_after_formatting_the_rest() {
    let Some(tree) = UnreadableTree::new("glob") else {
        return;
    };
    let pattern = tree.glob();
    let (_out, err, code) = run_args(&[std::ffi::OsStr::new(&pattern)]);
    assert_eq!(
        code, 3,
        "a glob walk error must fail the run; stderr:\n{err}"
    );
    assert!(
        err.contains(&tree.diagnostic()),
        "the diagnostic must name the unreadable directory; got:\n{err}"
    );
    assert!(
        tree.readable_was_formatted(),
        "the readable match must still be formatted"
    );
}

#[test]
fn walk_error_outranks_a_check_failure() {
    let Some(tree) = UnreadableTree::new("check") else {
        return;
    };
    let (_out, err, code) = run_args(&[std::ffi::OsStr::new("--check"), tree.root.as_os_str()]);
    assert_eq!(code, 3, "exit 3 outranks exit 1; stderr:\n{err}");
    assert!(
        err.contains("would reformat"),
        "the readable file is still checked; got:\n{err}"
    );
    assert!(err.contains(&tree.diagnostic()), "got:\n{err}");
}

#[test]
fn invalid_glob_outranks_a_walk_error() {
    let Some(tree) = UnreadableTree::new("rank") else {
        return;
    };
    let pattern = format!("{}/[", tree.root.display().to_string().replace('\\', "/"));
    let (_out, err, code) = run_args(&[std::ffi::OsStr::new(&pattern), tree.root.as_os_str()]);
    assert_eq!(code, 2, "exit 2 outranks exit 3; stderr:\n{err}");
    assert!(err.contains("invalid glob pattern"), "got:\n{err}");
    assert!(err.contains(&tree.diagnostic()), "got:\n{err}");
    assert!(
        tree.readable_was_formatted(),
        "the readable file must still be formatted"
    );
}

#[cfg(unix)]
#[test]
fn formatting_keeps_the_file_mode() {
    use std::os::unix::fs::PermissionsExt;

    let path = temp_file("mode", "st", b"x:=1;");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o751)).unwrap();

    let (_out, err, code) = run_args(&[path.as_os_str()]);

    assert_eq!(code, 0, "got:\n{err}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "x := 1;");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777;
    assert_eq!(mode, 0o751, "mode {mode:o}");
    let _ = std::fs::remove_file(&path);
}

#[cfg(windows)]
#[test]
fn formatting_keeps_the_hidden_attribute() {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;

    let path = std::env::temp_dir().join(format!("tctidy-hidden-{}.st", std::process::id()));
    let _ = std::fs::remove_file(&path);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .attributes(FILE_ATTRIBUTE_HIDDEN)
        .open(&path)
        .unwrap()
        .write_all(b"x:=1;")
        .unwrap();

    let (_out, err, code) = run_args(&[path.as_os_str()]);

    assert_eq!(code, 0, "got:\n{err}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "x := 1;");
    let attributes = std::fs::metadata(&path).unwrap().file_attributes();
    assert_ne!(
        attributes & FILE_ATTRIBUTE_HIDDEN,
        0,
        "the file must stay hidden"
    );
    let _ = std::fs::remove_file(&path);
}

#[cfg(unix)]
#[test]
fn a_directory_that_cannot_be_synced_fails_the_run_after_writing() {
    use std::os::unix::fs::PermissionsExt;

    // Write and search permission let tctidy replace the file, but without read
    // permission the directory cannot be opened to sync the rename: the file is
    // formatted, and the run must say the change may not survive a power loss.
    let dir = std::env::temp_dir().join(format!("tctidy-nosync-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("a.st");
    std::fs::write(&path, "x:=1;").unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o300)).unwrap();
    if std::fs::File::open(&dir).is_ok() {
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        skip(
            "a_directory_that_cannot_be_synced_fails_the_run_after_writing",
            "running with privileges that ignore directory permissions",
        );
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }

    let (_out, err, code) = run_args(&[path.as_os_str()]);
    let content = std::fs::read_to_string(&path).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(
        code, 3,
        "a change that may be lost is not a success; got:\n{err}"
    );
    assert!(
        err.contains(&format!("formatted {}, but", path.display()))
            && err.contains("power loss")
            && err.contains(&format!("cannot sync directory {}", dir.display())),
        "got:\n{err}"
    );
    assert_eq!(content, "x := 1;", "the formatted content is in place");
    let _ = std::fs::remove_dir_all(&dir);
}
