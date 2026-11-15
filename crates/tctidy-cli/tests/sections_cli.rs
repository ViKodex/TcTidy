//! `tctidy --stdin-sections` end to end: several ST sections in one run, each
//! with its own grammar, configuration and result.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_tctidy");

/// A fresh, empty directory unique to this test.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tctidy-sections-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(BIN)
        .args(args)
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

/// Send `sections` as one request; return the response's results and the exit code.
fn format(sections: Value) -> (Vec<Value>, i32) {
    let out = run(
        &["--stdin-sections"],
        &json!({ "sections": sections }).to_string(),
    );
    let response: Value = serde_json::from_slice(&out.stdout).expect("a JSON response");
    (
        response["results"].as_array().expect("results").clone(),
        out.status.code().unwrap(),
    )
}

#[test]
fn each_section_is_formatted_with_its_own_grammar_in_request_order() {
    let (results, code) = format(json!([
        { "source": "x:=1;\r\nIF a THEN\r\nb:=2;\r\nEND_IF\r\n" },
        { "source": "VAR\na:INT;\nEND_VAR", "declaration": true },
        { "source": "y := 2;", "declaration": false },
    ]));

    assert_eq!(code, 0);
    assert_eq!(
        results,
        [
            // Line endings and the final newline are kept, as for `tctidy -`.
            json!({ "output": "x := 1;\r\nIF a THEN\r\n    b := 2;\r\nEND_IF\r\n" }),
            json!({ "output": "VAR\n    a : INT;\nEND_VAR" }),
            json!({ "output": "y := 2;" }),
        ]
    );
}

#[test]
fn a_section_that_fails_does_not_stop_the_others() {
    let (results, code) = format(json!([
        { "source": "x:=1;" },
        { "source": "x := 1;\nEND_IF\n" },
        { "source": "y:=2;" },
    ]));

    assert_eq!(code, 3, "a section that fails to format exits 3");
    assert_eq!(
        results,
        [
            json!({ "output": "x := 1;" }),
            json!({ "error": "parse error in implementation: line 2" }),
            json!({ "output": "y := 2;" }),
        ]
    );
}

#[test]
fn each_section_uses_the_config_of_its_file() {
    let dir = scratch("config");
    let lower = dir.join("lower");
    std::fs::create_dir_all(&lower).unwrap();
    std::fs::write(lower.join(".tctidy.toml"), "keyword_case = \"lower\"\n").unwrap();
    let source = "IF a THEN\nb:=1;\nEND_IF";

    // The files need not exist: only their directories choose the configuration.
    let (results, code) = format(json!([
        { "source": source, "filepath": lower.join("FB_A.TcPOU") },
        { "source": source, "filepath": dir.join("FB_B.TcPOU") },
    ]));

    assert_eq!(code, 0);
    assert_eq!(
        results,
        [
            json!({ "output": "if a then\n    b := 1;\nend_if" }),
            json!({ "output": "IF a THEN\n    b := 1;\nEND_IF" }),
        ]
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_invalid_config_fails_only_the_sections_it_covers() {
    let dir = scratch("invalid-config");
    let broken = dir.join("broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join(".tctidy.toml"), "no_such_setting = 1\n").unwrap();

    let (results, code) = format(json!([
        { "source": "x:=1;", "filepath": broken.join("FB_A.TcPOU") },
        { "source": "y:=2;", "filepath": dir.join("FB_B.TcPOU") },
    ]));

    assert_eq!(code, 2, "an invalid configuration exits 2");
    let error = results[0]["error"].as_str().expect("an error");
    assert!(error.contains("no_such_setting"), "got: {error}");
    assert_eq!(results[1], json!({ "output": "y := 2;" }));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_leading_byte_order_mark_is_skipped() {
    // A .NET Framework caller writes one before the request when the system
    // code page is UTF-8.
    let request = json!({ "sections": [{ "source": "x:=1;" }] });
    let out = run(&["--stdin-sections"], &format!("\u{FEFF}{request}"));

    assert_eq!(out.status.code(), Some(0));
    let response: Value = serde_json::from_slice(&out.stdout).expect("a JSON response");
    assert_eq!(response, json!({ "results": [{ "output": "x := 1;" }] }));
}

#[test]
fn an_empty_request_gets_an_empty_response() {
    let (results, code) = format(json!([]));

    assert_eq!(code, 0);
    assert!(results.is_empty());
}

#[test]
fn an_invalid_request_is_a_usage_error_and_writes_nothing() {
    for request in ["", "x := 1;", r#"{"sections":[{"declaration":true}]}"#] {
        let out = run(&["--stdin-sections"], request);

        assert_eq!(out.status.code(), Some(2), "request: {request}");
        assert!(out.stdout.is_empty(), "request: {request}");
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert!(
            stderr.starts_with("tctidy: --stdin-sections: invalid request: "),
            "got: {stderr}"
        );
    }
}

#[test]
fn the_other_modes_are_rejected() {
    for args in [
        &["--stdin-sections", "-"][..],
        &["--stdin-sections", "--check"],
        &["--stdin-sections", "--decl"],
        &["--stdin-sections", "--stdin-filepath", "FB_A.TcPOU"],
        &["--stdin-sections", "--files-from", "-"],
    ] {
        let out = run(args, r#"{"sections":[]}"#);

        assert_eq!(out.status.code(), Some(2), "args: {args:?}");
        assert!(out.stdout.is_empty(), "args: {args:?}");
    }
}
