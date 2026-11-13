//! Phone remote: turn cheats on and off from a phone on the same Wi-Fi.
//!
//! Off until the player turns it on (then it stays on at the next start, until turned off).
//! One port serves two kinds of phone:
//!
//! - **The JuiceBox Remote app** (Android, v1.36.0): pinned TLS. The QR code holds this PC's
//!   certificate fingerprint and a one-time code; the app trades the code for its own token
//!   (`POST /app/pair`) and is remembered (only the token's SHA-256 is kept, in
//!   `%APPDATA%\JuiceBox Trainer\remote\phones.json`). If this PC's address changes, the app
//!   asks the network for it ([`jb_remote::discover`], answered on the same UDP port).
//! - **A web page** for any phone (the camera opens the same QR link): plain HTTP. The phone
//!   gets a random session cookie for a day and the code is replaced. Every other request
//!   needs that cookie, changes also need a custom header (so other web pages can't send
//!   them), and requests must address the PC by IP (so a web page can't reach it through a
//!   made-up host name).
//!
//! Either way the phone can only do what the trainer's own buttons do.

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use jb_remote::tls::Identity;
use jb_remote::{PairLink, PairReply, PairRequest, PresetRequest, same, sha256_hex};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

pub use jb_remote::{Action, DEFAULT_PORT, token};

/// Web-page phones (in memory, a day each).
const MAX_DEVICES: usize = 5;
/// App phones remembered.
const MAX_PHONES: usize = 10;
const MAX_CONNECTIONS: usize = 16;
const MAX_REQUEST: usize = 16 * 1024;
/// How often a phone's "last used" is written to disk.
const SEEN_EVERY_SECS: u64 = 3600;

const PAGE: &str = include_str!("../remote/index.html");
const SCRIPT: &str = include_str!("../remote/app.js");

/// The trainer, as the remote sees it.
pub trait Backend: Send + Sync + 'static {
    /// The loaded pack's cheats (or `{"pack": null}`).
    fn view(&self) -> serde_json::Value;
    fn act(&self, action: &Action) -> Result<(), String>;
    /// Applies one of the loaded game's presets.
    fn preset(&self, name: &str) -> Result<(), String>;
}

/// This PC's address on the local network (the one the default route uses).
pub fn lan_ip() -> Option<IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    // UDP connect sends nothing; it only picks the outgoing interface.
    s.connect("10.254.254.254:9").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// This PC's name, shown on the phone.
fn pc_name() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "My PC".into())
}

// ----- what's kept on disk ---------------------------------------------------------

/// A phone with the app, paired with this PC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Phone {
    id: String,
    name: String,
    token_sha256: String,
    /// Seconds since 1970.
    added: u64,
    last_seen: u64,
}

/// A paired phone, for the screen (no token).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneInfo {
    pub id: String,
    pub name: String,
    pub added: u64,
    pub last_seen: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PhonesFile {
    #[serde(default)]
    phones: Vec<Phone>,
}

/// Where the remote keeps its certificate, paired phones and on/off (`None`: nowhere, tests).
#[derive(Debug, Clone)]
pub struct Store {
    dir: Option<PathBuf>,
}

impl Store {
    /// `%APPDATA%\JuiceBox Trainer\remote`.
    pub fn default_location() -> Self {
        Self {
            dir: std::env::var_os("APPDATA")
                .map(|a| PathBuf::from(a).join(jb_core::names::FOLDER).join("remote")),
        }
    }

    #[cfg(test)]
    fn at(dir: PathBuf) -> Self {
        Self { dir: Some(dir) }
    }

