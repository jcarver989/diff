use diff_core::{DiffDocument, ReviewSubmission};
use rand::RngCore;
use std::{
    env,
    fmt::Write as _,
    fs,
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
};
use thiserror::Error;

const MAX_HEADERS: usize = 64 * 1024;
const MAX_SUBMISSION: usize = 8 * 1024 * 1024;
const BUILTIN_JAVASCRIPT: &[u8] = include_bytes!("../assets/app.js");
const BUILTIN_WASM_GZIP: &[u8] = include_bytes!("../assets/app.wasm.gz");
const BUILTIN_INDEX: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="color-scheme" content="dark">
  <title>Diff Review</title>
  <style>html,body{width:100%;height:100%;margin:0;overflow:hidden;background:#101418}canvas{display:block;width:100%;height:100%}</style>
  <script type="module">
    import init, * as bindings from "/app.js";
    const wasm = await init({ module_or_path: "/app.wasm" });
    window.wasmBindings = bindings;
    dispatchEvent(new CustomEvent("TrunkApplicationStarted", { detail: { wasm } }));
  </script>
</head>
<body></body>
</html>"#;

pub struct Options {
    pub port: u16,
    pub no_open: bool,
    pub assets: Option<PathBuf>,
}

pub fn run(
    document: &DiffDocument,
    options: &Options,
) -> Result<Option<ReviewSubmission>, WebError> {
    let assets = resolve_assets(options.assets.as_deref())?;
    let index = match &assets {
        Some(assets) => {
            fs::read_to_string(assets.join("index.html")).map_err(|source| WebError::Asset {
                path: assets.join("index.html"),
                source,
            })?
        }
        None => BUILTIN_INDEX.to_owned(),
    };
    let host = WebHost { assets, index };
    serve(document, options.port, Some(&host), |url| {
        eprintln!("Review ready at {url}");
        if options.no_open {
            Ok(())
        } else {
            open::that(url).map_err(|error| error.to_string())
        }
    })
}

pub fn run_tui_session(
    document: &DiffDocument,
    port: u16,
    launch: impl FnOnce(&str) -> Result<(), String>,
) -> Result<Option<ReviewSubmission>, WebError> {
    serve(document, port, None, launch)
}

fn serve(
    document: &DiffDocument,
    port: u16,
    host: Option<&WebHost>,
    launch: impl FnOnce(&str) -> Result<(), String>,
) -> Result<Option<ReviewSubmission>, WebError> {
    let token = session_token();
    let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))?;
    let address = listener.local_addr()?;
    let url = format!("http://127.0.0.1:{}/session/{token}/", address.port());
    launch(&url).map_err(WebError::Launch)?;

    let document_json = serde_json::to_vec(document)?;
    for connection in listener.incoming() {
        let mut stream = connection?;
        match handle_connection(&mut stream, host, &token, &document_json) {
            Ok(ConnectionOutcome::Continue) => {}
            Ok(ConnectionOutcome::Submitted(submission)) => return Ok(Some(submission)),
            Ok(ConnectionOutcome::Cancelled) => return Ok(None),
            Err(error) => {
                eprintln!("review session request failed: {error}");
                let _ = respond(
                    &mut stream,
                    400,
                    "text/plain; charset=utf-8",
                    b"bad request",
                );
            }
        }
    }
    Err(WebError::ServerStopped)
}

struct WebHost {
    assets: Option<PathBuf>,
    index: String,
}

