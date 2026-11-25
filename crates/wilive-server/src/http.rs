//! A small, strict HTTP/1.1 server half: just what one port needs for the
//! dashboard's files, a JSON API and a WebSocket upgrade. Every size is capped and
//! every read has a deadline, so a slow or hostile client costs one thread for
//! seconds, never memory.

use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::time::{Duration, Instant};

pub const HEAD_MAX: usize = 8 * 1024;
pub const BODY_MAX: usize = 4 * 1024;
pub const HEAD_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    /// The path without the query.
    pub path: String,
    pub query: HashMap<String, String>,
    /// Lowercased names.
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(|s| s.as_str())
    }
    pub fn is_websocket(&self) -> bool {
        self.header("upgrade").is_some_and(|u| u.eq_ignore_ascii_case("websocket"))
            && self
                .header("connection")
                .is_some_and(|c| c.split(',').any(|t| t.trim().eq_ignore_ascii_case("upgrade")))
    }
    pub fn keep_alive(&self) -> bool {
        !self.header("connection").is_some_and(|c| c.eq_ignore_ascii_case("close"))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReadError {
    /// The client closed before sending anything: the normal end of keep-alive.
    Closed,
    TooBig,
    Malformed,
    Slow,
    Io(String),
}

fn decode_component(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parses one request head (everything before the blank line).
pub fn parse_head(head: &str) -> Result<Request, ReadError> {
    let mut lines = head.split("\r\n");
    let first = lines.next().ok_or(ReadError::Malformed)?;
    let mut parts = first.split(' ');
    let (method, target, version) = match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(m), Some(t), Some(v), None) => (m, t, v),
        _ => return Err(ReadError::Malformed),
    };
    if !version.starts_with("HTTP/1.") || !target.starts_with('/') || method.is_empty() || !method.bytes().all(|c| c.is_ascii_uppercase()) {
        return Err(ReadError::Malformed);
    }
    let (path, q) = target.split_once('?').unwrap_or((target, ""));
    let query = q
        .split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (decode_component(k), decode_component(v))
        })
        .collect();
    let mut headers = HashMap::new();
    for l in lines {
        if l.is_empty() {
            continue;
        }
        let (k, v) = l.split_once(':').ok_or(ReadError::Malformed)?;
        if k.is_empty() || k.contains(' ') {
            return Err(ReadError::Malformed);
        }
        headers.insert(k.to_ascii_lowercase(), v.trim().to_string());
    }
    Ok(Request { method: method.to_string(), path: decode_component(path), query, headers, body: Vec::new() })
}