    fn file(&self, name: &str) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(name))
    }

    /// This PC's certificate: made once, then kept (paired phones trust exactly it).
    fn identity(&self) -> Result<Identity, String> {
        let (Some(cert_file), Some(key_file)) = (self.file("cert.der"), self.file("key.der"))
        else {
            return Identity::generate().map_err(|e| e.to_string());
        };
        if let (Ok(cert), Ok(key)) = (std::fs::read(&cert_file), std::fs::read(&key_file))
            && let id = (Identity { cert, key })
            && id.server_config().is_ok()
        {
            return Ok(id);
        }
        let id = Identity::generate().map_err(|e| e.to_string())?;
        if let Some(dir) = &self.dir {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&key_file, &id.key).map_err(|e| e.to_string())?;
        std::fs::write(&cert_file, &id.cert).map_err(|e| e.to_string())?;
        info!(fingerprint = %id.fingerprint(), "made the phone remote's certificate");
        Ok(id)
    }

    fn phones(&self) -> Vec<Phone> {
        self.file("phones.json")
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|t| serde_json::from_str::<PhonesFile>(&t).ok())
            .map(|p| p.phones)
            .unwrap_or_default()
    }

    fn save_phones(&self, phones: &[Phone]) {
        let Some(file) = self.file("phones.json") else {
            return;
        };
        let text = serde_json::to_string_pretty(&PhonesFile {
            phones: phones.to_vec(),
        })
        .unwrap_or_default();
        let tmp = file.with_extension("json.tmp");
        let ok = file
            .parent()
            .is_some_and(|d| std::fs::create_dir_all(d).is_ok())
            && std::fs::write(&tmp, text).is_ok()
            && std::fs::rename(&tmp, &file).is_ok();
        if !ok {
            warn!("couldn't save the paired phones");
        }
    }

    /// Whether the player left the remote on (it starts with the app then).
    pub fn is_on(&self) -> bool {
        self.file("on").is_some_and(|f| f.exists())
    }

    pub fn set_on(&self, on: bool) {
        let Some(file) = self.file("on") else { return };
        let _ = if on {
            file.parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&file, b"on"))
        } else {
            std::fs::remove_file(&file).or(Ok(()))
        };
    }
}

// ----- the server ------------------------------------------------------------------

#[derive(Debug)]
struct Shared {
    code: Mutex<String>,
    /// Web-page sessions.
    sessions: Mutex<Vec<String>>,
    /// App phones (kept on disk).
    phones: Mutex<Vec<Phone>>,
    store: Store,
    fingerprint: String,
    tls: Arc<rustls::ServerConfig>,
}

fn locked<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Shared {
    fn authed(&self, cookie: Option<&str>) -> bool {
        let Some(c) = cookie else { return false };
        locked(&self.sessions).iter().any(|s| same(s, c))
    }

    /// Uses up the pairing code (a new one replaces it). False if it's not the current one.
    fn use_code(&self, code: &str) -> bool {
        let mut current = locked(&self.code);
        if !same(&current, code) {
            return false;
        }
        *current = token();
        true
    }

    /// A web-page phone's new session, for the pairing code.
    fn pair(&self, code: &str) -> Option<String> {
        if !self.use_code(code) {
            return None;
        }
        let session = token();
        let mut sessions = locked(&self.sessions);
        if sessions.len() >= MAX_DEVICES {
            sessions.remove(0);
        }
        sessions.push(session.clone());
        Some(session)
    }

    /// An app phone's new token, for the pairing code.
    fn pair_app(&self, req: &PairRequest) -> Option<String> {
        if !self.use_code(&req.code) {
            return None;
        }
        let (token, id) = (token(), token()[..8].to_string());
        let name: String = req
            .name
            .trim()
            .chars()
            .filter(|c| !c.is_control())
            .take(40)
            .collect();
        let mut phones = locked(&self.phones);
        if phones.len() >= MAX_PHONES
            && let Some(oldest) = phones
                .iter()
                .enumerate()
                .min_by_key(|(_, p)| p.last_seen)
                .map(|(i, _)| i)
        {
            phones.remove(oldest);
        }
        let t = now();
        phones.push(Phone {
            id,
            name: if name.is_empty() {
                "Phone".into()
            } else {
                name
            },
            token_sha256: sha256_hex(token.as_bytes()),
            added: t,
            last_seen: t,
        });
        self.store.save_phones(&phones);
        Some(token)
    }

    /// The app phone sending `bearer`, if paired (and notes that it was used).
    fn phone(&self, bearer: Option<&str>) -> Option<String> {
        let hash = sha256_hex(bearer?.as_bytes());
        let mut phones = locked(&self.phones);
        let p = phones.iter_mut().find(|p| same(&p.token_sha256, &hash))?;
        let id = p.id.clone();
        let t = now();
        if t.saturating_sub(p.last_seen) >= SEEN_EVERY_SECS {
            p.last_seen = t;
            self.store.save_phones(&phones);
        } else {
            p.last_seen = t;
        }
        Some(id)
    }

    fn unpair(&self, id: &str) -> bool {
        let mut phones = locked(&self.phones);
        let before = phones.len();
        phones.retain(|p| p.id != id);
        let gone = phones.len() != before;
        if gone {
            self.store.save_phones(&phones);
        }
        gone
    }
}

