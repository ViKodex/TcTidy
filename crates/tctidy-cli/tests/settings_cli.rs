//! `tctidy --settings` end to end: the loopback server, its access checks, and
//! the API the settings page uses.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_tctidy");

/// A fresh, empty directory unique to this test.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tctidy-settings-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A running server; killed when dropped.
struct Server {
    child: Child,
    port: u16,
    token: String,
}

impl Server {
    fn start(dir: &Path) -> Server {
        let mut child = Command::new(BIN)
            .arg("--settings")
            .arg(dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn tctidy");
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let rest = line
            .trim()
            .strip_prefix("http://127.0.0.1:")
            .unwrap_or_else(|| panic!("unexpected first line: {line}"));
        let (port, token) = rest.split_once("/#").unwrap();
        Server {
            child,
            port: port.parse().unwrap(),
            token: token.to_owned(),
        }
    }

    /// Send one request; `headers` come on top of a correct `Host`.
    fn send(&self, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Reply {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut request = format!("{method} {path} HTTP/1.1\r\n");
        if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
            request.push_str(&format!("Host: 127.0.0.1:{}\r\n", self.port));
        }
        for (k, v) in headers {
            request.push_str(&format!("{k}: {v}\r\n"));
        }
        request.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
        stream.write_all(request.as_bytes()).unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).unwrap();
        let (head, body) = raw.split_once("\r\n\r\n").unwrap();
        Reply {
            status: head[9..12].parse().unwrap(),
            head: head.to_ascii_lowercase(),
            body: body.to_owned(),
        }
    }

    fn api(&self, method: &str, path: &str, body: Value) -> Reply {
        let body = if body.is_null() {
            String::new()
        } else {
            body.to_string()
        };
        self.send(method, path, &[("X-TcTidy-Token", &self.token)], &body)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Reply {
    status: u16,
    head: String,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|e| panic!("{e}: {}", self.body))
    }
}

/// The top-level blocks of a style sheet as `(prelude, contents)`: a selector
/// list or an at-rule with its spaces collapsed, then what its braces hold.
fn css_blocks(css: &str) -> Vec<(String, String)> {
    let mut text = String::new();
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        text.push_str(&rest[..start]);
        let end = rest[start + 2..].find("*/").expect("unclosed comment");
        rest = &rest[start + 2 + end + 2..];
    }
    text.push_str(rest);
    let (mut blocks, mut depth, mut prelude, mut open) = (Vec::new(), 0, 0, 0);
    for (i, c) in text.char_indices() {
        match c {
            '{' => {
                if depth == 0 {
                    open = i;
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let head = text[prelude..open].split_whitespace();
                    blocks.push((head.collect::<Vec<_>>().join(" "), text[open + 1..i].into()));
                    prelude = i + 1;
                }
            }
            _ => {}
        }
    }
    blocks
}

/// The declarations of the rules for exactly `selector` in `css`, spaces
/// collapsed: `"grid-row: 3"`.
fn css_declarations(css: &str, selector: &str) -> Vec<String> {
    let mut declarations = Vec::new();
    for (_, body) in css_blocks(css).into_iter().filter(|(s, _)| s == selector) {
        for declaration in body.split(';') {
            let words: Vec<_> = declaration.split_whitespace().collect();
            if !words.is_empty() {
                declarations.push(words.join(" "));
            }
        }
    }
    declarations
}

/// Fails unless a rule for exactly `selector` in `css` has `declaration`.
fn assert_rule(css: &str, selector: &str, declaration: &str) {
    let declarations = css_declarations(css, selector);
    assert!(
        declarations.iter().any(|d| d == declaration),
        "{selector} lacks `{declaration}`: {declarations:?}"
    );
}

/// The contents of the at-rule `prelude` in `css`.
fn css_at_rule(css: &str, prelude: &str) -> String {
    css_blocks(css)
        .into_iter()
        .find(|(p, _)| p == prelude)
        .unwrap_or_else(|| panic!("no `{prelude}` block"))
        .1
}

