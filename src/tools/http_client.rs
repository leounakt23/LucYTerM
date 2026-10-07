//! HTTP command construction for local or remote execution, plus a
//! dependency-free plain-HTTP/1.1 client (GET/POST, redirects, chunked
//! bodies) over TCP. HTTPS is out of scope here — use the SSH tunnel or
//! the system client for TLS.

use super::shell_quote;

/// curl one-liner for the remote path (the far host always has a shell;
/// native TCP here would only measure the controller's own network).
pub fn command(method: &str, url: &str, headers: &str, body: &str) -> String {
    let mut command = format!(
        "curl -L --fail --silent --show-error -X {}",
        shell_quote(method)
    );
    for header in headers
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        command.push_str(&format!(" -H {}", shell_quote(header)));
    }
    if !body.is_empty() {
        command.push_str(&format!(" --data {}", shell_quote(body)));
    }
    command.push_str(&format!(" {}", shell_quote(url)));
    command
}

/// Remote HTTP run: the curl one-liner executes on `session`.
pub fn run_remote(
    session: mbxt_core::SessionId,
    method: String,
    url: String,
    headers: String,
    body: String,
    cancel: super::CancelToken,
) -> super::BoxStream<'static, super::ToolEvent> {
    use super::{OutputLevel, ToolEvent};
    use futures::StreamExt as _;
    Box::pin(
        futures::stream::once(async move {
            if cancel.is_cancelled() {
                return vec![ToolEvent::Cancelled];
            }
            match super::remote_exec(session, &command(&method, &url, &headers, &body)).await {
                Ok(output) => output
                    .lines()
                    .map(|line| ToolEvent::Output {
                        line: line.to_string(),
                        level: OutputLevel::Info,
                    })
                    .chain([ToolEvent::Completed { summary: None }])
                    .collect(),
                Err(error) => vec![ToolEvent::Failed { error }],
            }
        })
        .flat_map(futures::stream::iter),
    )
}

/// Parsed `http://host[:port]/path` target. Only plain HTTP is
/// supported; anything else is a clean error, not a silent downgrade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpTarget {
    pub host: String,
    pub port: u16,
    pub path: String,
}

pub fn parse_url(url: &str) -> Result<HttpTarget, String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| "only plain http:// URLs are supported".to_string())?;
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], rest[index..].to_string()),
        None => (rest, "/".to_string()),
    };
    if authority.is_empty() {
        return Err("http URL has no host".to_string());
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => (
            host.to_string(),
            port.parse()
                .map_err(|_| format!("invalid http port in {url:?}"))?,
        ),
        _ => (authority.to_string(), 80),
    };
    if host.is_empty() {
        return Err("http URL has no host".to_string());
    }
    Ok(HttpTarget { host, port, path })
}

/// Parsed response head: status plus lowercase header map.
#[derive(Debug, Clone)]
pub struct HttpHead {
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
}

impl HttpHead {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

pub fn parse_head(raw: &str) -> Result<(HttpHead, usize), String> {
    let end = raw
        .find("\r\n\r\n")
        .ok_or_else(|| "http response head is incomplete".to_string())?;
    let mut lines = raw[..end].lines();
    let status_line = lines
        .next()
        .ok_or_else(|| "http response is empty".to_string())?;
    let mut parts = status_line.splitn(3, ' ');
    let version = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/") {
        return Err(format!("not an http response: {status_line:?}"));
    }
    let status: u16 = parts
        .next()
        .unwrap_or("")
        .parse()
        .map_err(|_| format!("bad http status: {status_line:?}"))?;
    let reason = parts.next().unwrap_or("").to_string();
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| format!("bad http header: {line:?}"))?;
        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
    }
    Ok((
        HttpHead {
            status,
            reason,
            headers,
        },
        end + 4,
    ))
}

/// Decode a chunked body starting at `body`; returns body + bytes
/// consumed so close-delimited reads know where framing ends.
pub fn decode_chunked(body: &[u8]) -> Result<(Vec<u8>, usize), String> {
    let mut out = Vec::new();
    let mut pos = 0;
    loop {
        let line_end = body[pos..]
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or_else(|| "truncated chunk size".to_string())?;
        let line = std::str::from_utf8(&body[pos..pos + line_end])
            .map_err(|_| "non-ascii chunk size".to_string())?;
        let size_text = line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| format!("bad chunk size: {size_text:?}"))?;
        pos += line_end + 2;
        if size == 0 {
            return Ok((out, pos));
        }
        if body.len() < pos + size + 2 {
            return Err("truncated chunk data".to_string());
        }
        out.extend_from_slice(&body[pos..pos + size]);
        pos += size + 2; // data + trailing CRLF
    }
}

/// Full response: head plus body framed by Content-Length, chunked
/// coding, or close-delimited read-to-EOF. Bodies are capped at 2 MiB.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub head: HttpHead,
    pub body: Vec<u8>,
}