/// Reads one request from a stream: head within [`HEAD_DEADLINE`], at most
/// [`HEAD_MAX`] bytes; then a body of at most [`BODY_MAX`] bytes. `carry` holds
/// bytes already read past the previous request (pipelining) and is refilled.
pub fn read_request<S: Read>(s: &mut S, carry: &mut Vec<u8>, set_timeout: &mut dyn FnMut(Duration)) -> Result<Request, ReadError> {
    let start = Instant::now();
    let mut buf = std::mem::take(carry);
    let end = loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p;
        }
        if buf.len() > HEAD_MAX {
            return Err(ReadError::TooBig);
        }
        let left = HEAD_DEADLINE.saturating_sub(start.elapsed());
        if left.is_zero() {
            return Err(ReadError::Slow);
        }
        set_timeout(left);
        let mut chunk = [0u8; 2048];
        match s.read(&mut chunk) {
            Ok(0) if buf.is_empty() => return Err(ReadError::Closed),
            Ok(0) => return Err(ReadError::Malformed),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return Err(if buf.is_empty() { ReadError::Closed } else { ReadError::Slow })
            }
            Err(e) => return Err(ReadError::Io(e.to_string())),
        }
    };
    if end > HEAD_MAX {
        return Err(ReadError::TooBig);
    }
    let head = std::str::from_utf8(&buf[..end]).map_err(|_| ReadError::Malformed)?;
    let mut req = parse_head(head)?;
    if req.headers.contains_key("transfer-encoding") {
        return Err(ReadError::Malformed); // never needed here; refusing it closes a smuggling door
    }
    let len: usize = match req.header("content-length") {
        Some(v) => v.parse().map_err(|_| ReadError::Malformed)?,
        None => 0,
    };
    if len > BODY_MAX {
        return Err(ReadError::TooBig);
    }
    let mut rest = buf.split_off(end + 4);
    while rest.len() < len {
        let left = HEAD_DEADLINE.saturating_sub(start.elapsed());
        if left.is_zero() {
            return Err(ReadError::Slow);
        }
        set_timeout(left);
        let mut chunk = [0u8; 2048];
        match s.read(&mut chunk) {
            Ok(0) => return Err(ReadError::Malformed),
            Ok(n) => rest.extend_from_slice(&chunk[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => return Err(ReadError::Slow),
            Err(e) => return Err(ReadError::Io(e.to_string())),
        }
    }
    *carry = rest.split_off(len);
    req.body = rest;
    Ok(req)
}

pub struct Response {
    pub status: u16,
    pub ctype: &'static str,
    pub body: Vec<u8>,
    pub headers: Vec<(&'static str, String)>,
}

impl Response {
    pub fn json(status: u16, v: &serde_json::Value) -> Self {
        Response { status, ctype: "application/json", body: v.to_string().into_bytes(), headers: vec![("Cache-Control", "no-store".into())] }
    }
    pub fn error(status: u16, reason: &str) -> Self {
        Self::json(status, &serde_json::json!({"error": reason}))
    }
    pub fn empty(status: u16) -> Self {
        Response { status, ctype: "text/plain", body: Vec::new(), headers: Vec::new() }
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        421 => "Misdirected Request",
        429 => "Too Many Requests",
        503 => "Service Unavailable",
        _ => "",
    }
}

/// Every response carries these: the page loads only its own files, cannot be
/// framed, and sends no referrer.
pub const SECURITY_HEADERS: &[(&str, &str)] = &[
    ("Content-Security-Policy", "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: blob:; media-src 'self' blob:; connect-src 'self' ws: wss:; font-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'"),
    ("X-Content-Type-Options", "nosniff"),
    ("Referrer-Policy", "no-referrer"),
    ("X-Frame-Options", "DENY"),
    ("Cross-Origin-Opener-Policy", "same-origin"),
    ("Cross-Origin-Resource-Policy", "same-origin"),
    ("Permissions-Policy", "camera=(), geolocation=(), payment=()"),
];

pub fn write_response<W: Write>(w: &mut W, r: &Response, keep_alive: bool, head_only: bool) -> std::io::Result<()> {
    let mut h = format!("HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: {}\r\n", r.status, reason(r.status), r.ctype, r.body.len(), if keep_alive { "keep-alive" } else { "close" });
    for (k, v) in SECURITY_HEADERS {
        h.push_str(&format!("{k}: {v}\r\n"));
    }
    for (k, v) in &r.headers {
        h.push_str(&format!("{k}: {v}\r\n"));
    }
    h.push_str("\r\n");
    let mut out = h.into_bytes();
    if !head_only {
        out.extend_from_slice(&r.body);
    }
    w.write_all(&out)?;
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn read(bytes: &[u8]) -> Result<Request, ReadError> {
        let mut carry = Vec::new();
        read_request(&mut Cursor::new(bytes.to_vec()), &mut carry, &mut |_| {})
    }

    #[test]
    fn a_request_with_a_query_and_a_body() {
        let r = read(b"POST /api/pair/start?x=1&name=Pixel%207 HTTP/1.1\r\nHost: a\r\nContent-Length: 4\r\n\r\n{\"a\"").unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.path, "/api/pair/start");
        assert_eq!(r.query["name"], "Pixel 7");
        assert_eq!(r.header("host"), Some("a"));
        assert_eq!(r.body, b"{\"a\"");
    }

    #[test]
    fn two_pipelined_requests_are_read_one_at_a_time() {
        let mut c = Cursor::new(b"GET /a HTTP/1.1\r\n\r\nGET /b HTTP/1.1\r\n\r\n".to_vec());
        let mut carry = Vec::new();
        assert_eq!(read_request(&mut c, &mut carry, &mut |_| {}).unwrap().path, "/a");
        assert_eq!(read_request(&mut c, &mut carry, &mut |_| {}).unwrap().path, "/b");
        assert_eq!(read_request(&mut c, &mut carry, &mut |_| {}).unwrap_err(), ReadError::Closed);
    }

    #[test]
    fn oversized_and_malformed_requests_are_refused() {
        let big = format!("GET / HTTP/1.1\r\nX: {}\r\n\r\n", "a".repeat(HEAD_MAX));
        assert_eq!(read(big.as_bytes()).unwrap_err(), ReadError::TooBig);
        let body = format!("POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n", BODY_MAX + 1);
        assert_eq!(read(body.as_bytes()).unwrap_err(), ReadError::TooBig);
        assert_eq!(read(b"GET / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap_err(), ReadError::Malformed);
        assert_eq!(read(b"GET noslash HTTP/1.1\r\n\r\n").unwrap_err(), ReadError::Malformed);
        assert_eq!(read(b"get / HTTP/1.1\r\n\r\n").unwrap_err(), ReadError::Malformed);
        assert_eq!(read(b"GET / HTTP/2\r\n\r\n").unwrap_err(), ReadError::Malformed);
        assert_eq!(read(b"GET / HTTP/1.1\r\nbad header\r\n\r\n").unwrap_err(), ReadError::Malformed);
        assert_eq!(read(b"POST / HTTP/1.1\r\nContent-Length: x\r\n\r\n").unwrap_err(), ReadError::Malformed);
        assert_eq!(read(b"GET / HTTP/1.1\r\n").unwrap_err(), ReadError::Malformed, "cut off mid-head");
    }

    #[test]
    fn a_websocket_upgrade_is_recognised() {
        let r = read(b"GET /v1/signal HTTP/1.1\r\nUpgrade: websocket\r\nConnection: keep-alive, Upgrade\r\n\r\n").unwrap();
        assert!(r.is_websocket());
        assert!(!read(b"GET / HTTP/1.1\r\n\r\n").unwrap().is_websocket());
    }

    #[test]
    fn percent_decoding_never_panics() {
        for s in ["%", "%2", "%zz", "a%2", "%E2%82", "+%20+"] {
            let _ = decode_component(s);
        }
        assert_eq!(decode_component("a%20b+c"), "a b c");
    }

    #[test]
    fn responses_carry_the_security_headers() {
        let mut out = Vec::new();
        write_response(&mut out, &Response::error(404, "nope"), false, false).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.starts_with("HTTP/1.1 404 Not Found\r\n"));
        assert!(s.contains("Content-Security-Policy: default-src 'self'"));
        assert!(s.contains("X-Frame-Options: DENY"));
        assert!(s.ends_with("{\"error\":\"nope\"}"));
    }
}
