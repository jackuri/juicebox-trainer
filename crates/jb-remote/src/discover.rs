//! Finding a paired PC whose address changed: the phone asks the local network (one UDP
//! broadcast) "who has this certificate?", and that PC answers with its remote's port.
//!
//! Only the start of the fingerprint is sent, and the answer is only a port; the phone then
//! connects with pinned TLS as always, so a wrong answer can't fool it.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

const ASK: &[u8] = b"JBR1?";
const ANSWER: &[u8] = b"JBR1!";
/// Fingerprint characters in a question (64 bits: enough to tell PCs apart).
const PREFIX: usize = 16;

/// The question for the PC with `fingerprint`.
pub fn question(fingerprint: &str) -> Vec<u8> {
    let mut q = ASK.to_vec();
    q.extend_from_slice(fingerprint.get(..PREFIX).unwrap_or(fingerprint).as_bytes());
    q
}

/// The PC's answer to `packet`, if it asks for this PC.
pub fn answer(packet: &[u8], fingerprint: &str, port: u16) -> Option<Vec<u8>> {
    (packet == question(fingerprint).as_slice()).then(|| {
        let mut a = ANSWER.to_vec();
        a.extend_from_slice(port.to_string().as_bytes());
        a
    })
}

fn port_of(reply: &[u8]) -> Option<u16> {
    std::str::from_utf8(reply.strip_prefix(ANSWER)?)
        .ok()?
        .parse()
        .ok()
        .filter(|p| *p != 0)
}

/// Asks the local network (broadcast to `port`, and to `also` if given) for the PC with
/// `fingerprint`. Returns where its remote is, or `None` after `wait`.
pub fn find(
    fingerprint: &str,
    port: u16,
    also: Option<IpAddr>,
    wait: Duration,
) -> Option<SocketAddr> {
    let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    sock.set_broadcast(true).ok()?;
    let q = question(fingerprint);
    let mut sent = sock.send_to(&q, (Ipv4Addr::BROADCAST, port)).is_ok();
    if let Some(ip) = also {
        sent |= sock.send_to(&q, (ip, port)).is_ok();
    }
    if !sent {
        return None;
    }
    let end = Instant::now() + wait;
    let mut buf = [0u8; 64];
    loop {
        let left = end.checked_duration_since(Instant::now())?;
        sock.set_read_timeout(Some(left.max(Duration::from_millis(1))))
            .ok()?;
        match sock.recv_from(&mut buf) {
            Ok((n, from)) => {
                if let Some(p) = port_of(&buf[..n]) {
                    return Some(SocketAddr::new(from.ip(), p));
                }
            }
            Err(_) => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_only_questions_for_this_pc() {
        let fp = "0123456789abcdef".repeat(4);
        let q = question(&fp);
        assert_eq!(q, b"JBR1?0123456789abcdef");
        assert_eq!(answer(&q, &fp, 47110).as_deref(), Some(&b"JBR1!47110"[..]));
        assert_eq!(answer(&question(&"f".repeat(64)), &fp, 47110), None);
        assert_eq!(answer(b"JBR1?", &fp, 47110), None);
        assert_eq!(port_of(b"JBR1!47110"), Some(47110));
        assert_eq!(port_of(b"JBR1!0"), None);
        assert_eq!(port_of(b"other"), None);
    }

    #[test]
    fn finds_a_pc_answering_on_this_machine() {
        let fp = "ab".repeat(32);
        let pc = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = pc.local_addr().unwrap().port();
        let fp2 = fp.clone();
        let t = std::thread::spawn(move || {
            let mut buf = [0u8; 64];
            pc.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            // The broadcast may or may not arrive here; the direct question does.
            loop {
                let (n, from) = pc.recv_from(&mut buf).unwrap();
                if let Some(a) = answer(&buf[..n], &fp2, 47999) {
                    pc.send_to(&a, from).unwrap();
                    return;
                }
            }
        });
        let found = find(
            &fp,
            port,
            Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            Duration::from_secs(5),
        );
        t.join().unwrap();
        assert_eq!(
            found,
            Some(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 47999))
        );
    }
}