/// A running remote. Dropping it stops the server (app phones stay paired).
#[derive(Debug)]
pub struct Server {
    addr: SocketAddr,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl Server {
    /// Listens on `bind:port` (`port` 0 picks a free one; if `port` is taken,
    /// a free one is used instead). `public` is the address put in the link.
    pub fn start(
        backend: Arc<dyn Backend>,
        bind: IpAddr,
        port: u16,
        public: IpAddr,
        store: Store,
    ) -> std::io::Result<Self> {
        let io = |e: String| std::io::Error::other(e);
        let identity = store.identity().map_err(io)?;
        let tls = identity.server_config().map_err(|e| io(e.to_string()))?;
        let listener = TcpListener::bind((bind, port)).or_else(|_| TcpListener::bind((bind, 0)))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let shared = Arc::new(Shared {
            code: Mutex::new(token()),
            sessions: Mutex::default(),
            phones: Mutex::new(store.phones()),
            store,
            fingerprint: identity.fingerprint(),
            tls,
        });
        let stop = Arc::new(AtomicBool::new(false));
        let mut threads = Vec::new();
        let (s, st) = (Arc::clone(&shared), Arc::clone(&stop));
        threads.push(
            thread::Builder::new()
                .name("jb-remote".into())
                .spawn(move || serve(&listener, &s, &backend, &st))?,
        );
        // Being found by paired phones when this PC's address changed (best effort).
        match UdpSocket::bind((bind, port)) {
            Ok(udp) => {
                let (fp, st) = (shared.fingerprint.clone(), Arc::clone(&stop));
                threads.push(
                    thread::Builder::new()
                        .name("jb-remote-find".into())
                        .spawn(move || answer_finders(&udp, &fp, port, &st))?,
                );
            }
            Err(e) => warn!("phone remote: paired phones can't look for this PC ({e})"),
        }
        info!(port, "phone remote started");
        Ok(Self {
            addr: SocketAddr::new(public, port),
            shared,
            stop,
            threads,
        })
    }

    /// The link for the QR code (holds the one-time pairing code).
    pub fn pair_url(&self) -> String {
        PairLink {
            addr: self.addr,
            code: locked(&self.shared.code).clone(),
            fingerprint: self.shared.fingerprint.clone(),
        }
        .to_url()
    }

    pub fn address(&self) -> String {
        format!("http://{}/", self.addr)
    }

    /// Phones that can use the remote now: web pages and apps.
    pub fn devices(&self) -> usize {
        locked(&self.shared.sessions).len() + locked(&self.shared.phones).len()
    }

    /// The app phones paired with this PC, newest first.
    pub fn phones(&self) -> Vec<PhoneInfo> {
        let mut list: Vec<PhoneInfo> = locked(&self.shared.phones)
            .iter()
            .map(|p| PhoneInfo {
                id: p.id.clone(),
                name: p.name.clone(),
                added: p.added,
                last_seen: p.last_seen,
            })
            .collect();
        list.sort_by_key(|p| std::cmp::Reverse(p.added));
        list
    }

    /// Unpairs one app phone.
    pub fn unpair(&self, id: &str) -> bool {
        self.shared.unpair(id)
    }

    /// Unpairs every phone (web pages and apps) and makes a new code.
    pub fn forget_devices(&self) {
        locked(&self.shared.sessions).clear();
        let mut phones = locked(&self.shared.phones);
        phones.clear();
        self.shared.store.save_phones(&phones);
        *locked(&self.shared.code) = token();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
        info!("phone remote stopped");
    }
}

fn answer_finders(udp: &UdpSocket, fingerprint: &str, port: u16, stop: &AtomicBool) {
    let _ = udp.set_read_timeout(Some(Duration::from_millis(200)));
    let mut buf = [0u8; 64];
    while !stop.load(Ordering::SeqCst) {
        if let Ok((n, from)) = udp.recv_from(&mut buf)
            && let Some(reply) = jb_remote::discover::answer(&buf[..n], fingerprint, port)
        {
            let _ = udp.send_to(&reply, from);
        }
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
                        .name("jb-remote-conn".into())
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

    fn bearer(&self) -> Option<&str> {
        self.header("authorization")?.strip_prefix("Bearer ")
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
        "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width,initial-scale=1\"><title>JuiceBox Trainer</title><body style=\"font-family:system-ui;background:#0e111a;color:#eceef6;padding:24px\"><p>{body}</p>"
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

fn error(status: &'static str, message: &str) -> Response {
    json(status, &serde_json::json!({ "error": message }))
}

/// The web page (plain HTTP).
fn route(req: &Request, shared: &Shared, backend: &dyn Backend) -> Response {
    if !host_ok(req.header("host")) {
        return text(
            "403 Forbidden",
            "Open the remote by the PC's address shown in JuiceBox Trainer.",
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
                "This pairing code was already used or is out of date. Scan the QR code in JuiceBox Trainer again.",
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
            "Not paired. In JuiceBox Trainer, open Phone remote and scan the QR code with this phone.",
        ),
        ("GET", "/app.js") => Response {
            status: "200 OK",
            content_type: "text/javascript; charset=utf-8",
            body: SCRIPT.as_bytes().to_vec(),
            extra: Vec::new(),
        },
        ("GET", "/api/state") if authed => json("200 OK", &backend.view()),
        ("POST", "/api/action") if authed => {
            if req.header("x-juicebox") != Some("1") {
                return error("403 Forbidden", "missing header");
            }
            act(req, backend)
        }
        (_, "/api/state" | "/api/action") => error("401 Unauthorized", "not paired"),
        (_, p) if p.starts_with("/app/") => error(
            "403 Forbidden",
            "the app talks to JuiceBox Trainer over a secure connection only",
        ),
        _ => text("404 Not Found", "Not found."),
    }
}

fn act(req: &Request, backend: &dyn Backend) -> Response {
    match serde_json::from_slice::<Action>(&req.body) {
        Ok(a) => match backend.act(&a) {
            Ok(()) => json("200 OK", &backend.view()),
            Err(e) => error("409 Conflict", &e),
        },
        Err(_) => error("400 Bad Request", "bad request"),
    }
}

/// The JuiceBox Remote app (pinned TLS only).
fn app_route(req: &Request, shared: &Shared, backend: &dyn Backend) -> Response {
    if (req.method.as_str(), req.path.as_str()) == ("POST", "/app/pair") {
        let Ok(pair) = serde_json::from_slice::<PairRequest>(&req.body) else {
            return error("400 Bad Request", "bad request");
        };
        return match shared.pair_app(&pair) {
            Some(token) => json(
                "200 OK",
                &serde_json::to_value(PairReply {
                    token,
                    pc: pc_name(),
                })
                .unwrap_or_default(),
            ),
            None => error(
                "403 Forbidden",
                "this QR code was already used or is out of date: scan the new one on your PC",
            ),
        };
    }
    let Some(phone) = shared.phone(req.bearer()) else {
        return error(
            "401 Unauthorized",
            "this phone isn't paired with the PC any more",
        );
    };
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/app/state") => json("200 OK", &backend.view()),
        ("POST", "/app/action") => act(req, backend),
        ("POST", "/app/preset") => match serde_json::from_slice::<PresetRequest>(&req.body) {
            Ok(p) => match backend.preset(&p.name) {
                Ok(()) => json("200 OK", &backend.view()),
                Err(e) => error("409 Conflict", &e),
            },
            Err(_) => error("400 Bad Request", "bad request"),
        },
        ("POST", "/app/unpair") => {
            shared.unpair(&phone);
            json("200 OK", &serde_json::json!({}))
        }
        _ => error("404 Not Found", "not found"),
    }
}

/// Reads one request; `None` if the phone hung up or sent too much.
fn read_request(stream: &mut impl Read) -> std::io::Result<Option<Request>> {
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            return Ok(None);
        }
        raw.extend_from_slice(&buf[..n]);
        if raw.len() > MAX_REQUEST {
            return Ok(None);
        }
        if let Some((mut req, total)) = parse(&raw)
            && raw.len() >= total
        {
            req.body = raw[total - req.content_length()..total].to_vec();
            return Ok(Some(req));
        }
    }
}

fn write_response(stream: &mut impl Write, r: &Response) -> std::io::Result<()> {
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

/// The first byte of a TLS connection (a handshake record).
const TLS_HANDSHAKE: u8 = 0x16;

fn handle(stream: TcpStream, shared: &Shared, backend: &dyn Backend) -> std::io::Result<()> {
    // Accepted sockets inherit the listener's non-blocking mode on Windows.
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut first = [0u8; 1];
    if stream.peek(&mut first)? == 0 {
        return Ok(());
    }
    if first[0] == TLS_HANDSHAKE {
        let conn = rustls::ServerConnection::new(Arc::clone(&shared.tls))
            .map_err(std::io::Error::other)?;
        let mut tls = rustls::StreamOwned::new(conn, stream);
        let Some(req) = read_request(&mut tls)? else {
            return Ok(());
        };
        write_response(&mut tls, &app_route(&req, shared, backend))?;
        tls.conn.send_close_notify();
        tls.flush()
    } else {
        let mut stream = stream;
        let Some(req) = read_request(&mut stream)? else {
            return Ok(());
        };
        write_response(&mut stream, &route(&req, shared, backend))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        acts: Mutex<Vec<Action>>,
        presets: Mutex<Vec<String>>,
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
        fn preset(&self, name: &str) -> Result<(), String> {
            self.presets.lock().unwrap().push(name.into());
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

    /// A folder of its own under the system temp folder.
    fn temp_store() -> (Store, PathBuf) {
        let dir = std::env::temp_dir().join(format!("jb-remote-test-{}", token()));
        (Store::at(dir.clone()), dir)
    }

    fn start(fake: &Arc<Fake>, store: Store) -> Server {
        let local: IpAddr = "127.0.0.1".parse().unwrap();
        Server::start(fake.clone(), local, 0, local, store).unwrap()
    }

    #[test]
    fn parses_requests() {
        let raw = b"POST /api/action?x=1 HTTP/1.1\r\nHost: 1.2.3.4:5\r\nCookie: a=b; ot=tok\r\nAuthorization: Bearer abc\r\nContent-Length: 2\r\n\r\n{}";
        let (r, total) = parse(raw).unwrap();
        assert_eq!(
            (r.method.as_str(), r.path.as_str(), r.query.as_str()),
            ("POST", "/api/action", "x=1")
        );
        assert_eq!(r.cookie("ot"), Some("tok"));
        assert_eq!(r.bearer(), Some("abc"));
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
        let server = start(&fake, Store { dir: None });
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
        // The app's routes aren't served without TLS.
        assert!(
            request(addr, &format!("GET /app/state HTTP/1.1\r\n{host}\r\n\r\n"))
                .starts_with("HTTP/1.1 403")
        );

        let url = server.pair_url();
        let link = PairLink::parse(&url).unwrap();
        assert_eq!(link.fingerprint, server.shared.fingerprint);
        let code = link.code;
        assert!(
            request(
                addr,
                &format!("GET /pair?code=wrong HTTP/1.1\r\n{host}\r\n\r\n")
            )
            .starts_with("HTTP/1.1 403")
        );
        // The page pairs with the link as the QR code has it (fingerprint and all).
        let path = &url[url.find("/pair").unwrap()..];
        let paired = request(addr, &format!("GET {path} HTTP/1.1\r\n{host}\r\n\r\n"));
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
        assert!(post("X-JuiceBox: 1\r\n").starts_with("HTTP/1.1 200"));
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

    fn app(
        server: &Server,
        fp: &str,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> jb_remote::client::Reply {
        jb_remote::client::request(
            server.addr,
            fp,
            method,
            path,
            token,
            body.as_ref(),
            Duration::from_secs(5),
        )
        .unwrap()
    }

    #[test]
    fn the_app_pairs_once_and_is_remembered() {
        let fake = Arc::new(Fake::default());
        let (store, dir) = temp_store();
        let server = start(&fake, store.clone());
        let link = PairLink::parse(&server.pair_url()).unwrap();
        let fp = link.fingerprint.clone();

        // Not paired, or a made-up token: nothing.
        assert_eq!(
            app(&server, &fp, "GET", "/app/state", None, None).status,
            401
        );
        assert_eq!(
            app(&server, &fp, "GET", "/app/state", Some("x"), None).status,
            401
        );
        let wrong = serde_json::json!({ "code": "0".repeat(32), "name": "Phone" });
        assert_eq!(
            app(&server, &fp, "POST", "/app/pair", None, Some(wrong)).status,
            403
        );

        let pair = serde_json::json!({ "code": link.code, "name": "Jack's phone\u{7}" });
        let reply = app(&server, &fp, "POST", "/app/pair", None, Some(pair.clone()));
        assert_eq!(reply.status, 200);
        let reply: PairReply = serde_json::from_slice(&reply.body).unwrap();
        // The code works once.
        assert_eq!(
            app(&server, &fp, "POST", "/app/pair", None, Some(pair)).status,
            403
        );
        let phones = server.phones();
        assert_eq!(phones.len(), 1);
        assert_eq!(phones[0].name, "Jack's phone");
        assert_eq!(server.devices(), 1);
        // Only the token's hash is on disk.
        let saved = std::fs::read_to_string(dir.join("phones.json")).unwrap();
        assert!(
            !saved.contains(&reply.token) && saved.contains(&sha256_hex(reply.token.as_bytes()))
        );

        let t = Some(reply.token.as_str());
        let state = app(&server, &fp, "GET", "/app/state", t, None);
        assert_eq!(state.json().unwrap()["pack"]["game"], "Testbed");
        let toggle = serde_json::json!({ "id": "inf_health", "op": "toggle", "on": true });
        assert_eq!(
            app(&server, &fp, "POST", "/app/action", t, Some(toggle)).status,
            200
        );
        let bad = serde_json::json!({ "id": "bad", "op": "toggle" });
        let r = app(&server, &fp, "POST", "/app/action", t, Some(bad));
        assert_eq!(
            (r.status, r.json().unwrap()["error"].as_str()),
            (409, Some("no cheat 'bad'"))
        );
        let preset = serde_json::json!({ "name": "Boss fight" });
        assert_eq!(
            app(&server, &fp, "POST", "/app/preset", t, Some(preset)).status,
            200
        );
        assert_eq!(fake.presets.lock().unwrap().as_slice(), ["Boss fight"]);

        // The PC app restarts: same certificate, the phone still works.
        drop(server);
        let server = start(&fake, store.clone());
        assert_eq!(PairLink::parse(&server.pair_url()).unwrap().fingerprint, fp);
        assert_eq!(app(&server, &fp, "GET", "/app/state", t, None).status, 200);

        // The phone forgets the PC: it's unpaired here too.
        assert_eq!(
            app(&server, &fp, "POST", "/app/unpair", t, None).status,
            200
        );
        assert!(server.phones().is_empty());
        assert_eq!(app(&server, &fp, "GET", "/app/state", t, None).status, 401);
        drop(server);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unpairing_from_the_pc_and_finding_it() {
        let fake = Arc::new(Fake::default());
        let (store, dir) = temp_store();
        let server = start(&fake, store.clone());
        let link = PairLink::parse(&server.pair_url()).unwrap();
        let fp = link.fingerprint.clone();
        let pair = serde_json::json!({ "code": link.code, "name": "" });
        let token: PairReply =
            serde_json::from_slice(&app(&server, &fp, "POST", "/app/pair", None, Some(pair)).body)
                .unwrap();
        assert_eq!(server.phones()[0].name, "Phone");
        assert!(server.unpair(&server.phones()[0].id));
        assert_eq!(
            app(&server, &fp, "GET", "/app/state", Some(&token.token), None).status,
            401
        );

        // A paired phone asking the network finds this PC's port.
        let found = jb_remote::discover::find(
            &fp,
            server.addr.port(),
            Some(server.addr.ip()),
            Duration::from_secs(5),
        );
        assert_eq!(found.map(|a| a.port()), Some(server.addr.port()));
        assert_eq!(
            jb_remote::discover::find(
                &"0".repeat(64),
                server.addr.port(),
                Some(server.addr.ip()),
                Duration::from_millis(500)
            ),
            None
        );

        // On/off is remembered.
        assert!(!store.is_on());
        store.set_on(true);
        assert!(store.is_on());
        store.set_on(false);
        assert!(!store.is_on());
        drop(server);
        let _ = std::fs::remove_dir_all(dir);
    }
}

// ----- app glue ------------------------------------------------------------------

/// The remote's place in the app: off until started.
#[derive(Debug)]
pub struct RemoteState {
    server: Mutex<Option<Server>>,
    store: Store,
}

impl Default for RemoteState {
    fn default() -> Self {
        Self {
            server: Mutex::default(),
            store: Store::default_location(),
        }
    }
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

    fn preset(&self, name: &str) -> Result<(), String> {
        use tauri::{Emitter, Manager};
        let state = self.app.state::<crate::commands::AppState>();
        let r = crate::commands::apply_preset(&state, name).map_err(|e| e.message);
        let _ = self.app.emit("trainer-changed", "remote");
        let (_, waiting) = r?;
        if let Some(w) = waiting {
            crate::commands::retry_in_background(self.app.clone(), w);
        }
        Ok(())
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
    /// Phones with the JuiceBox Remote app.
    pub phones: Vec<PhoneInfo>,
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
        phones: server.phones(),
    }
}

impl RemoteState {
    /// Stops the remote (app exit). It starts again next time if it was on.
    pub fn stop(&self) {
        lock(self).take();
    }

    /// Starts the remote if the player left it on (app start).
    pub fn start_if_on(&self, app: &tauri::AppHandle) {
        if self.store.is_on()
            && let Err(e) = self.start(app)
        {
            warn!("phone remote didn't start: {}", e.message);
        }
    }

    fn start(&self, app: &tauri::AppHandle) -> crate::commands::CmdResult<RemoteStatus> {
        let mut server = lock(self);
        if server.is_none() {
            let ip = lan_ip().ok_or_else(|| {
                crate::commands::CommandError::new(
                    "remote",
                    "this PC isn't on a network, so a phone can't reach it",
                )
            })?;
            let backend: Arc<dyn Backend> = Arc::new(AppBackend { app: app.clone() });
            *server = Some(
                Server::start(
                    backend,
                    IpAddr::from([0, 0, 0, 0]),
                    DEFAULT_PORT,
                    ip,
                    self.store.clone(),
                )
                .map_err(|e| {
                    crate::commands::CommandError::new(
                        "remote",
                        format!("couldn't start the remote: {e}"),
                    )
                })?,
            );
        }
        Ok(status_of(server.as_ref().expect("started above")))
    }
}

fn lock(state: &RemoteState) -> std::sync::MutexGuard<'_, Option<Server>> {
    locked(&state.server)
}

/// Starts the remote on the local network and keeps it on at the next start (Windows may ask
/// to allow it through the firewall: allow it on private networks).
#[tauri::command]
pub async fn remote_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, RemoteState>,
) -> crate::commands::CmdResult<RemoteStatus> {
    let status = state.start(&app)?;
    state.store.set_on(true);
    Ok(status)
}

#[tauri::command]
pub fn remote_stop(state: tauri::State<'_, RemoteState>) {
    lock(&state).take();
    state.store.set_on(false);
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

/// Unpairs one phone with the app.
#[tauri::command]
pub fn remote_unpair(id: String, state: tauri::State<'_, RemoteState>) -> Option<RemoteStatus> {
    let server = lock(&state);
    server.as_ref().map(|s| {
        s.unpair(&id);
        status_of(s)
    })
}
