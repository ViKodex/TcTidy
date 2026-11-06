//! `tctidy --settings`: a settings page for `.tctidy.toml`, served on loopback.
//!
//! The server binds `127.0.0.1` on a free port and prints the page's URL on
//! stdout; the URL's fragment carries a one-time token that never reaches the
//! server logs or a `Referer`. The page sends it back in an `X-TcTidy-Token`
//! header on every API call, and the server also checks `Host` (against DNS
//! rebinding) and `Origin`. The custom header forces a CORS preflight that the
//! server never grants, so another site open in the browser cannot call it.
//! The page, its script and its style are fixed assets under a strict
//! Content-Security-Policy; the page can only choose setting values, never a
//! path: the file it edits is fixed when the server starts.
//!
//! The server stops after [`IDLE_TIMEOUT`] without an API call (the open page
//! pings it), or when asked to.

use std::collections::hash_map::{DefaultHasher, RandomState};
use std::collections::BTreeMap;
use std::hash::{BuildHasher, Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tctidy::settings::{self, Preview, SettingError, SettingValue};
use tctidy::{
    find_config, format_st, parse_config, update_config, FormatOptions, StSection, StyleConfig,
    CONFIG_FILE_NAME,
};

use crate::io_atomic::{write_atomic, WriteError};

/// How long the server waits for an API call before exiting. The open page
/// pings every 30 seconds (once a minute at worst in a background tab).
const IDLE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;
/// Connections served at once; more are closed straight away.
const MAX_CONNECTIONS: usize = 32;

struct Asset {
    path: &'static str,
    content_type: &'static str,
    body: &'static str,
}

const ASSETS: &[Asset] = &[
    Asset {
        path: "/",
        content_type: "text/html; charset=utf-8",
        body: include_str!("settings_page/index.html"),
    },
    Asset {
        path: "/app.css",
        content_type: "text/css; charset=utf-8",
        body: include_str!("settings_page/app.css"),
    },
    Asset {
        path: "/app.js",
        content_type: "text/javascript; charset=utf-8",
        body: include_str!("settings_page/app.js"),
    },
    Asset {
        path: "/icon.svg",
        content_type: "image/svg+xml",
        body: include_str!("settings_page/icon.svg"),
    },
];

/// Run the settings server for the configuration that applies to `dir`: the
/// nearest existing `.tctidy.toml` from `dir` up, else a new one in `dir`.
pub fn run(dir: &str) -> ExitCode {
    let root = std::path::absolute(dir).unwrap_or_else(|_| PathBuf::from(dir));
    if !root.is_dir() {
        eprintln!("tctidy: --settings: {} is not a directory", root.display());
        return ExitCode::from(2);
    }
    let listener = match TcpListener::bind(("127.0.0.1", 0)) {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("tctidy: --settings: cannot listen on 127.0.0.1: {e}");
            return ExitCode::from(1);
        }
    };
    let port = match listener.local_addr() {
        Ok(addr) => addr.port(),
        Err(e) => {
            eprintln!("tctidy: --settings: {e}");
            return ExitCode::from(1);
        }
    };
    let server = Arc::new(Server {
        config_path: find_config(&root).unwrap_or_else(|| root.join(CONFIG_FILE_NAME)),
        token: random_token(),
        port,
        save_lock: Mutex::new(()),
        last_request: AtomicU64::new(now_secs()),
        connections: AtomicUsize::new(0),
    });

    println!("http://127.0.0.1:{port}/#{}", server.token);
    let _ = std::io::stdout().flush();
    eprintln!(
        "tctidy: settings page for {} (open the URL above; it stops 10 minutes after the page is closed)",
        server.config_path.display()
    );

    let watchdog = Arc::clone(&server);
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(5));
        let idle = now_secs().saturating_sub(watchdog.last_request.load(Ordering::Relaxed));
        if idle >= IDLE_TIMEOUT.as_secs() {
            std::process::exit(0);
        }
    });

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if server.connections.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
            server.connections.fetch_sub(1, Ordering::AcqRel);
            continue;
        }
        let server = Arc::clone(&server);
        std::thread::spawn(move || {
            server.handle(stream);
            server.connections.fetch_sub(1, Ordering::AcqRel);
        });
    }
    ExitCode::SUCCESS
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// 128 unpredictable bits as hex. Each thread seeds `RandomState` from the OS
/// random source, so two fresh threads give two independent keys without an
/// extra dependency.
fn random_token() -> String {
    (0..2)
        .map(|_| {
            std::thread::spawn(|| {
                let mut h = RandomState::new().build_hasher();
                h.write_u128(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_or(0, |d| d.as_nanos()),
                );
                format!("{:016x}", h.finish())
            })
            .join()
            .unwrap_or_default()
        })
        .collect()
}

