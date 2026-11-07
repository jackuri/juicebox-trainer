//! Phone remote: a small web page on this PC's local network, so a phone on
//! the same Wi-Fi can turn cheats on and off.
//!
//! Off until the player starts it. A phone pairs by opening the link in the
//! QR code, which holds a one-time code; the phone then gets its own random
//! session cookie and the code is replaced. Every other request needs that
//! cookie, changes also need a custom header (so other web pages can't send
//! them), and requests must address the PC by IP (so a web page can't reach
//! it through a made-up host name). The page can only do what the trainer's
//! own buttons do.

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::Deserialize;
use tracing::{info, warn};
use windows::Win32::Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom};

pub const DEFAULT_PORT: u16 = 47110;
const MAX_DEVICES: usize = 5;
const MAX_CONNECTIONS: usize = 16;
const MAX_REQUEST: usize = 16 * 1024;

const PAGE: &str = include_str!("../remote/index.html");
const SCRIPT: &str = include_str!("../remote/app.js");

/// What a phone asked for.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub id: String,
    /// `toggle`, `press`, `set` or `release`.
    pub op: String,
    #[serde(default)]
    pub on: Option<bool>,
    #[serde(default)]
    pub value: Option<String>,
}

/// The trainer, as the remote sees it.
pub trait Backend: Send + Sync + 'static {
    /// The loaded pack's cheats (or `{"pack": null}`).
    fn view(&self) -> serde_json::Value;
    fn act(&self, action: &Action) -> Result<(), String>;
}

