//! A blocking HTTP/1.1 JSON client for an engine on loopback. One request per act is all
//! the replay sends, so std's `TcpStream` replaces an HTTP crate and its async runtime,
//! which is what keeps this crate building in seconds.
//!
//! No read timeout: a prefill of a long captured prompt on a slow node legitimately takes
//! minutes, and an engine that dies closes the socket, which ends the read with an error.

use std::io::{Read, Write};
use std::net::TcpStream;

use crate::target::TargetError;

/// `host:port` of an engine, e.g. `127.0.0.1:58123`.
#[derive(Debug, Clone)]
pub struct Endpoint(pub String);

pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json(&self) -> Result<serde_json::Value, TargetError> {
        serde_json::from_slice(&self.body) // BOUNDARY: the engine's HTTP response body.
            .map_err(|e| TargetError(format!("engine answered {} with non-JSON ({e}): {}", self.status, self.text())))
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

impl Endpoint {
    pub fn get(&self, path: &str) -> Result<Response, TargetError> {
        self.send("GET", path, None)
    }

    pub fn post(&self, path: &str, body: &serde_json::Value) -> Result<Response, TargetError> {
        let bytes = serde_json::to_vec(body) // BOUNDARY: the engine's HTTP request body.
            .map_err(|e| TargetError(format!("cannot encode request to {path}: {e}")))?;
        self.send("POST", path, Some(&bytes))
    }

    /// POST and require a 2xx JSON answer.
    pub fn post_ok(&self, path: &str, body: &serde_json::Value) -> Result<serde_json::Value, TargetError> {
        let response = self.post(path, body)?;
        if !(200..300).contains(&response.status) {
            return Err(TargetError(format!("{path} answered {}: {}", response.status, response.text())));
        }
        response.json()
    }

    fn send(&self, method: &str, path: &str, body: Option<&[u8]>) -> Result<Response, TargetError> {
        let fail = |what: &str, e: std::io::Error| TargetError(format!("{method} {}{path}: {what}: {e}", self.0));
        let mut stream = TcpStream::connect(&self.0).map_err(|e| fail("connect", e))?;
        let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n", self.0);
        if let Some(body) = body {
            head.push_str(&format!("Content-Type: application/json\r\nContent-Length: {}\r\n", body.len()));
        }
        head.push_str("\r\n");
        stream.write_all(head.as_bytes()).map_err(|e| fail("write", e))?;
        if let Some(body) = body {
            stream.write_all(body).map_err(|e| fail("write", e))?;
        }
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).map_err(|e| fail("read", e))?;
        parse_response(&raw).map_err(|e| TargetError(format!("{method} {}{path}: {e}", self.0)))
    }
}

/// Parse a complete `Connection: close` response: status line, headers, then a body that
/// is either the rest of the stream or chunked.
pub fn parse_response(raw: &[u8]) -> Result<Response, String> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("response has no header terminator (the engine closed the connection mid-answer)")?;
    let head = String::from_utf8_lossy(&raw[..split]);
    let mut lines = head.split("\r\n");
    let status_line = lines.next().ok_or("empty response")?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| format!("bad status line {status_line:?}"))?;
    let chunked = lines.any(|l| {
        let l = l.to_ascii_lowercase();
        l.starts_with("transfer-encoding:") && l.contains("chunked")
    });
    let rest = &raw[split + 4..];
    let body = if chunked { dechunk(rest)? } else { rest.to_vec() };
    Ok(Response { status, body })
}

fn dechunk(mut rest: &[u8]) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    loop {
        let eol = rest.windows(2).position(|w| w == b"\r\n").ok_or("chunk size line is unterminated")?;
        let size_text = String::from_utf8_lossy(&rest[..eol]);
        let size_hex = size_text.split(';').next().unwrap_or_default().trim(); // unwrap_or_default: split always yields a first item
        let size = usize::from_str_radix(size_hex, 16).map_err(|_| format!("bad chunk size {size_text:?}"))?;
        rest = &rest[eol + 2..];
        if size == 0 {
            return Ok(body);
        }
        if rest.len() < size {
            return Err("chunk is shorter than its declared size".into());
        }
        body.extend_from_slice(&rest[..size]);
        rest = rest.get(size + 2..).unwrap_or_default(); // unwrap_or_default: a missing trailing CRLF ends the body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a body read wrong from the engine — a chunked answer kept with
    // its size lines would fail every JSON decode; a plain one cut short would lose
    // the timings the report is built from.
    #[test]
    fn plain_and_chunked_bodies_decode_to_the_same_json() {
        let plain = b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\n{\"a\":1}\n";
        let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\n{\"a\r\n5;x=y\r\n\":1}\n\r\n0\r\n\r\n";
        let a = parse_response(plain).map(|r| (r.status, r.json().ok()));
        let b = parse_response(chunked).map(|r| (r.status, r.json().ok()));
        assert_eq!(a, Ok((200, Some(serde_json::json!({"a": 1})))));
        assert_eq!(a, b);
        assert!(parse_response(b"HTTP/1.1 200 OK\r\n").is_err());
    }
}
