//! amber_ureq — Minimal, dependency-free blocking HTTP client.
//!
//! Implements the small slice of `ureq` used by simple tools: GET/POST with
//! headers, timeouts, and redirect following. It uses only `std::net`, so it
//! supports plain `http://` only. `https://` returns a clear error: adding TLS
//! would require a dependency (e.g. `rustls`), which defeats the purpose of a
//! drop-in removal. Use upstream `ureq` for HTTPS.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_MAX_REDIRECTS: u8 = 5;

/// A minimal HTTP request builder.
#[derive(Debug, Clone)]
pub struct Request {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    timeout: Duration,
    max_redirects: u8,
}

impl Request {
    #[must_use]
    pub fn new(method: &str, url: &str) -> Self {
        Self {
            method: method.to_string(),
            url: url.to_string(),
            headers: Vec::new(),
            timeout: DEFAULT_TIMEOUT,
            max_redirects: DEFAULT_MAX_REDIRECTS,
        }
    }

    /// Set a request header (builder style).
    #[must_use]
    pub fn set(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    /// Set a request header (alias for [`Request::set`]).
    #[must_use]
    pub fn set_header(self, name: &str, value: &str) -> Self {
        self.set(name, value)
    }

    /// Set the request timeout.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Limit how many redirects are followed.
    #[must_use]
    pub fn redirects(mut self, max: u8) -> Self {
        self.max_redirects = max;
        self
    }

    /// Send the request without a body.
    ///
    /// # Errors
    /// Returns an error if the URL is unsupported or the transfer fails.
    pub fn call(&self) -> Result<Response, Error> {
        self.call_with_body(None)
    }

    /// Send the request with a string body.
    ///
    /// # Errors
    /// Returns an error if the URL is unsupported or the transfer fails.
    pub fn send_string(&self, body: &str) -> Result<Response, Error> {
        self.call_with_body(Some(body.as_bytes()))
    }

    /// Send the request with a string body (alias for [`Request::send_string`]).
    ///
    /// # Errors
    /// Returns an error if the URL is unsupported or the transfer fails.
    pub fn send(&self, body: &str) -> Result<Response, Error> {
        self.send_string(body)
    }

    fn call_with_body(&self, body: Option<&[u8]>) -> Result<Response, Error> {
        let mut current_url = self.url.clone();
        let mut redirects_left = self.max_redirects;

        loop {
            let response = self.single_request(&current_url, body)?;
            if is_redirect(response.status) && redirects_left > 0 {
                if let Some(location) = response.header("Location") {
                    current_url = resolve_redirect(&current_url, location)?;
                    redirects_left -= 1;
                    continue;
                }
            }
            return Ok(response);
        }
    }

    fn single_request(&self, url: &str, body: Option<&[u8]>) -> Result<Response, Error> {
        let parsed = parse_url(url)?;
        let addr = format!("{}:{}", parsed.host, parsed.port);
        let socket_addr = addr
            .to_socket_addrs()
            .map_err(|e| Error::new(format!("resolve `{addr}` failed: {e}")))?
            .next()
            .ok_or_else(|| Error::new(format!("no addresses for `{addr}`")))?;

        let mut stream = TcpStream::connect_timeout(&socket_addr, self.timeout)
            .map_err(|e| Error::new(format!("connection to `{addr}` failed: {e}")))?;
        stream
            .set_read_timeout(Some(self.timeout))
            .map_err(|e| Error::new(format!("set read timeout failed: {e}")))?;
        stream
            .set_write_timeout(Some(self.timeout))
            .map_err(|e| Error::new(format!("set write timeout failed: {e}")))?;

        let body_len = body.map_or(0, <[u8]>::len);
        let mut request = format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Length: {}\r\n",
            self.method, parsed.path, parsed.host, body_len
        );
        for (name, value) in &self.headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str("\r\n");

        stream
            .write_all(request.as_bytes())
            .map_err(|e| Error::new(format!("send failed: {e}")))?;
        if let Some(body) = body {
            stream
                .write_all(body)
                .map_err(|e| Error::new(format!("send body failed: {e}")))?;
        }

        let mut response_bytes = Vec::new();
        stream
            .read_to_end(&mut response_bytes)
            .map_err(|e| Error::new(format!("read failed: {e}")))?;

        parse_response(&response_bytes)
    }
}

struct ParsedUrl {
    host: String,
    port: u16,
    path: String,
}

fn parse_url(url: &str) -> Result<ParsedUrl, Error> {
    if url.starts_with("https://") {
        return Err(Error::new(
            "HTTPS is not supported by amber_ureq (stdlib-only); use `ureq` or add a TLS backend",
        ));
    }
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| Error::new("only http:// URLs are supported"))?;
    let (hostport, path) = rest.split_once('/').map_or((rest, "/"), |(h, p)| (h, p));
    let (host, port) = match hostport.split_once(':') {
        Some((h, p)) => (
            h.to_string(),
            p.parse::<u16>()
                .map_err(|_| Error::new(format!("invalid port `{p}`")))?,
        ),
        None => (hostport.to_string(), 80),
    };
    let path = if path.is_empty() {
        "/".to_string()
    } else {
        format!("/{path}")
    };
    Ok(ParsedUrl { host, port, path })
}

fn parse_response(bytes: &[u8]) -> Result<Response, Error> {
    let header_end = find_header_end(bytes).ok_or_else(|| Error::new("malformed HTTP response"))?;
    let header_text = std::str::from_utf8(&bytes[..header_end])
        .map_err(|_| Error::new("response headers are not valid UTF-8"))?;
    let mut lines = header_text.split("\r\n");
    let status_line = lines.next().ok_or_else(|| Error::new("missing status line"))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| Error::new(format!("invalid status line `{status_line}`")))?;

    let mut headers = Vec::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }
    }

    let body = bytes[header_end + 4..].to_vec();
    Ok(Response {
        status,
        headers,
        body,
    })
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|w| w == b"\r\n\r\n")
}

const fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

fn resolve_redirect(current_url: &str, location: &str) -> Result<String, Error> {
    if location.starts_with("http://") || location.starts_with("https://") {
        return Ok(location.to_string());
    }
    let parsed = parse_url(current_url)?;
    let base = format!("http://{}:{}", parsed.host, parsed.port);
    if location.starts_with('/') {
        Ok(format!("{base}{location}"))
    } else {
        let dir = parsed
            .path
            .rfind('/')
            .map_or("/", |i| &parsed.path[..=i]);
        Ok(format!("{base}{dir}{location}"))
    }
}

/// A minimal HTTP response.
#[derive(Debug, Clone)]
pub struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Response {
    /// HTTP status code.
    #[must_use]
    pub const fn status(&self) -> u16 {
        self.status
    }

    /// Look up a response header by name (case-insensitive).
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Response body as bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.body
    }

    /// Response body as a string (lossy UTF-8).
    #[must_use]
    pub fn into_string(self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// Error type for HTTP operations.
#[derive(Debug, Clone)]
pub struct Error {
    message: String,
}

impl Error {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Error {}

/// Convenience `get` helper.
///
/// # Errors
/// Returns an error if the request fails.
pub fn get(url: &str) -> Result<Response, Error> {
    Request::new("GET", url).call()
}

/// Convenience `post` helper.
///
/// # Errors
/// Returns an error if the request fails.
pub fn post(url: &str, body: &str) -> Result<Response, Error> {
    Request::new("POST", url).send_string(body)
}