/// Random hex token (128 bits from the system's secure generator).
pub fn token() -> String {
    let mut buf = [0u8; 16];
    // SAFETY: fills `buf`; the system RNG needs no algorithm handle.
    let status = unsafe { BCryptGenRandom(None, &mut buf, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    assert!(status.is_ok(), "the system random generator failed");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn same(a: &str, b: &str) -> bool {
    // Constant time for equal lengths.
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// This PC's address on the local network (the one the default route uses).
pub fn lan_ip() -> Option<IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    // UDP connect sends nothing; it only picks the outgoing interface.
    s.connect("10.254.254.254:9").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

#[derive(Debug)]
struct Shared {
    code: Mutex<String>,
    sessions: Mutex<Vec<String>>,
}

impl Shared {
    fn authed(&self, cookie: Option<&str>) -> bool {
        let Some(c) = cookie else { return false };
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .any(|s| same(s, c))
    }

    /// Uses up the pairing code; returns the phone's new session.
    fn pair(&self, code: &str) -> Option<String> {
        let mut current = self.code.lock().unwrap_or_else(|e| e.into_inner());
        if !same(&current, code) {
            return None;
        }
        *current = token();
        let session = token();
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if sessions.len() >= MAX_DEVICES {
            sessions.remove(0);
        }
        sessions.push(session.clone());
        Some(session)
    }
}

/// A running remote. Dropping it stops the server and forgets every phone.
#[derive(Debug)]
pub struct Server {
    addr: SocketAddr,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// Listens on `bind:port` (`port` 0 picks a free one; if `port` is taken,
    /// a free one is used instead). `public` is the address put in the link.
    pub fn start(
        backend: Arc<dyn Backend>,
        bind: IpAddr,
        port: u16,
        public: IpAddr,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind((bind, port)).or_else(|_| TcpListener::bind((bind, 0)))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let shared = Arc::new(Shared {
            code: Mutex::new(token()),
            sessions: Mutex::default(),
        });
        let stop = Arc::new(AtomicBool::new(false));
        let (s, st) = (Arc::clone(&shared), Arc::clone(&stop));
        let thread = thread::Builder::new()
            .name("ot-remote".into())
            .spawn(move || serve(&listener, &s, &backend, &st))?;
        info!(port, "phone remote started");
        Ok(Self {
            addr: SocketAddr::new(public, port),
            shared,
            stop,
            thread: Some(thread),
        })
    }

    /// The link for the QR code (holds the one-time pairing code).
    pub fn pair_url(&self) -> String {
        let code = self
            .shared
            .code
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        format!("http://{}/pair?code={code}", self.addr)
    }

    pub fn address(&self) -> String {
        format!("http://{}/", self.addr)
    }

    pub fn devices(&self) -> usize {
        self.shared
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    /// Unpairs every phone and makes a new code.
    pub fn forget_devices(&self) {
        self.shared
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        *self.shared.code.lock().unwrap_or_else(|e| e.into_inner()) = token();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        info!("phone remote stopped");
    }
}

fn serve(
    listener: &TcpListener,
    shared: &Arc<Shared>,
    backend: &Arc<dyn Backend>,
    stop: &AtomicBool,
) {
    let open = Arc::new(AtomicUsize::new(0));
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                if open.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                    continue;
                }
                open.fetch_add(1, Ordering::SeqCst);
                let (s, b, o) = (Arc::clone(shared), Arc::clone(backend), Arc::clone(&open));
                let spawned =
                    thread::Builder::new()
                        .name("ot-remote-conn".into())
                        .spawn(move || {
                            if let Err(e) = handle(stream, &s, b.as_ref()) {
                                tracing::debug!("remote request: {e}");
                            }
                            o.fetch_sub(1, Ordering::SeqCst);
                        });
                if spawned.is_err() {
                    open.fetch_sub(1, Ordering::SeqCst);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                warn!("remote accept: {e}");
                thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Request {
    method: String,
    path: String,
    query: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn content_length(&self) -> usize {
        self.header("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }

    fn cookie(&self, name: &str) -> Option<&str> {
        self.header("cookie")?.split(';').find_map(|kv| {
            let (k, v) = kv.trim().split_once('=')?;
            (k == name).then_some(v)
        })
    }

    fn param(&self, name: &str) -> Option<&str> {
        self.query.split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            (k == name).then_some(v)
        })
    }
}

/// Parses one HTTP/1.1 request (headers, then a body of Content-Length).
fn parse(raw: &[u8]) -> Option<(Request, usize)> {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..end]).ok()?;
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_string();
    let target = first.next()?;
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            Some((k.trim().to_string(), v.trim().to_string()))
        })
        .collect();
    let req = Request {
        method,
        path: path.to_string(),
        query: query.to_string(),
        headers,
        body: Vec::new(),
    };
    let len = req.content_length();
    Some((req, end + 4 + len))
}

/// Requests must name the PC by IP address (`192.168.1.20:47110`), never by
/// a host name, which stops DNS-rebinding pages from reaching the remote.
fn host_ok(host: Option<&str>) -> bool {
    let Some(h) = host else { return false };
    h.parse::<SocketAddr>().is_ok() || h.parse::<IpAddr>().is_ok()
}

struct Response {
    status: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
    extra: Vec<String>,
}

fn text(status: &'static str, body: &str) -> Response {
    let html = format!(
        "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width,initial-scale=1\"><title>OpenTrainer</title><body style=\"font-family:system-ui;background:#09090b;color:#e4e4e7;padding:24px\"><p>{body}</p>"
    );
    Response {
        status,
        content_type: "text/html; charset=utf-8",
        body: html.into_bytes(),
        extra: Vec::new(),
    }
}

fn json(status: &'static str, v: &serde_json::Value) -> Response {
    Response {
        status,
        content_type: "application/json",
        body: v.to_string().into_bytes(),
        extra: Vec::new(),
    }
}

fn route(req: &Request, shared: &Shared, backend: &dyn Backend) -> Response {
    if !host_ok(req.header("host")) {
        return text(
            "403 Forbidden",
            "Open the remote by the PC's address shown in OpenTrainer.",
        );
    }
    let authed = shared.authed(req.cookie("ot"));
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/pair") => match req.param("code").and_then(|c| shared.pair(c)) {
            Some(session) => Response {
                status: "303 See Other",
                content_type: "text/plain",
                body: b"Paired".to_vec(),
                extra: vec![
                    "Location: /".into(),
                    format!(
                        "Set-Cookie: ot={session}; Path=/; HttpOnly; SameSite=Strict; Max-Age=86400"
                    ),
                ],
            },
            None => text(
                "403 Forbidden",
                "This pairing code was already used or is out of date. Scan the QR code in OpenTrainer again.",
            ),
        },
        ("GET", "/") if authed => Response {
            status: "200 OK",
            content_type: "text/html; charset=utf-8",
            body: PAGE.as_bytes().to_vec(),
            extra: Vec::new(),
        },
        ("GET", "/") => text(
            "401 Unauthorized",
            "Not paired. In OpenTrainer, open Phone remote and scan the QR code with this phone.",
        ),
        ("GET", "/app.js") => Response {
            status: "200 OK",
            content_type: "text/javascript; charset=utf-8",
            body: SCRIPT.as_bytes().to_vec(),
            extra: Vec::new(),
        },
        ("GET", "/api/state") if authed => json("200 OK", &backend.view()),
        ("POST", "/api/action") if authed => {
            if req.header("x-opentrainer") != Some("1") {
                return json(
                    "403 Forbidden",
                    &serde_json::json!({ "error": "missing header" }),
                );
            }
            match serde_json::from_slice::<Action>(&req.body) {
                Ok(a) => match backend.act(&a) {
                    Ok(()) => json("200 OK", &backend.view()),
                    Err(e) => json("409 Conflict", &serde_json::json!({ "error": e })),
                },
                Err(_) => json(
                    "400 Bad Request",
                    &serde_json::json!({ "error": "bad request" }),
                ),
            }
        }
        (_, "/api/state" | "/api/action") => json(
            "401 Unauthorized",
            &serde_json::json!({ "error": "not paired" }),
        ),
        _ => text("404 Not Found", "Not found."),
    }
}