/// A fingerprint of the file's content, so a save can tell that the file
/// changed on disk since the page read it. Empty when there is no file.
fn version_of(content: Option<&[u8]>) -> String {
    content.map_or_else(String::new, |bytes| {
        let mut h = DefaultHasher::new();
        bytes.hash(&mut h);
        format!("{:016x}", h.finish())
    })
}

struct Server {
    config_path: PathBuf,
    token: String,
    port: u16,
    /// Serializes saves, so two pages cannot interleave read-check-write.
    save_lock: Mutex<()>,
    last_request: AtomicU64,
    connections: AtomicUsize,
}

struct Request {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

struct Response {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl Response {
    fn json(status: u16, value: &impl Serialize) -> Self {
        Response {
            status,
            content_type: "application/json",
            body: serde_json::to_vec(value).unwrap_or_default(),
        }
    }

    fn error(status: u16, message: &str) -> Self {
        Response::json(status, &serde_json::json!({ "error": message }))
    }
}

/// What the page shows on load.
#[derive(Serialize)]
struct State {
    path: String,
    exists: bool,
    version: String,
    /// Problems in the existing file; the page starts from the defaults then.
    problems: Vec<String>,
    catalog: settings::Catalog,
    previews: BTreeMap<&'static str, Preview>,
    /// The code the page formats until the user changes it.
    example: settings::Example,
    tctidy_version: &'static str,
}

/// The values the page shows, and optionally which previews it wants (all
/// when absent).
#[derive(Deserialize)]
struct PreviewRequest {
    values: BTreeMap<String, SettingValue>,
    #[serde(default)]
    keys: Option<Vec<String>>,
}

#[derive(Serialize)]
struct PreviewResponse {
    previews: BTreeMap<String, Preview>,
    errors: Vec<SettingError>,
}

#[derive(Deserialize)]
struct SaveRequest {
    values: BTreeMap<String, SettingValue>,
    /// The file version the page started from; a different one on disk
    /// refuses the save unless `force` is set.
    version: String,
    #[serde(default)]
    force: bool,
}

#[derive(Serialize)]
struct SaveResponse {
    path: String,
    version: String,
    /// Set when the file was saved but the save may not survive a power loss.
    #[serde(skip_serializing_if = "Option::is_none")]
    warning: Option<String>,
}

/// Format some ST of the user's own with the values the page shows.
#[derive(Deserialize)]
struct FormatRequest {
    values: BTreeMap<String, SettingValue>,
    source: String,
    #[serde(default)]
    declaration: bool,
}

impl Server {
    fn handle(&self, mut stream: TcpStream) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
        let (response, stop) = match read_request(&mut stream) {
            Ok(request) => self.route(&request),
            Err(status) => (Response::error(status, "bad request"), false),
        };
        self.write_response(&mut stream, &response);
        if stop {
            close_gracefully(stream);
            std::process::exit(0);
        }
    }

    /// The response, and whether the server should stop once it is sent.
    fn route(&self, request: &Request) -> (Response, bool) {
        if !self.host_allowed(request) {
            return (Response::error(421, "unexpected host"), false);
        }
        if let Some(origin) = request.headers.get("origin") {
            if !self.origin_allowed(origin) {
                return (Response::error(403, "unexpected origin"), false);
            }
        }
        let path = request.path.split(['?', '#']).next().unwrap_or("");
        if !path.starts_with("/api/") {
            return match ASSETS.iter().find(|a| a.path == path) {
                Some(asset) if request.method == "GET" => (
                    Response {
                        status: 200,
                        content_type: asset.content_type,
                        body: asset.body.as_bytes().to_vec(),
                    },
                    false,
                ),
                Some(_) => (Response::error(405, "method not allowed"), false),
                None => (Response::error(404, "not found"), false),
            };
        }
        if !self.token_ok(request) {
            return (Response::error(401, "missing or wrong token"), false);
        }
        // Only the page keeps the server alive, not whoever else knocks.
        self.last_request.store(now_secs(), Ordering::Relaxed);
        let response = match (request.method.as_str(), path) {
            ("GET", "/api/state") => self.state(),
            ("POST", "/api/preview") => self.preview(&request.body),
            ("POST", "/api/save") => self.save(&request.body),
            ("POST", "/api/format") => self.format(&request.body),
            ("POST", "/api/ping") => self.ping(),
            ("POST", "/api/close") => return (Response::json(200, &serde_json::json!({})), true),
            _ => Response::error(405, "method not allowed"),
        };
        (response, false)
    }