fn handle_connection(
    stream: &mut TcpStream,
    host: Option<&WebHost>,
    token: &str,
    document: &[u8],
) -> Result<ConnectionOutcome, WebError> {
    let request = Request::read(stream)?;
    let session_root = format!("/session/{token}/");
    let document_path = format!("/api/{token}/document");
    let submit_path = format!("/api/{token}/submit");
    let cancel_path = format!("/api/{token}/cancel");

    if request.method == "GET"
        && request.path == session_root
        && let Some(host) = host
    {
        let html = inject_bridge(&host.index, token);
        respond(stream, 200, "text/html; charset=utf-8", html.as_bytes())?;
        return Ok(ConnectionOutcome::Continue);
    }
    if request.method == "GET" && request.path == document_path {
        respond(stream, 200, "application/json", document)?;
        return Ok(ConnectionOutcome::Continue);
    }
    if request.method == "POST" && request.path == submit_path {
        let submission = serde_json::from_slice(&request.body)?;
        respond(stream, 204, "text/plain", b"")?;
        return Ok(ConnectionOutcome::Submitted(submission));
    }
    if request.method == "POST" && request.path == cancel_path {
        respond(stream, 204, "text/plain", b"")?;
        return Ok(ConnectionOutcome::Cancelled);
    }
    if request.method == "GET" {
        let relative = request.path.trim_start_matches('/');
        if !relative.is_empty()
            && !relative.contains('/')
            && !relative.contains("..")
            && let Some(host) = host
        {
            if let Some(assets) = &host.assets {
                let path = assets.join(relative);
                if path.is_file() {
                    let body =
                        fs::read(&path).map_err(|source| WebError::Asset { path, source })?;
                    respond(stream, 200, content_type(relative), &body)?;
                    return Ok(ConnectionOutcome::Continue);
                }
            } else if relative == "app.js" {
                respond(stream, 200, content_type(relative), BUILTIN_JAVASCRIPT)?;
                return Ok(ConnectionOutcome::Continue);
            } else if relative == "app.wasm" {
                respond_gzip(stream, 200, content_type(relative), BUILTIN_WASM_GZIP)?;
                return Ok(ConnectionOutcome::Continue);
            }
        }
    }

    respond(stream, 404, "text/plain; charset=utf-8", b"not found")?;
    Ok(ConnectionOutcome::Continue)
}

fn inject_bridge(index: &str, token: &str) -> String {
    let bridge = format!(
        r#"<script>
const reviewApi = "/api/{token}";
const loadReviewDocument = () => fetch(reviewApi + "/document")
  .then(response => {{ if (!response.ok) throw new Error("failed to load diff"); return response.text(); }})
  .then(documentJson => document.dispatchEvent(new CustomEvent("diff-review-set-document", {{ detail: documentJson }})))
  .catch(error => console.error(error));
if (window.wasmBindings) {{
  loadReviewDocument();
}} else {{
  window.addEventListener("TrunkApplicationStarted", loadReviewDocument, {{ once: true }});
}}
document.addEventListener("diff-review-submit", async event => {{
  const response = await fetch(reviewApi + "/submit", {{
    method: "POST",
    headers: {{ "Content-Type": "application/json" }},
    body: event.detail,
  }});
  if (!response.ok) {{ console.error("failed to submit review"); return; }}
  document.body.innerHTML = '<main style="display:grid;place-items:center;height:100%;color:#d6d9dc;font:18px sans-serif">Feedback sent. You can close this tab.</main>';
}});
document.addEventListener("diff-review-cancel", async () => {{
  await fetch(reviewApi + "/cancel", {{ method: "POST" }});
  document.body.innerHTML = '<main style="display:grid;place-items:center;height:100%;color:#d6d9dc;font:18px sans-serif">Review cancelled. You can close this tab.</main>';
}});
</script>"#
    );
    index.replacen("</body>", &format!("{bridge}</body>"), 1)
}

fn resolve_assets(explicit: Option<&Path>) -> Result<Option<PathBuf>, WebError> {
    let requested = explicit
        .map(Path::to_path_buf)
        .or_else(|| env::var_os("CLANKERDIFF_WEB_ASSETS").map(PathBuf::from));
    match requested {
        Some(path) if path.join("index.html").is_file() => Ok(Some(path)),
        Some(_) => Err(WebError::MissingAssets),
        None => Ok(None),
    }
}

fn session_token() -> String {
    let mut bytes = [0_u8; 24];
    rand::rng().fill_bytes(&mut bytes);
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(token, "{byte:02x}");
    }
    token
}

