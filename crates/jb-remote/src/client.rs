//! The phone's requests to the PC: one HTTP/1.1 request per pinned-TLS connection.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use rustls::pki_types::ServerName;

/// Longest reply read (the cheat list of a big pack is a few tens of KB).
const MAX_REPLY: usize = 4 * 1024 * 1024;

/// A reply: the HTTP status and the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn json(&self) -> Result<serde_json::Value, crate::Error> {
        serde_json::from_slice(&self.body).map_err(|_| crate::Error::BadReply)
    }
}

/// Sends one request to the PC at `addr`, trusting only `fingerprint`.
pub fn request(
    addr: SocketAddr,
    fingerprint: &str,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<&serde_json::Value>,
    timeout: Duration,
) -> Result<Reply, crate::Error> {
    let config = crate::tls::client_config(fingerprint)?;
    let name = ServerName::IpAddress(addr.ip().into());
    let conn = rustls::ClientConnection::new(config, name)?;
    let sock = TcpStream::connect_timeout(&addr, timeout)?;
    sock.set_read_timeout(Some(timeout))?;
    sock.set_write_timeout(Some(timeout))?;
    sock.set_nodelay(true)?;
    let mut tls = rustls::StreamOwned::new(conn, sock);

    let body = body.map(|b| b.to_string()).unwrap_or_default();
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        body.len()
    );
    if let Some(t) = token {
        head.push_str(&format!("Authorization: Bearer {t}\r\n"));
    }
    head.push_str("\r\n");
    tls.write_all(head.as_bytes())?;
    tls.write_all(body.as_bytes())?;
    tls.flush()?;

    let mut raw = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        if let Some(reply) = parse_reply(&raw)? {
            return Ok(reply);
        }
        let n = match tls.read(&mut buf) {
            Ok(n) => n,
            // The PC closed without a TLS goodbye: whatever arrived is all there is.
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => 0,
            Err(e) => return Err(e.into()),
        };
        if n == 0 {
            return parse_reply(&raw)?.ok_or(crate::Error::BadReply);
        }
        raw.extend_from_slice(&buf[..n]);
        if raw.len() > MAX_REPLY {
            return Err(crate::Error::BadReply);
        }
    }
}

/// A whole reply once `raw` holds the headers and `Content-Length` bytes of body.
fn parse_reply(raw: &[u8]) -> Result<Option<Reply>, crate::Error> {
    let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
        return Ok(None);
    };
    let head = std::str::from_utf8(&raw[..end]).map_err(|_| crate::Error::BadReply)?;
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or(crate::Error::BadReply)?;
    let len = lines
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse::<usize>().ok())
        .ok_or(crate::Error::BadReply)?;
    let start = end + 4;
    if raw.len() < start + len {
        return Ok(None);
    }
    Ok(Some(Reply {
        status,
        body: raw[start..start + len].to_vec(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_are_read_by_content_length() {
        assert_eq!(parse_reply(b"HTTP/1.1 200 OK\r\nContent-Le").unwrap(), None);
        assert_eq!(
            parse_reply(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nab").unwrap(),
            None
        );
        let r = parse_reply(b"HTTP/1.1 401 Unauthorized\r\ncontent-length: 2\r\n\r\n{}extra")
            .unwrap()
            .unwrap();
        assert_eq!(r.status, 401);
        assert_eq!(r.body, b"{}");
        assert!(parse_reply(b"nonsense\r\n\r\n").is_err());
    }
}