const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

/// One request/response round trip (no redirect following).
async fn round_trip(
    target: &HttpTarget,
    method: &str,
    headers: &[(String, String)],
    body: &[u8],
    timeout_ms: u64,
) -> Result<(HttpHead, Vec<u8>), String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let address = format!("{}:{}", target.host, target.port);
    let mut stream = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms.max(1)),
        tokio::net::TcpStream::connect(&address),
    )
    .await
    .map_err(|_| format!("http connect to {address} timed out"))?
    .map_err(|error| format!("http connect to {address} failed: {error}"))?;
    let mut request = format!(
        "{} {} HTTP/1.1\r\nhost: {}\r\nconnection: close\r\n",
        method.to_ascii_uppercase(),
        target.path,
        target.host
    );
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    if !body.is_empty() {
        request.push_str(&format!("content-length: {}\r\n", body.len()));
    }
    request.push_str("\r\n");
    let timeout = std::time::Duration::from_millis(timeout_ms.max(1));
    tokio::time::timeout(timeout, stream.write_all(request.as_bytes()))
        .await
        .map_err(|_| "http request write timed out".to_string())?
        .map_err(|error| format!("http request write failed: {error}"))?;
    if !body.is_empty() {
        tokio::time::timeout(timeout, stream.write_all(body))
            .await
            .map_err(|_| "http request body write timed out".to_string())?
            .map_err(|error| format!("http request body write failed: {error}"))?;
    }
    let mut raw = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        if raw.len() > MAX_BODY_BYTES + 65536 {
            return Err("http response exceeded size cap".to_string());
        }
        match tokio::time::timeout(timeout, stream.read(&mut chunk)).await {
            Err(_) => return Err("http response read timed out".to_string()),
            Ok(Err(error)) => return Err(format!("http response read failed: {error}")),
            Ok(Ok(0)) => break,
            Ok(Ok(count)) => raw.extend_from_slice(&chunk[..count]),
        }
    }
    let text = String::from_utf8_lossy(&raw);
    let (head, body_start) = parse_head(&text)?;
    let framed = &raw[body_start.min(raw.len())..];
    let body = if head
        .header("transfer-encoding")
        .is_some_and(|coding| coding.contains("chunked"))
    {
        decode_chunked(framed)?.0
    } else if let Some(length) = head.header("content-length") {
        let length: usize = length
            .trim()
            .parse()
            .map_err(|_| "bad content-length".to_string())?;
        if length > MAX_BODY_BYTES {
            return Err("http body exceeded size cap".to_string());
        }
        framed
            .get(..length.min(framed.len()))
            .unwrap_or(&[])
            .to_vec()
    } else {
        // Close-delimited: everything after the head.
        framed.to_vec()
    };
    if body.len() > MAX_BODY_BYTES {
        return Err("http body exceeded size cap".to_string());
    }
    Ok((head, body))
}

/// Resolve a `Location` value against the request target (absolute URL
/// or path-absolute form). Other forms are rejected, not guessed.
fn resolve_redirect(target: &HttpTarget, location: &str) -> Result<HttpTarget, String> {
    let location = location.trim();
    if let Ok(absolute) = parse_url(location) {
        return Ok(absolute);
    }
    if let Some(path) = location.strip_prefix('/') {
        return Ok(HttpTarget {
            host: target.host.clone(),
            port: target.port,
            path: format!("/{path}"),
        });
    }
    Err(format!("unsupported http redirect target: {location:?}"))
}

/// GET/POST with redirect following (301/302/303/307/308, same method
/// except 303 which becomes GET without a body).
pub async fn request(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
    timeout_ms: u64,
) -> Result<HttpResponse, String> {
    let mut target = parse_url(url)?;
    let mut method = method.to_ascii_uppercase();
    let mut body = body.to_vec();
    for _ in 0..=MAX_REDIRECTS {
        let (head, response_body) =
            round_trip(&target, &method, headers, &body, timeout_ms).await?;
        let redirect = matches!(head.status, 301 | 302 | 303 | 307 | 308)
            .then(|| head.header("location").map(|target| target.to_string()))
            .flatten();
        match redirect {
            Some(location) => {
                target = resolve_redirect(&target, &location)?;
                if head.status == 303 {
                    method = "GET".to_string();
                    body.clear();
                }
            },
            None => {
                return Ok(HttpResponse {
                    head,
                    body: response_body,
                });
            },
        }
    }
    Err(format!("http exceeded {MAX_REDIRECTS} redirects"))
}