fn content_type(path: &str) -> &'static str {
    let extension = Path::new(path).extension().and_then(|value| value.to_str());
    if extension.is_some_and(|value| value.eq_ignore_ascii_case("js")) {
        "text/javascript; charset=utf-8"
    } else if extension.is_some_and(|value| value.eq_ignore_ascii_case("wasm")) {
        "application/wasm"
    } else {
        "application/octet-stream"
    }
}

fn respond(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) -> io::Result<()> {
    respond_with_encoding(stream, status, content_type, body, None)
}

fn respond_gzip(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> io::Result<()> {
    respond_with_encoding(stream, status, content_type, body, Some("gzip"))
}

fn respond_with_encoding(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
    encoding: Option<&str>,
) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store\r\n",
        body.len()
    )?;
    if let Some(encoding) = encoding {
        write!(stream, "Content-Encoding: {encoding}\r\n")?;
    }
    write!(stream, "\r\n")?;
    stream.write_all(body)?;
    stream.flush()
}

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
}

impl Request {
    fn read(stream: &mut TcpStream) -> Result<Self, WebError> {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 8192];
        let header_end = loop {
            let count = stream.read(&mut chunk)?;
            if count == 0 {
                return Err(WebError::MalformedRequest);
            }
            bytes.extend_from_slice(&chunk[..count]);
            if let Some(position) = find_bytes(&bytes, b"\r\n\r\n") {
                break position + 4;
            }
            if bytes.len() > MAX_HEADERS {
                return Err(WebError::RequestTooLarge);
            }
        };

        let headers = std::str::from_utf8(&bytes[..header_end])?;
        let mut lines = headers.split("\r\n");
        let mut request_line = lines
            .next()
            .ok_or(WebError::MalformedRequest)?
            .split_whitespace();
        let method = request_line
            .next()
            .ok_or(WebError::MalformedRequest)?
            .to_owned();
        let path = request_line
            .next()
            .ok_or(WebError::MalformedRequest)?
            .to_owned();
        let content_length = lines
            .filter_map(|line| line.split_once(':'))
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .map_or(Ok(0), |(_, value)| value.trim().parse::<usize>())
            .map_err(|_| WebError::MalformedRequest)?;
        if content_length > MAX_SUBMISSION {
            return Err(WebError::RequestTooLarge);
        }
        while bytes.len() < header_end + content_length {
            let count = stream.read(&mut chunk)?;
            if count == 0 {
                return Err(WebError::MalformedRequest);
            }
            bytes.extend_from_slice(&chunk[..count]);
        }
        let body = bytes[header_end..header_end + content_length].to_vec();
        Ok(Self { method, path, body })
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

enum ConnectionOutcome {
    Continue,
    Submitted(ReviewSubmission),
    Cancelled,
}

#[derive(Debug, Error)]
pub enum WebError {
    #[error("the requested web asset directory does not contain index.html")]
    MissingAssets,
    #[error("could not read web asset `{path}`: {source}")]
    Asset { path: PathBuf, source: io::Error },
    #[error("could not launch the review client: {0}")]
    Launch(String),
    #[error("the web review server stopped before receiving feedback")]
    ServerStopped,
    #[error("malformed HTTP request")]
    MalformedRequest,
    #[error("HTTP request is too large")]
    RequestTooLarge,
    #[error("HTTP request was not UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[error("invalid review submission: {0}")]
    Submission(#[from] serde_json::Error),
    #[error("web server I/O failed: {0}")]
    Io(#[from] io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_contains_token_and_review_events() {
        let html = inject_bridge("<body></body>", "secret");
        assert!(html.contains("/api/secret"));
        assert!(html.contains("diff-review-submit"));
        assert!(html.contains("diff-review-set-document"));
        assert!(html.contains("diff-review-cancel"));
    }

    #[test]
    fn only_serves_known_static_content_types() {
        assert_eq!(content_type("app.js"), "text/javascript; charset=utf-8");
        assert_eq!(content_type("app.wasm"), "application/wasm");
    }
}