    fn host_allowed(&self, request: &Request) -> bool {
        request.headers.get("host").is_some_and(|host| {
            *host == format!("127.0.0.1:{}", self.port)
                || *host == format!("localhost:{}", self.port)
        })
    }

    fn origin_allowed(&self, origin: &str) -> bool {
        origin == format!("http://127.0.0.1:{}", self.port)
            || origin == format!("http://localhost:{}", self.port)
    }

    fn token_ok(&self, request: &Request) -> bool {
        let Some(sent) = request.headers.get("x-tctidy-token") else {
            return false;
        };
        // Compare without an early exit, so timing does not leak a prefix.
        sent.len() == self.token.len()
            && sent
                .bytes()
                .zip(self.token.bytes())
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                == 0
    }

    fn read_file(&self) -> Option<Vec<u8>> {
        std::fs::read(&self.config_path).ok()
    }

    fn state(&self) -> Response {
        let content = self.read_file();
        let (style, problems) = match content.as_deref().map(parse) {
            None => (StyleConfig::default(), Vec::new()),
            Some(Ok(style)) => (style, Vec::new()),
            Some(Err(problems)) => (StyleConfig::default(), problems),
        };
        Response::json(
            200,
            &State {
                path: display(&self.config_path),
                exists: content.is_some(),
                version: version_of(content.as_deref()),
                problems,
                catalog: settings::catalog(&style),
                previews: settings::previews(&style),
                example: settings::EXAMPLE,
                tctidy_version: env!("CARGO_PKG_VERSION"),
            },
        )
    }

    fn ping(&self) -> Response {
        Response::json(
            200,
            &serde_json::json!({ "version": version_of(self.read_file().as_deref()) }),
        )
    }

    fn preview(&self, body: &[u8]) -> Response {
        let request: PreviewRequest = match serde_json::from_slice(body) {
            Ok(request) => request,
            Err(e) => return Response::error(400, &format!("invalid request: {e}")),
        };
        // Preview whatever is valid; the page shows the rest next to its setting.
        let mut style = StyleConfig::default();
        let errors = settings::apply_values(
            &mut style,
            request.values.iter().map(|(k, v)| (k.as_str(), v)),
        );
        let previews = match request.keys {
            None => settings::previews(&style)
                .into_iter()
                .map(|(k, p)| (k.to_owned(), p))
                .collect(),
            Some(keys) => keys
                .into_iter()
                .filter_map(|k| Some((k.clone(), settings::preview(&k, &style)?.ok()?)))
                .collect(),
        };
        Response::json(200, &PreviewResponse { previews, errors })
    }

    fn save(&self, body: &[u8]) -> Response {
        let request: SaveRequest = match serde_json::from_slice(body) {
            Ok(request) => request,
            Err(e) => return Response::error(400, &format!("invalid request: {e}")),
        };
        let style = match settings::style_from_values(
            request.values.iter().map(|(k, v)| (k.as_str(), v)),
        ) {
            Ok(style) => style,
            Err(errors) => return Response::json(422, &serde_json::json!({ "errors": errors })),
        };

        let _guard = self.save_lock.lock().unwrap_or_else(|e| e.into_inner());
        let content = self.read_file();
        if !request.force && version_of(content.as_deref()) != request.version {
            return Response::error(
                409,
                &format!("{} changed on disk", display(&self.config_path)),
            );
        }
        // Keep what a person wrote in a valid file; an unreadable or invalid
        // one is replaced, as the page warned.
        let text = content
            .as_deref()
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .and_then(|src| update_config(src, &style).ok())
            .unwrap_or_else(|| tctidy::config::render_config(&style, false));
        let mut warning = None;
        if content.as_deref() != Some(text.as_bytes()) {
            match write_atomic(&self.config_path, text.as_bytes()) {
                Ok(()) => {}
                Err(WriteError::NotWritten(e)) => {
                    return Response::error(
                        500,
                        &format!("cannot write {}: {e}", display(&self.config_path)),
                    );
                }
                // The new file is in place, so the page takes its version, but
                // it must not present the save as safe from a power loss.
                Err(e @ WriteError::NotDurable { .. }) => {
                    warning = Some(format!(
                        "Saved {}, but the change may not survive a power loss: {e}",
                        display(&self.config_path)
                    ));
                }
            }
        }
        Response::json(
            200,
            &SaveResponse {
                path: display(&self.config_path),
                version: version_of(Some(text.as_bytes())),
                warning,
            },
        )
    }