/// Local HTTP run as a [`ToolEvent`] stream.
pub fn run_local(
    method: String,
    url: String,
    headers: String,
    body: String,
    timeout_ms: u64,
    cancel: super::CancelToken,
) -> super::BoxStream<'static, super::ToolEvent> {
    use super::{OutputLevel, ToolEvent};
    use futures::StreamExt as _;
    Box::pin(
        futures::stream::once(async move {
            if cancel.is_cancelled() {
                return vec![ToolEvent::Cancelled];
            }
            let parsed: Vec<(String, String)> = headers
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .filter_map(|line| {
                    line.split_once(':')
                        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
                })
                .collect();
            let mut events = vec![ToolEvent::Started];
            match request(&method, &url, &parsed, body.as_bytes(), timeout_ms).await {
                Ok(response) => {
                    events.push(ToolEvent::Output {
                        line: format!("{} {}", response.head.status, response.head.reason),
                        level: OutputLevel::Info,
                    });
                    for (name, value) in &response.head.headers {
                        events.push(ToolEvent::Output {
                            line: format!("{name}: {value}"),
                            level: OutputLevel::Info,
                        });
                    }
                    let text = String::from_utf8_lossy(&response.body);
                    for line in text.lines().take(200) {
                        events.push(ToolEvent::Output {
                            line: line.to_string(),
                            level: OutputLevel::Info,
                        });
                    }
                    events.push(ToolEvent::Completed {
                        summary: Some(format!("{} bytes", response.body.len())),
                    });
                },
                Err(error) => events.push(ToolEvent::Failed { error }),
            }
            events
        })
        .flat_map(futures::stream::iter),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_forms_parse() {
        assert_eq!(
            parse_url("http://example.com").unwrap(),
            HttpTarget {
                host: "example.com".into(),
                port: 80,
                path: "/".into(),
            }
        );
        assert_eq!(
            parse_url("http://example.com:8080/a/b?x=1").unwrap(),
            HttpTarget {
                host: "example.com".into(),
                port: 8080,
                path: "/a/b?x=1".into(),
            }
        );
        assert!(parse_url("https://example.com").is_err());
        assert!(parse_url("http://").is_err());
        assert!(parse_url("http://example.com:abc/").is_err());
    }

    #[test]
    fn head_and_chunked_framing() {
        let raw =
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\n\r\n";
        let (head, consumed) = parse_head(raw).unwrap();
        assert_eq!(head.status, 200);
        assert_eq!(head.header("transfer-encoding"), Some("chunked"));
        assert_eq!(consumed, raw.len());
        assert!(parse_head("HTTP/1.1 200\r\n\r\n").is_ok());
        assert!(parse_head("garbage").is_err());

        let (body, used) = decode_chunked(b"4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n").unwrap();
        assert_eq!(body, b"Wikipedia");
        // Consumes everything through the zero-chunk line; the final
        // CRLF terminates trailers and is left for the caller.
        assert_eq!(used, "4\r\nWiki\r\n5\r\npedia\r\n0\r\n".len());
        assert!(decode_chunked(b"zz\r\n").is_err());
        assert!(decode_chunked(b"4\r\nWi").is_err());
    }

    /// Canned HTTP server on loopback: `behavior` maps the request path
    /// to a raw response. Serves until the test ends.
    async fn canned_http(behavior: fn(&str) -> &'static str) -> u16 {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("loopback bind");
        let port = listener.local_addr().expect("addr").port();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut request = vec![0u8; 1024];
                let Ok(count) = socket.read(&mut request).await else {
                    continue;
                };
                let text = String::from_utf8_lossy(&request[..count]).into_owned();
                let path = text
                    .lines()
                    .next()
                    .unwrap_or("")
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_string();
                let _ = socket.write_all(behavior(&path).as_bytes()).await;
            }
        });
        port
    }

    #[tokio::test]
    async fn chunked_body_round_trip() {
        let port = canned_http(|_| {
            "HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n5\r\nhello\r\n0\r\n\r\n"
        })
        .await;
        let response = request("GET", &format!("http://127.0.0.1:{port}/"), &[], &[], 2000)
            .await
            .expect("request");
        assert_eq!(response.head.status, 200);
        assert_eq!(response.body, b"hello");
    }

    #[tokio::test]
    async fn redirect_is_followed() {
        let port = canned_http(|path| {
            if path == "/old" {
                "HTTP/1.1 302 Found\r\nlocation: /new\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
            } else {
                "HTTP/1.1 200 OK\r\ncontent-length: 3\r\nconnection: close\r\n\r\nnew"
            }
        })
        .await;
        let response = request(
            "GET",
            &format!("http://127.0.0.1:{port}/old"),
            &[],
            &[],
            2000,
        )
        .await
        .expect("request");
        assert_eq!(response.head.status, 200);
        assert_eq!(response.body, b"new");
    }

    #[tokio::test]
    async fn https_is_rejected_not_downgraded() {
        let err = request("GET", "https://example.com/", &[], &[], 500)
            .await
            .expect_err("https must not downgrade");
        assert!(err.contains("plain http"), "got: {err}");
    }
}