fn handle(mut stream: TcpStream, shared: &Shared, backend: &dyn Backend) -> std::io::Result<()> {
    // Accepted sockets inherit the listener's non-blocking mode on Windows.
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    let req = loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        raw.extend_from_slice(&buf[..n]);
        if raw.len() > MAX_REQUEST {
            return Ok(());
        }
        if let Some((mut req, total)) = parse(&raw)
            && raw.len() >= total
        {
            req.body = raw[total - req.content_length()..total].to_vec();
            break req;
        }
    };
    let r = route(&req, shared, backend);
    let mut head = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; frame-ancestors 'none'\r\n",
        r.status,
        r.content_type,
        r.body.len()
    );
    for h in &r.extra {
        head.push_str(h);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(&r.body)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        acts: Mutex<Vec<Action>>,
    }

    impl Backend for Fake {
        fn view(&self) -> serde_json::Value {
            serde_json::json!({ "pack": { "game": "Testbed" }, "cheats": [] })
        }
        fn act(&self, a: &Action) -> Result<(), String> {
            if a.id == "bad" {
                return Err("no cheat 'bad'".into());
            }
            self.acts.lock().unwrap().push(a.clone());
            Ok(())
        }
    }

    fn request(addr: SocketAddr, text: &str) -> String {
        let mut s = TcpStream::connect(addr).unwrap();
        s.write_all(text.as_bytes()).unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        out
    }

    fn cookie_of(response: &str) -> String {
        let line = response
            .lines()
            .find(|l| l.starts_with("Set-Cookie: ot="))
            .unwrap();
        line["Set-Cookie: ot=".len()..]
            .split(';')
            .next()
            .unwrap()
            .to_string()
    }

    #[test]
    fn parses_requests() {
        let raw = b"POST /api/action?x=1 HTTP/1.1\r\nHost: 1.2.3.4:5\r\nCookie: a=b; ot=tok\r\nContent-Length: 2\r\n\r\n{}";
        let (r, total) = parse(raw).unwrap();
        assert_eq!(
            (r.method.as_str(), r.path.as_str(), r.query.as_str()),
            ("POST", "/api/action", "x=1")
        );
        assert_eq!(r.cookie("ot"), Some("tok"));
        assert_eq!(total, raw.len());
        assert!(
            parse(b"GET / HTTP/1.1\r\n").is_none(),
            "headers not finished"
        );
        assert!(host_ok(Some("192.168.1.5:47110")) && host_ok(Some("[::1]:80")));
        assert!(!host_ok(Some("evil.example:47110")) && !host_ok(None));
    }

    #[test]
    fn pairs_once_then_needs_the_cookie() {
        let fake = Arc::new(Fake::default());
        let local: IpAddr = "127.0.0.1".parse().unwrap();
        let server = Server::start(fake.clone(), local, 0, local).unwrap();
        let addr = server.addr;
        let host = format!("Host: {addr}");

        // Not paired: no page, no state, no actions.
        assert!(
            request(addr, &format!("GET / HTTP/1.1\r\n{host}\r\n\r\n")).starts_with("HTTP/1.1 401")
        );
        assert!(
            request(addr, &format!("GET /api/state HTTP/1.1\r\n{host}\r\n\r\n"))
                .starts_with("HTTP/1.1 401")
        );
        // A host name is refused.
        assert!(
            request(addr, "GET / HTTP/1.1\r\nHost: evil.example\r\n\r\n")
                .starts_with("HTTP/1.1 403")
        );

        let url = server.pair_url();
        let code = url.split("code=").nth(1).unwrap().to_string();
        assert!(
            request(
                addr,
                &format!("GET /pair?code=wrong HTTP/1.1\r\n{host}\r\n\r\n")
            )
            .starts_with("HTTP/1.1 403")
        );
        let paired = request(
            addr,
            &format!("GET /pair?code={code} HTTP/1.1\r\n{host}\r\n\r\n"),
        );
        assert!(paired.starts_with("HTTP/1.1 303"), "{paired}");
        let cookie = cookie_of(&paired);
        assert_eq!(server.devices(), 1);
        // The code works once.
        assert!(
            request(
                addr,
                &format!("GET /pair?code={code} HTTP/1.1\r\n{host}\r\n\r\n")
            )
            .starts_with("HTTP/1.1 403")
        );
        assert_ne!(server.pair_url(), url);

        let page = request(
            addr,
            &format!("GET / HTTP/1.1\r\n{host}\r\nCookie: ot={cookie}\r\n\r\n"),
        );
        assert!(
            page.starts_with("HTTP/1.1 200") && page.contains("/app.js"),
            "{page}"
        );
        let state = request(
            addr,
            &format!("GET /api/state HTTP/1.1\r\n{host}\r\nCookie: ot={cookie}\r\n\r\n"),
        );
        assert!(state.contains("Testbed"));

        // Actions need the header too.
        let body = r#"{"id":"inf_health","op":"toggle","on":true}"#;
        let post = |extra: &str| {
            request(
                addr,
                &format!(
                    "POST /api/action HTTP/1.1\r\n{host}\r\nCookie: ot={cookie}\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                ),
            )
        };
        assert!(post("").starts_with("HTTP/1.1 403"));
        assert!(post("X-OpenTrainer: 1\r\n").starts_with("HTTP/1.1 200"));
        assert_eq!(fake.acts.lock().unwrap()[0].op, "toggle");

        server.forget_devices();
        assert!(
            request(
                addr,
                &format!("GET /api/state HTTP/1.1\r\n{host}\r\nCookie: ot={cookie}\r\n\r\n")
            )
            .starts_with("HTTP/1.1 401")
        );
    }

    #[test]
    fn tokens_are_random_hex() {
        let (a, b) = (token(), token());
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}

// ----- app glue ------------------------------------------------------------------

/// The remote's place in the app: off until started.
#[derive(Debug, Default)]
pub struct RemoteState {
    server: Mutex<Option<Server>>,
}

struct AppBackend {
    app: tauri::AppHandle,
}

impl Backend for AppBackend {
    fn view(&self) -> serde_json::Value {
        use tauri::Manager;
        self.app.state::<crate::commands::AppState>().remote_view()
    }

    fn act(&self, action: &Action) -> Result<(), String> {
        use tauri::{Emitter, Manager};
        let r = self
            .app
            .state::<crate::commands::AppState>()
            .remote_act(action);
        let _ = self.app.emit("trainer-changed", "remote");
        r
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStatus {
    /// The link in the QR code (one-time).
    pub pair_url: String,
    /// The QR code as an SVG image.
    pub qr_svg: String,
    /// Where paired phones open the remote.
    pub address: String,
    pub devices: usize,
}

fn status_of(server: &Server) -> RemoteStatus {
    let pair_url = server.pair_url();
    let qr_svg = qrcode::QrCode::new(pair_url.as_bytes())
        .map(|q| {
            q.render::<qrcode::render::svg::Color<'_>>()
                .min_dimensions(220, 220)
                .quiet_zone(true)
                .build()
        })
        .unwrap_or_default();
    RemoteStatus {
        pair_url,
        qr_svg,
        address: server.address(),
        devices: server.devices(),
    }
}

impl RemoteState {
    /// Stops the remote (app exit).
    pub fn stop(&self) {
        lock(self).take();
    }
}

fn lock(state: &RemoteState) -> std::sync::MutexGuard<'_, Option<Server>> {
    state.server.lock().unwrap_or_else(|e| e.into_inner())
}

/// Starts the remote on the local network (Windows may ask to allow it
/// through the firewall: allow it on private networks).
#[tauri::command]
pub async fn remote_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, RemoteState>,
) -> crate::commands::CmdResult<RemoteStatus> {
    let mut server = lock(&state);
    if server.is_none() {
        let ip = lan_ip().ok_or_else(|| {
            crate::commands::CommandError::new(
                "remote",
                "this PC isn't on a network, so a phone can't reach it",
            )
        })?;
        let backend: Arc<dyn Backend> = Arc::new(AppBackend { app });
        *server = Some(
            Server::start(backend, IpAddr::from([0, 0, 0, 0]), DEFAULT_PORT, ip).map_err(|e| {
                crate::commands::CommandError::new(
                    "remote",
                    format!("couldn't start the remote: {e}"),
                )
            })?,
        );
    }
    Ok(status_of(server.as_ref().expect("started above")))
}

#[tauri::command]
pub fn remote_stop(state: tauri::State<'_, RemoteState>) {
    lock(&state).take();
}

#[tauri::command]
pub fn remote_status(state: tauri::State<'_, RemoteState>) -> Option<RemoteStatus> {
    lock(&state).as_ref().map(status_of)
}

/// Unpairs every phone (they need to scan the new QR code).
#[tauri::command]
pub fn remote_forget(state: tauri::State<'_, RemoteState>) -> Option<RemoteStatus> {
    let server = lock(&state);
    server.as_ref().map(|s| {
        s.forget_devices();
        status_of(s)
    })
}