    fn format(&self, body: &[u8]) -> Response {
        let request: FormatRequest = match serde_json::from_slice(body) {
            Ok(request) => request,
            Err(e) => return Response::error(400, &format!("invalid request: {e}")),
        };
        let mut style = StyleConfig::default();
        settings::apply_values(
            &mut style,
            request.values.iter().map(|(k, v)| (k.as_str(), v)),
        );
        let section = if request.declaration {
            StSection::Declaration
        } else {
            StSection::Implementation
        };
        match format_st(&request.source, section, &FormatOptions { style }) {
            Ok(output) => Response::json(200, &serde_json::json!({ "output": output })),
            Err(e) => Response::json(200, &serde_json::json!({ "error": e.to_string() })),
        }
    }

    fn write_response(&self, stream: &mut TcpStream, response: &Response) {
        let reason = match response.status {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            409 => "Conflict",
            413 => "Content Too Large",
            421 => "Misdirected Request",
            422 => "Unprocessable Content",
            _ => "Internal Server Error",
        };
        let head = format!(
            "HTTP/1.1 {} {reason}\r\n\
             Content-Type: {}\r\n\
             Content-Length: {}\r\n\
             Cache-Control: no-store\r\n\
             X-Content-Type-Options: nosniff\r\n\
             X-Frame-Options: DENY\r\n\
             Referrer-Policy: no-referrer\r\n\
             Cross-Origin-Opener-Policy: same-origin\r\n\
             Cross-Origin-Resource-Policy: same-origin\r\n\
             Content-Security-Policy: default-src 'none'; script-src 'self'; \
             style-src 'self'; img-src 'self'; connect-src 'self'; base-uri 'none'; \
             form-action 'none'; frame-ancestors 'none'\r\n\
             Connection: close\r\n\r\n",
            response.status,
            response.content_type,
            response.body.len()
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(&response.body);
        let _ = stream.flush();
    }
}

/// Close `stream` so the client reads the whole response before the process exits:
/// Windows resets a socket still open at exit, and the client can then lose the
/// response it was reading. Send FIN after the response, then wait (briefly) for the
/// client to close its side.
fn close_gracefully(mut stream: TcpStream) {
    let _ = stream.shutdown(Shutdown::Write);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let mut sink = [0u8; 512];
    while matches!(stream.read(&mut sink), Ok(n) if n > 0) {}
}

/// The style a configuration file holds, or its problems as `line:col: message`.
fn parse(bytes: &[u8]) -> Result<StyleConfig, Vec<String>> {
    let src = std::str::from_utf8(bytes).map_err(|_| vec!["not valid UTF-8".to_owned()])?;
    parse_config(src).map_err(|e| {
        e.diagnostics
            .iter()
            .map(|d| match (d.line, d.column) {
                (Some(line), Some(col)) => format!("{line}:{col}: {}", d.message),
                _ => d.message.clone(),
            })
            .collect()
    })
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

/// Read one HTTP/1.1 request, or the status to refuse it with.
fn read_request(stream: &mut TcpStream) -> Result<Request, u16> {
    let mut reader = BufReader::new(stream);
    let mut head = Vec::new();
    loop {
        let before = head.len();
        let n = reader
            .by_ref()
            .take((MAX_HEADER_BYTES + 1 - head.len()) as u64)
            .read_until(b'\n', &mut head)
            .map_err(|_| 400u16)?;
        if n == 0 || head.len() > MAX_HEADER_BYTES {
            return Err(400);
        }
        if head[before..] == *b"\r\n" || head[before..] == *b"\n" {
            break;
        }
    }
    let head = String::from_utf8(head).map_err(|_| 400u16)?;
    let mut lines = head.lines();
    let mut start = lines.next().unwrap_or("").split(' ');
    let (Some(method), Some(path)) = (start.next(), start.next()) else {
        return Err(400);
    };
    let headers: BTreeMap<String, String> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    if headers.contains_key("transfer-encoding") {
        return Err(400);
    }
    let length: usize = match headers.get("content-length") {
        Some(v) => v.parse().map_err(|_| 400u16)?,
        None => 0,
    };
    if length > MAX_BODY_BYTES {
        return Err(413);
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).map_err(|_| 400u16)?;
    Ok(Request {
        method: method.to_owned(),
        path: path.to_owned(),
        headers,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_long_and_distinct() {
        let (a, b) = (random_token(), random_token());
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn versions_tell_contents_apart() {
        assert_eq!(version_of(None), "");
        assert_ne!(version_of(Some(b"")), "");
        assert_ne!(version_of(Some(b"a = 1")), version_of(Some(b"a = 2")));
        assert_eq!(version_of(Some(b"a = 1")), version_of(Some(b"a = 1")));
    }
}