#[test]
fn serves_the_page_under_a_strict_policy() {
    let dir = scratch("page");
    let server = Server::start(&dir);

    let page = server.send("GET", "/", &[], "");
    assert_eq!(page.status, 200);
    assert!(
        page.head.contains("content-type: text/html"),
        "{}",
        page.head
    );
    assert!(
        page.head
            .contains("content-security-policy: default-src 'none'; script-src 'self';"),
        "{}",
        page.head
    );
    assert!(page.body.contains("<script src=\"/app.js\""));
    for (path, kind) in [
        ("/app.js", "text/javascript"),
        ("/app.css", "text/css"),
        ("/icon.svg", "image/svg+xml"),
    ] {
        let asset = server.send("GET", path, &[], "");
        assert_eq!(asset.status, 200, "{path}");
        assert!(asset.head.contains(kind), "{path}: {}", asset.head);
    }
    assert_eq!(server.send("GET", "/../Cargo.toml", &[], "").status, 404);
    drop(server);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_page_fills_the_window_and_fits_a_phone() {
    let dir = scratch("layout");
    let server = Server::start(&dir);
    let css = server.send("GET", "/app.css", &[], "").body;

    // The workspace sits in the row that stretches, also when the banner row
    // above it is empty and not displayed: auto-placed, it would take that
    // row and shrink to the settings shown.
    assert_rule(&css, "body", "grid-template-rows: auto auto minmax(0, 1fr)");
    assert_rule(&css, ".workspace", "grid-row: 3");

    // On a phone the app bar's actions shrink to the window, the status
    // wrapping, instead of pushing the Save button out of it.
    let narrow = css_at_rule(&css, "@media (max-width: 760px)");
    assert_rule(&narrow, ".appbar-actions", "flex-shrink: 1");
    assert_rule(&narrow, ".status", "white-space: normal");
    drop(server);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_preview_shows_each_part_typed_beside_it_formatted() {
    let dir = scratch("preview");
    let server = Server::start(&dir);
    let page = server.send("GET", "/", &[], "").body;
    let css = server.send("GET", "/app.css", &[], "").body;

    // The selected setting's sample, then the user's code: the declaration
    // above the implementation, filling the rest of the panel's height. Each
    // row puts the typed code beside the formatted code, and stacks the two
    // only when the panel is too narrow for both.
    assert_rule(
        &css,
        ".preview",
        "grid-template-rows: auto auto minmax(0, 2fr) minmax(0, 3fr)",
    );
    assert_rule(&css, ".preview", "height: calc(100% - var(--toolbar-h))");
    assert_rule(
        &css,
        ".preview-row",
        "grid-template-columns: repeat(2, minmax(0, 1fr))",
    );
    let stacked = css_at_rule(&css, "@container (max-width: 688px)");
    assert_rule(
        &stacked,
        ".preview-row",
        "grid-template-columns: minmax(0, 1fr)",
    );

    // The preview takes two thirds of the width; the group navigation only
    // comes with a wide window.
    assert_rule(
        &css,
        ".workspace",
        "grid-template-columns: minmax(480px, 1fr) minmax(0, 2fr)",
    );
    assert_rule(&css, ".nav", "display: none");
    let wide = css_at_rule(&css, "@media (min-width: 1700px)");
    assert_rule(&wide, ".nav", "display: block");

    // Neither text area wraps its lines, so they scroll level with the
    // formatted ones.
    for part in ["decl", "impl"] {
        let textarea = format!(r#"<textarea id="{part}-input" class="editor-input" wrap="off""#);
        assert!(page.contains(&textarea), "{textarea}");
    }
    drop(server);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_api_needs_the_token_the_host_and_the_origin() {
    let dir = scratch("access");
    let server = Server::start(&dir);

    assert_eq!(server.send("GET", "/api/state", &[], "").status, 401);
    let wrong = "0".repeat(server.token.len());
    assert_eq!(
        server
            .send("GET", "/api/state", &[("X-TcTidy-Token", &wrong)], "")
            .status,
        401
    );
    // DNS rebinding: a page served for another name reaching this port.
    let rebound = server.send(
        "GET",
        "/api/state",
        &[
            ("Host", "attacker.example"),
            ("X-TcTidy-Token", &server.token),
        ],
        "",
    );
    assert_eq!(rebound.status, 421);
    let foreign = server.send(
        "POST",
        "/api/save",
        &[
            ("Origin", "http://attacker.example"),
            ("X-TcTidy-Token", &server.token),
        ],
        "{}",
    );
    assert_eq!(foreign.status, 403);
    // A CORS preflight is never granted.
    assert_ne!(server.send("OPTIONS", "/api/save", &[], "").status, 200);

    let localhost = format!("localhost:{}", server.port);
    let ok = server.send(
        "GET",
        "/api/state",
        &[("Host", &localhost), ("X-TcTidy-Token", &server.token)],
        "",
    );
    assert_eq!(ok.status, 200);
    drop(server);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn state_preview_save_and_format() {
    let dir = scratch("api");
    let config = dir.join(".tctidy.toml");
    std::fs::write(&config, "# ours\nindent_style = \"tabs\" # legacy\n").unwrap();
    let server = Server::start(&dir);

    let state = server.api("GET", "/api/state", Value::Null).json();
    assert_eq!(state["exists"], true);
    assert_eq!(state["path"], config.display().to_string());
    assert!(state["problems"].as_array().unwrap().is_empty());
    let version = state["version"].as_str().unwrap().to_owned();
    let indent = &state["catalog"]["groups"][0]["settings"][0];
    assert_eq!(indent["key"], "indent_style");
    assert_eq!(indent["value"], "tabs");
    assert_eq!(indent["default"], "spaces");
    assert!(state["example"]["declaration"]
        .as_str()
        .unwrap()
        .starts_with("FUNCTION_BLOCK"));
    assert!(state["example"]["implementation"].is_string());
    assert!(state["previews"]["keyword_case"]["before"]
        .as_str()
        .unwrap()
        .starts_with("IF bStart"));

    // A preview of some keys, with an invalid value reported next to its key.
    let preview = server
        .api(
            "POST",
            "/api/preview",
            json!({
                "values": { "keyword_case": "lower", "indent_width": 99 },
                "keys": ["keyword_case"]
            }),
        )
        .json();
    let previews = preview["previews"].as_object().unwrap();
    assert_eq!(previews.len(), 1);
    assert!(previews["keyword_case"]["after"]
        .as_str()
        .unwrap()
        .starts_with("if bStart"));
    assert_eq!(preview["errors"][0]["key"], "indent_width");

    // Saving checks the version, refuses invalid values, and keeps comments.
    let values = json!({ "indent_style": "tabs", "keyword_case": "lower" });
    let stale = server.api(
        "POST",
        "/api/save",
        json!({ "values": values, "version": "stale" }),
    );
    assert_eq!(stale.status, 409);
    let invalid = server.api(
        "POST",
        "/api/save",
        json!({ "values": { "indent_width": 0 }, "version": version }),
    );
    assert_eq!(invalid.status, 422);
    assert_eq!(invalid.json()["errors"][0]["key"], "indent_width");
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "# ours\nindent_style = \"tabs\" # legacy\n"
    );
    let saved = server.api(
        "POST",
        "/api/save",
        json!({ "values": values, "version": version }),
    );
    assert_eq!(saved.status, 200, "{}", saved.body);
    assert!(saved.json().get("warning").is_none(), "{}", saved.body);
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "# ours\nindent_style = \"tabs\" # legacy\nkeyword_case = \"lower\"\n"
    );
    let version = saved.json()["version"].as_str().unwrap().to_owned();
    let ping = server.api("POST", "/api/ping", Value::Null).json();
    assert_eq!(ping["version"], version);
    // Force overwrites a file changed on disk.
    std::fs::write(&config, "line_width = 100\n").unwrap();
    let forced = server.api(
        "POST",
        "/api/save",
        json!({ "values": values, "version": version, "force": true }),
    );
    assert_eq!(forced.status, 200);
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "indent_style = \"tabs\"\nkeyword_case = \"lower\"\n"
    );

    // Formatting the user's own code with the page's values.
    let formatted = server
        .api(
            "POST",
            "/api/format",
            json!({ "values": values, "source": "IF a THEN\nb:=1;\nEND_IF" }),
        )
        .json();
    assert_eq!(formatted["output"], "if a then\n\tb := 1;\nend_if");
    let refused = server
        .api(
            "POST",
            "/api/format",
            json!({ "values": values, "source": "VAR\ncafé : INT;\nEND_VAR", "declaration": true }),
        )
        .json();
    assert!(refused["error"].is_string(), "{refused}");
    drop(server);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn edits_the_nearest_config_or_creates_one_in_the_directory() {
    let dir = scratch("nearest");
    let nested = dir.join("Machine");
    std::fs::create_dir_all(&nested).unwrap();

    let server = Server::start(&nested);
    let state = server.api("GET", "/api/state", Value::Null).json();
    assert_eq!(state["exists"], false);
    assert_eq!(state["version"], "");
    let created = nested.join(".tctidy.toml");
    assert_eq!(state["path"], created.display().to_string());
    let saved = server.api(
        "POST",
        "/api/save",
        json!({ "values": { "blank_lines_max": 1 }, "version": "" }),
    );
    assert_eq!(saved.status, 200);
    assert!(std::fs::read_to_string(&created)
        .unwrap()
        .ends_with("# Blank lines\nblank_lines_max = 1\n"));
    drop(server);

    // An existing file in a parent directory is the one that applies.
    std::fs::remove_file(&created).unwrap();
    std::fs::write(dir.join(".tctidy.toml"), "keyword_case = 3\n").unwrap();
    let server = Server::start(&nested);
    let state = server.api("GET", "/api/state", Value::Null).json();
    assert_eq!(
        state["path"],
        dir.join(".tctidy.toml").display().to_string()
    );
    assert!(state["problems"][0]
        .as_str()
        .unwrap()
        .starts_with("1:16: invalid value for `keyword_case`"));
    drop(server);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Say that a test could not run here. libtest has no skip status, so this
/// writes to the real stderr (output capture only covers `eprintln!`), where
/// `cargo test` shows it even for a test that returns early.
fn skip(test: &str, why: &str) {
    let _ = writeln!(std::io::stderr(), "SKIPPED {test}: {why}");
}

/// Create a symbolic link at `link` whose literal target is `target`; false
/// when the platform refuses it (Windows without the symbolic link privilege).
fn symlink(target: &Path, link: &Path) -> bool {
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
fn saving_a_linked_config_updates_the_file_it_points_to() {
    let dir = scratch("linked");
    let shared = dir.join("shared");
    let project = dir.join("project");
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    let target = shared.join("tctidy.toml");
    std::fs::write(&target, "indent_style = \"tabs\"\n").unwrap();
    let link = project.join(".tctidy.toml");
    let literal = Path::new("..").join("shared").join("tctidy.toml");
    if !symlink(&literal, &link) {
        skip(
            "saving_a_linked_config_updates_the_file_it_points_to",
            "no symlink privilege",
        );
        std::fs::remove_dir_all(&dir).unwrap();
        return;
    }
    let server = Server::start(&project);
    let version = server.api("GET", "/api/state", Value::Null).json()["version"]
        .as_str()
        .unwrap()
        .to_owned();

    let saved = server.api(
        "POST",
        "/api/save",
        json!({ "values": { "indent_style": "tabs", "keyword_case": "lower" }, "version": version }),
    );

    assert_eq!(saved.status, 200, "{}", saved.body);
    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(std::fs::read_link(&link).unwrap(), literal);
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "indent_style = \"tabs\"\nkeyword_case = \"lower\"\n"
    );
    drop(server);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[test]
fn a_save_that_cannot_be_synced_is_reported_as_not_durable() {
    use std::os::unix::fs::PermissionsExt;

    // Write and search permission let the save replace the file, but without
    // read permission the directory cannot be opened to sync the rename.
    let dir = scratch("nosync");
    let config = dir.join(".tctidy.toml");
    std::fs::write(&config, "indent_style = \"tabs\"\n").unwrap();
    let server = Server::start(&dir);
    let version = server.api("GET", "/api/state", Value::Null).json()["version"]
        .as_str()
        .unwrap()
        .to_owned();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o300)).unwrap();
    if std::fs::File::open(&dir).is_ok() {
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        skip(
            "a_save_that_cannot_be_synced_is_reported_as_not_durable",
            "running with privileges that ignore directory permissions",
        );
        drop(server);
        std::fs::remove_dir_all(&dir).unwrap();
        return;
    }

    let saved = server.api(
        "POST",
        "/api/save",
        json!({ "values": { "indent_style": "tabs", "keyword_case": "lower" }, "version": version }),
    );
    let content = std::fs::read_to_string(&config).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

    // The file was replaced, so the page must take the new version, and be
    // told that the change may not survive a power loss.
    assert_eq!(saved.status, 200, "{}", saved.body);
    let body = saved.json();
    let warning = body["warning"].as_str().unwrap_or_else(|| panic!("{body}"));
    assert!(warning.contains("power loss"), "{warning}");
    assert!(
        warning.contains(&format!("cannot sync directory {}", dir.display())),
        "{warning}"
    );
    assert_eq!(
        content,
        "indent_style = \"tabs\"\nkeyword_case = \"lower\"\n"
    );
    drop(server);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn close_stops_the_server() {
    let dir = scratch("close");
    let mut server = Server::start(&dir);
    assert_eq!(server.api("POST", "/api/close", Value::Null).status, 200);
    let status = server.child.wait().unwrap();
    assert!(status.success());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_missing_directory_is_a_usage_error() {
    let dir = scratch("missing").join("nope");
    let out = Command::new(BIN)
        .arg("--settings")
        .arg(&dir)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("is not a directory"));
    assert!(out.stdout.is_empty());
}
