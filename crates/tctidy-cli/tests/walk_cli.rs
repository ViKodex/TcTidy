//! End-to-end tests for batch target discovery: exit codes for missing paths
//! and empty globs, and the content-based TwinCAT detection. The pure walking
//! logic is unit-tested in `src/walk.rs`.

use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_tctidy");

fn run(args: &[&std::ffi::OsStr]) -> (String, i32) {
    let out = Command::new(BIN)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("run tctidy");
    (
        String::from_utf8(out.stderr).unwrap(),
        out.status.code().unwrap(),
    )
}

#[test]
fn nonexistent_literal_path_reports_and_exits_3() {
    let gone = std::env::temp_dir().join(format!("tctidy-gone-{}.TcDUT", std::process::id()));
    let (err, code) = run(&[gone.as_os_str()]);
    assert_eq!(code, 3, "a nonexistent literal path must fail the run");
    assert!(
        err.contains("no such file or directory"),
        "stderr must name the problem; got:\n{err}"
    );
}

#[test]
fn glob_matching_nothing_does_not_fail_the_run() {
    let root = std::env::temp_dir().join(format!("tctidy-emptyglob-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let pattern = format!(
        "{}/Nothing*.TcDUT",
        root.display().to_string().replace('\\', "/")
    );
    let (_err, code) = run(&[std::ffi::OsStr::new(&pattern)]);
    assert_eq!(code, 0, "an empty glob match is not an error");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn xml_content_in_a_non_twincat_extension_takes_the_container_path() {
    // BOM + leading whitespace + XML content under a `.xml` extension: the
    // content probe (not the extension) must route it down the TwinCAT
    // container path, so the XML survives and only the ST is formatted.
    let path = std::env::temp_dir().join(format!("tctidy-content-{}.xml", std::process::id()));
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(
        b"\r\n  <TcPlcObject Version=\"1.1.0.1\">\r\n  <DUT Name=\"X\" Id=\"{00000000-0000-0000-0000-000000000000}\">\r\n    <Declaration><![CDATA[tYpE X : sTrUcT\na:INT;\neNd_sTrUcT\neNd_tYpE]]></Declaration>\r\n  </DUT>\r\n</TcPlcObject>\r\n",
    );
    std::fs::write(&path, &bytes).unwrap();

    let (_err, code) = run(&[path.as_os_str()]);
    assert_eq!(code, 0);

    let after = std::fs::read(&path).unwrap();
    assert_eq!(&after[..3], &[0xEF, 0xBB, 0xBF], "the BOM must survive");
    let text = String::from_utf8(after).unwrap();
    assert!(text.contains("TYPE X : STRUCT"), "got:\n{text}");
    assert!(
        text.contains("Id=\"{00000000-0000-0000-0000-000000000000}\""),
        "the XML container must be untouched; got:\n{text}"
    );

    let _ = std::fs::remove_file(&path);
}
