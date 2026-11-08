//! Sharing a pack with the community list on GitHub, done by the app.
//!
//! The player signs in once with GitHub's device flow (they type a short code
//! on github.com; OpenTrainer never sees their password). The token, scoped to
//! public repositories, is kept in Windows Credential Manager. Sharing then
//! copies the community repository to the player's account (a fork), adds
//! `packs/<id>/pack.json` on a new branch and opens a pull request. A
//! maintainer checks it before it appears in the list.
//!
//! Only valid packs of the player's own are shared, and packs converted from
//! a Cheat Engine table only when the player says they may share it.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use base64::Engine as _;
use ot_packs::store;
use serde::Serialize;
use serde_json::{Value, json};
use tauri::State;

use crate::commands::{CmdResult, CommandError};

/// The community repository (`owner/name`).
pub const COMMUNITY_REPO: &str = "jackuri/opentrainer-packs";
const COMMUNITY_OWNER: &str = "jackuri";
/// OpenTrainer's GitHub OAuth app (public, not a secret; owned by jackuri,
/// device flow on, tokens don't expire). `OT_GITHUB_CLIENT_ID` overrides it
/// for testing.
const CLIENT_ID: &str = "Ov23liChvIJtZaXBnMC4";
/// Where the player types the code. Only this page is opened.
pub const DEVICE_PAGE: &str = "https://github.com/login/device";
/// Pull requests the app may open in the browser start with this.
const PR_PREFIX: &str = "https://github.com/jackuri/opentrainer-packs/pull/";
/// Write access to public repositories: needed to fork and open the request.
const SCOPE: &str = "public_repo";
const CREDENTIAL: &str = "OpenTrainer/github";
const MAX_NOTE: usize = 500;
/// Author text of packs converted from a Cheat Engine table (`ot_packs::ct`).
const TABLE_AUTHOR: &str = "Cheat Engine table (imported)";

fn client_id() -> Option<String> {
    std::env::var("OT_GITHUB_CLIENT_ID")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| (!CLIENT_ID.is_empty()).then(|| CLIENT_ID.to_string()))
}

#[derive(Debug, Default)]
pub struct GithubState {
    /// The sign-in in progress: device code and poll interval (seconds).
    device: Mutex<Option<(String, u64)>>,
    cancel: AtomicBool,
    /// The signed-in account, once looked up.
    login: Mutex<Option<String>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn err(kind: &'static str, message: impl Into<String>) -> CommandError {
    CommandError::new(kind, message.into())
}

// ----- token storage (Windows Credential Manager) ---------------------------

mod vault {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::Security::Credentials::{
        CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW,
        CredFree, CredReadW, CredWriteW,
    };
    use windows::core::{HSTRING, PWSTR};

    pub fn save(target: &str, secret: &str) -> Result<(), String> {
        let target = HSTRING::from(target);
        let user = HSTRING::from("github");
        let mut blob = secret.as_bytes().to_vec();
        let cred = CREDENTIALW {
            Flags: CRED_FLAGS(0),
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target.as_ptr().cast_mut()),
            Comment: PWSTR::null(),
            LastWritten: FILETIME::default(),
            CredentialBlobSize: u32::try_from(blob.len()).map_err(|e| e.to_string())?,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: std::ptr::null_mut(),
            TargetAlias: PWSTR::null(),
            UserName: PWSTR(user.as_ptr().cast_mut()),
        };
        // SAFETY: every pointer in `cred` points into `target`, `user` and
        // `blob`, which live until after the call; Windows copies the data.
        unsafe { CredWriteW(&cred, 0) }.map_err(|e| e.to_string())
    }

    pub fn load(target: &str) -> Option<String> {
        let target = HSTRING::from(target);
        let mut out: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `target` is a valid wide string; `out` receives a buffer
        // owned by Windows, freed with CredFree below.
        unsafe { CredReadW(&target, CRED_TYPE_GENERIC, None, &mut out) }.ok()?;
        // SAFETY: CredReadW succeeded, so `out` points to a valid CREDENTIALW
        // whose blob holds `CredentialBlobSize` bytes.
        let secret = unsafe {
            let c = &*out;
            let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
            let s = String::from_utf8(bytes.to_vec()).ok();
            CredFree(out.cast());
            s
        };
        secret.filter(|s| !s.is_empty())
    }

    pub fn delete(target: &str) {
        let target = HSTRING::from(target);
        // SAFETY: `target` is a valid wide string for the call.
        let _ = unsafe { CredDeleteW(&target, CRED_TYPE_GENERIC, None) };
    }
}

// ----- HTTP -----------------------------------------------------------------

/// GitHub's two addresses (tests point them at a local server).
#[derive(Debug, Clone)]
pub struct Hub {
    web: String,
    api: String,
    client: reqwest::Client,
}

impl Hub {
    fn github() -> Result<Self, CommandError> {
        Self::new("https://github.com", "https://api.github.com")
    }

    fn new(web: &str, api: &str) -> Result<Self, CommandError> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder()
            .user_agent(concat!("OpenTrainer/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| err("io", e.to_string()))?;
        Ok(Self {
            web: web.trim_end_matches('/').into(),
            api: api.trim_end_matches('/').into(),
            client,
        })
    }

    /// A form POST to github.com (sign-in), answered with JSON.
    async fn form(&self, path: &str, body: String) -> Result<Value, CommandError> {
        let resp = self
            .client
            .post(format!("{}{path}", self.web))
            .header("Accept", "application/json")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(offline)?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(offline)?;
        if !status.is_success() {
            return Err(err(
                "github",
                format!("GitHub answered {status} during sign-in"),
            ));
        }
        serde_json::from_slice(&bytes).map_err(|e| err("github", e.to_string()))
    }

    /// A call to the REST API. Returns the status and the JSON body (or Null).
    async fn call(
        &self,
        token: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(u16, Value), CommandError> {
        let mut req = self
            .client
            .request(method, format!("{}{path}", self.api))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("Authorization", format!("Bearer {token}"));
        if let Some(b) = body {
            req = req
                .header("Content-Type", "application/json")
                .body(b.to_string());
        }
        let resp = req.send().await.map_err(offline)?;
        let status = resp.status().as_u16();
        let bytes = resp.bytes().await.map_err(offline)?;
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if status == 401 {
            return Err(err(
                "signedOut",
                "GitHub says the sign-in has expired; sign in again",
            ));
        }
        Ok((status, value))
    }

    /// Like [`Hub::call`], failing on anything but 2xx.
    async fn ok(
        &self,
        token: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<&Value>,
        doing: &str,
    ) -> Result<Value, CommandError> {
        let (status, v) = self.call(token, method, path, body).await?;
        if (200..300).contains(&status) {
            Ok(v)
        } else {
            let why = v["message"].as_str().unwrap_or("no details");
            Err(err(
                "github",
                format!("GitHub refused {doing} ({status}: {why})"),
            ))
        }
    }
}

fn offline(e: reqwest::Error) -> CommandError {
    if e.is_connect() || e.is_timeout() {
        err(
            "offline",
            "couldn't reach GitHub; check your internet connection",
        )
    } else {
        err("github", e.to_string())
    }
}

// ----- sign-in --------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubStatus {
    /// This build knows OpenTrainer's GitHub app.
    pub configured: bool,
    pub signed_in: bool,
    pub login: Option<String>,
}

#[tauri::command]
pub async fn github_status(state: State<'_, GithubState>) -> CmdResult<GithubStatus> {
    let token = vault::load(CREDENTIAL);
    let mut login = lock(&state.login).clone();
    if let (Some(t), None) = (&token, &login) {
        // Offline is fine: still signed in, just no name yet.
        if let Ok(hub) = Hub::github()
            && let Ok((200, v)) = hub.call(t, reqwest::Method::GET, "/user", None).await
        {
            login = v["login"].as_str().map(str::to_string);
            lock(&state.login).clone_from(&login);
        }
    }
    Ok(GithubStatus {
        configured: client_id().is_some(),
        signed_in: token.is_some(),
        login,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    /// What the player types on the device page, e.g. `ABCD-1234`.
    pub user_code: String,
    pub expires_in: u64,
}

async fn start_device(hub: &Hub, client: &str) -> Result<(DeviceCode, String, u64), CommandError> {
    let v = hub
        .form(
            "/login/device/code",
            format!("client_id={client}&scope={SCOPE}"),
        )
        .await?;
    let (Some(code), Some(device)) = (v["user_code"].as_str(), v["device_code"].as_str()) else {
        let why = v["error_description"].as_str().unwrap_or("no code");
        return Err(err(
            "github",
            format!("GitHub didn't give a sign-in code ({why})"),
        ));
    };
    let interval = v["interval"].as_u64().unwrap_or(5).clamp(1, 60);
    Ok((
        DeviceCode {
            user_code: code.into(),
            expires_in: v["expires_in"].as_u64().unwrap_or(900),
        },
        device.into(),
        interval,
    ))
}

/// Starts signing in: returns the code the player types on GitHub's page.
#[tauri::command]
pub async fn github_sign_in(state: State<'_, GithubState>) -> CmdResult<DeviceCode> {
    let client =
        client_id().ok_or_else(|| err("notConfigured", "sharing isn't set up in this version"))?;
    let (code, device, interval) = start_device(&Hub::github()?, &client).await?;
    *lock(&state.device) = Some((device, interval));
    state.cancel.store(false, Ordering::SeqCst);
    Ok(code)
}

/// Opens GitHub's code page in the browser.
#[tauri::command]
pub async fn github_open_device_page() -> CmdResult<()> {
    open(DEVICE_PAGE)
}

/// Polls until the player approved (or refused, or the code expired).
async fn wait_for_token(
    hub: &Hub,
    client: &str,
    device: &str,
    mut interval: u64,
    cancel: &AtomicBool,
    max_polls: usize,
) -> Result<String, CommandError> {
    for _ in 0..max_polls {
        tokio_sleep(Duration::from_secs(interval)).await;
        if cancel.load(Ordering::SeqCst) {
            return Err(err("cancelled", "sign-in cancelled"));
        }
        let v = hub
            .form(
                "/login/oauth/access_token",
                format!(
                    "client_id={client}&device_code={device}&grant_type=urn:ietf:params:oauth:grant-type:device_code"
                ),
            )
            .await?;
        if let Some(token) = v["access_token"].as_str() {
            return Ok(token.to_string());
        }
        match v["error"].as_str() {
            Some("authorization_pending") => {}
            Some("slow_down") => {
                interval = v["interval"].as_u64().unwrap_or(interval + 5).clamp(1, 60)
            }
            Some("expired_token") => return Err(err("expired", "the code expired; start again")),
            Some("access_denied") => {
                return Err(err("denied", "you didn't allow OpenTrainer on GitHub"));
            }
            other => {
                return Err(err(
                    "github",
                    format!("GitHub sign-in failed ({})", other.unwrap_or("no answer")),
                ));
            }
        }
    }
    Err(err("expired", "the code expired; start again"))
}

/// Sleeps without blocking the app's async threads.
async fn tokio_sleep(d: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}

/// Waits for the player to approve on GitHub, then keeps the sign-in.
/// Returns the account name.
#[tauri::command]
pub async fn github_sign_in_wait(state: State<'_, GithubState>) -> CmdResult<String> {
    let client =
        client_id().ok_or_else(|| err("notConfigured", "sharing isn't set up in this version"))?;
    let (device, interval) = lock(&state.device)
        .clone()
        .ok_or_else(|| err("badRequest", "start signing in first"))?;
    let hub = Hub::github()?;
    let token = wait_for_token(&hub, &client, &device, interval, &state.cancel, 400).await?;
    *lock(&state.device) = None;
    vault::save(CREDENTIAL, &token)
        .map_err(|e| err("io", format!("couldn't keep the sign-in: {e}")))?;
    let (_, v) = hub
        .call(&token, reqwest::Method::GET, "/user", None)
        .await?;
    let login = v["login"].as_str().unwrap_or("you").to_string();
    *lock(&state.login) = Some(login.clone());
    Ok(login)
}

#[tauri::command]
pub fn github_sign_in_cancel(state: State<'_, GithubState>) {
    state.cancel.store(true, Ordering::SeqCst);
    *lock(&state.device) = None;
}

/// Forgets the sign-in on this PC.
#[tauri::command]
pub fn github_sign_out(state: State<'_, GithubState>) {
    vault::delete(CREDENTIAL);
    *lock(&state.login) = None;
}

// ----- sharing ----------------------------------------------------------------

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Shared {
    /// The pull request on GitHub.
    pub url: String,
    pub number: u64,
    /// The list already had a pack for this game; this replaces it.
    pub update: bool,
}

/// What the player said when sharing.
#[derive(Debug, Clone)]
pub struct ShareRequest {
    pub note: String,
    /// The player made the table this pack came from, or its author allows sharing it.
    pub table_ok: bool,
}

/// Checks a pack may be shared; returns the parsed pack.
pub fn shareable(text: &str, req: &ShareRequest) -> Result<ot_packs::Pack, CommandError> {
    let (pack, _) = ot_packs::parse(text)
        .map_err(|e| err("invalidPack", format!("the pack isn't valid: {e}")))?;
    if pack.cheats.is_empty() {
        return Err(err("invalidPack", "the pack has no cheats"));
    }
    if pack.meta.author == TABLE_AUTHOR && !req.table_ok {
        return Err(err(
            "needsPermission",
            "this pack came from someone's Cheat Engine table: share it only if you made the table or its author allows it",
        ));
    }
    Ok(pack)
}

fn pr_text(
    pack: &ot_packs::Pack,
    req: &ShareRequest,
    update: bool,
    sharer: &str,
) -> (String, String) {
    let g = &pack.game;
    let title = format!("{} pack: {}", if update { "Update" } else { "Add" }, g.name);
    let names: Vec<&str> = pack
        .cheats
        .iter()
        .take(20)
        .map(|c| c.name.as_str())
        .collect();
    let more = pack.cheats.len().saturating_sub(names.len());
    let note: String = req.note.trim().chars().take(MAX_NOTE).collect();
    let mut body = format!(
        "Shared from OpenTrainer {} by @{sharer}.\n\n\
         - **Game:** {} (`{}`{})\n\
         - **Cheats ({}):** {}{}\n\
         - **Made by:** {}{}\n\
         - **Tested on:** {}\n\n\
         The sharer says every cheat works in a single-player game.",
        env!("CARGO_PKG_VERSION"),
        g.name,
        g.exe,
        g.arch.map_or(String::new(), |a| format!(
            ", {}",
            if a == ot_packs::schema::PackArch::X86 {
                "32-bit"
            } else {
                "64-bit"
            }
        )),
        pack.cheats.len(),
        names.join(", "),
        if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        },
        pack.meta.author,
        if pack.meta.is_ai() {
            " (made with the AI helper)"
        } else {
            ""
        },
        if note.is_empty() {
            "not said"
        } else {
            note.as_str()
        },
    );
    if pack.meta.author == TABLE_AUTHOR {
        body.push_str(" It was converted from a Cheat Engine table; the sharer says they made the table or its author allows sharing it.");
    }
    if update {
        body.push_str("\n\nThe list already has a pack for this game: this replaces it.");
    }
    (title, body)
}

/// Does the whole GitHub part: fork, branch, file, pull request.
pub async fn share(
    hub: &Hub,
    token: &str,
    text: &str,
    req: &ShareRequest,
    stamp: u64,
) -> Result<Shared, CommandError> {
    use reqwest::Method;
    let pack = shareable(text, req)?;
    let id = &pack.game.id;
    let file = format!("packs/{id}/pack.json");

    let me = hub
        .ok(token, Method::GET, "/user", None, "to say who you are")
        .await?;
    let login = me["login"]
        .as_str()
        .ok_or_else(|| err("github", "GitHub didn't say who you are"))?
        .to_string();

    // Where the branch goes: the player's copy, or the list itself for its owner.
    let head_repo = if login.eq_ignore_ascii_case(COMMUNITY_OWNER) {
        COMMUNITY_REPO.to_string()
    } else {
        let fork = hub
            .ok(
                token,
                Method::POST,
                &format!("/repos/{COMMUNITY_REPO}/forks"),
                Some(&json!({ "default_branch_only": true })),
                "to copy the community list to your account",
            )
            .await?;
        let full = fork["full_name"]
            .as_str()
            .ok_or_else(|| err("github", "GitHub didn't say where the copy is"))?
            .to_string();
        // A new copy takes a moment to appear.
        let mut ready = false;
        for _ in 0..20 {
            if let Ok((200, _)) = hub
                .call(token, Method::GET, &format!("/repos/{full}"), None)
                .await
            {
                ready = true;
                break;
            }
            tokio_sleep(Duration::from_millis(1500)).await;
        }
        if !ready {
            return Err(err(
                "github",
                "GitHub is still making your copy of the list; try again in a minute",
            ));
        }
        // An older copy: bring it up to date (fine if it already is).
        let _ = hub
            .call(
                token,
                Method::POST,
                &format!("/repos/{full}/merge-upstream"),
                Some(&json!({ "branch": "main" })),
            )
            .await;
        full
    };

    let main = hub
        .ok(
            token,
            Method::GET,
            &format!("/repos/{COMMUNITY_REPO}/git/ref/heads/main"),
            None,
            "to read the list",
        )
        .await?;
    let sha = main["object"]["sha"]
        .as_str()
        .ok_or_else(|| err("github", "GitHub didn't give the list's version"))?;
    let (listed, _) = hub
        .call(
            token,
            Method::GET,
            &format!("/repos/{COMMUNITY_REPO}/contents/{file}"),
            None,
        )
        .await?;
    let update = listed == 200;

    let branch = format!("pack-{id}-{stamp}");
    hub.ok(
        token,
        Method::POST,
        &format!("/repos/{head_repo}/git/refs"),
        Some(&json!({ "ref": format!("refs/heads/{branch}"), "sha": sha })),
        "to start your change",
    )
    .await?;
    let (status, existing) = hub
        .call(
            token,
            Method::GET,
            &format!("/repos/{head_repo}/contents/{file}?ref={branch}"),
            None,
        )
        .await?;
    let mut put = json!({
        "message": format!("{} pack: {}", if update { "Update" } else { "Add" }, pack.game.name),
        "content": base64::engine::general_purpose::STANDARD.encode(text.as_bytes()),
        "branch": branch,
    });
    if status == 200
        && let Some(old) = existing["sha"].as_str()
    {
        put["sha"] = json!(old);
    }
    hub.ok(
        token,
        Method::PUT,
        &format!("/repos/{head_repo}/contents/{file}"),
        Some(&put),
        "to add the pack",
    )
    .await?;

    let (title, body) = pr_text(&pack, req, update, &login);
    let head = if head_repo == COMMUNITY_REPO {
        branch.clone()
    } else {
        format!("{login}:{branch}")
    };
    let pr = hub
        .ok(
            token,
            Method::POST,
            &format!("/repos/{COMMUNITY_REPO}/pulls"),
            Some(&json!({ "title": title, "head": head, "base": "main", "body": body, "maintainer_can_modify": true })),
            "to send the pack",
        )
        .await?;
    Ok(Shared {
        url: pr["html_url"].as_str().unwrap_or_default().to_string(),
        number: pr["number"].as_u64().unwrap_or(0),
        update,
    })
}

/// Shares one of the player's packs with the community list.
#[tauri::command]
pub async fn community_share(
    key: String,
    note: String,
    table_ok: bool,
    state: State<'_, GithubState>,
) -> CmdResult<Shared> {
    let found = store::all()
        .into_iter()
        .find(|f| f.key == key)
        .ok_or_else(|| err("notFound", format!("no pack '{key}'")))?;
    if matches!(found.source, store::Source::Bundled) {
        return Err(err("badRequest", "built-in examples are already shared"));
    }
    let text = found.text.map_err(|e| err("io", e.to_string()))?;
    let token =
        vault::load(CREDENTIAL).ok_or_else(|| err("signedOut", "sign in to GitHub first"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let result = share(
        &Hub::github()?,
        &token,
        &text,
        &ShareRequest { note, table_ok },
        stamp,
    )
    .await;
    if let Err(e) = &result
        && e.kind == "signedOut"
    {
        vault::delete(CREDENTIAL);
        *lock(&state.login) = None;
    }
    result
}

/// A community pull request page, and nothing else.
fn is_pr_url(url: &str) -> bool {
    url.strip_prefix(PR_PREFIX).is_some_and(|rest| {
        !rest.is_empty() && rest.len() < 10 && rest.chars().all(|c| c.is_ascii_digit())
    })
}

/// Opens a shared pack's pull request in the browser.
#[tauri::command]
pub async fn github_open_pr(url: String) -> CmdResult<()> {
    if !is_pr_url(&url) {
        return Err(err("badRequest", "not a community pack request"));
    }
    open(&url)
}

fn open(url: &str) -> CmdResult<()> {
    std::process::Command::new("explorer.exe")
        .arg(url)
        .spawn()
        .map_err(|e| err("io", format!("couldn't open the browser: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::Arc;

    type Seen = Arc<Mutex<Vec<(String, String, String)>>>;

    /// A fake GitHub: `route(method, path, body)` gives the status and JSON.
    fn fake(route: impl Fn(&str, &str, &str) -> (u16, String) + Send + 'static) -> (Hub, Seen) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen: Seen = Arc::default();
        let log = Arc::clone(&seen);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { return };
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                let mut parts = line.split_whitespace();
                let (method, path) = (
                    parts.next().unwrap_or("").to_string(),
                    parts.next().unwrap_or("").to_string(),
                );
                let mut len = 0;
                loop {
                    let mut h = String::new();
                    r.read_line(&mut h).unwrap();
                    if h.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; len];
                r.read_exact(&mut body).unwrap();
                let body = String::from_utf8_lossy(&body).to_string();
                let (status, out) = route(&method, &path, &body);
                lock(&log).push((method, path, body));
                let _ = write!(
                    s,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}",
                    out.len()
                );
            }
        });
        let base = format!("http://127.0.0.1:{port}");
        (Hub::new(&base, &base).unwrap(), seen)
    }

    fn pack_text(author: &str) -> String {
        let (mut pack, _) = ot_packs::parse(store::BUNDLED[0].1).unwrap();
        pack.game.id = "coin-quest".into();
        pack.game.name = "Coin Quest".into();
        pack.meta.author = author.into();
        serde_json::to_string_pretty(&pack).unwrap()
    }

    fn req(table_ok: bool) -> ShareRequest {
        ShareRequest {
            note: "Steam, v1.2".into(),
            table_ok,
        }
    }

    fn github_like(
        login: &'static str,
        listed: bool,
    ) -> impl Fn(&str, &str, &str) -> (u16, String) + Send + 'static {
        move |m: &str, p: &str, _b: &str| {
            match (m, p) {
            ("GET", "/user") => (200, json!({ "login": login }).to_string()),
            ("POST", "/repos/jackuri/opentrainer-packs/forks") => {
                (202, json!({ "full_name": format!("{login}/opentrainer-packs") }).to_string())
            }
            ("GET", p) if p == format!("/repos/{login}/opentrainer-packs") => (200, "{}".into()),
            ("POST", p) if p.ends_with("/merge-upstream") => (200, "{}".into()),
            ("GET", "/repos/jackuri/opentrainer-packs/git/ref/heads/main") => {
                (200, json!({ "object": { "sha": "abc123" } }).to_string())
            }
            ("GET", "/repos/jackuri/opentrainer-packs/contents/packs/coin-quest/pack.json") => {
                if listed { (200, json!({ "sha": "old" }).to_string()) } else { (404, "{}".into()) }
            }
            ("POST", p) if p.ends_with("/git/refs") => (201, "{}".into()),
            ("GET", p) if p.contains("/contents/packs/coin-quest/pack.json?ref=") => (404, "{}".into()),
            ("PUT", p) if p.ends_with("/contents/packs/coin-quest/pack.json") => (201, "{}".into()),
            ("POST", "/repos/jackuri/opentrainer-packs/pulls") => (
                201,
                json!({ "html_url": "https://github.com/jackuri/opentrainer-packs/pull/7", "number": 7 }).to_string(),
            ),
            _ => (404, json!({ "message": "Not Found" }).to_string()),
        }
        }
    }

    fn run<T>(f: impl std::future::Future<Output = T>) -> T {
        tauri::async_runtime::block_on(f)
    }

    #[test]
    fn shares_through_a_fork_and_a_pull_request() {
        let (hub, seen) = fake(github_like("sam", false));
        let text = pack_text("sam");
        let done = run(share(&hub, "tok", &text, &req(false), 42)).unwrap();
        assert_eq!(
            done,
            Shared {
                url: "https://github.com/jackuri/opentrainer-packs/pull/7".into(),
                number: 7,
                update: false
            }
        );
        let seen = lock(&seen).clone();
        let calls: Vec<String> = seen.iter().map(|(m, p, _)| format!("{m} {p}")).collect();
        let at = |c: &str| {
            calls
                .iter()
                .position(|x| x == c)
                .unwrap_or_else(|| panic!("no {c} in {calls:?}"))
        };
        assert!(
            at("POST /repos/jackuri/opentrainer-packs/forks")
                < at("POST /repos/sam/opentrainer-packs/git/refs")
        );
        let refs = &seen[at("POST /repos/sam/opentrainer-packs/git/refs")].2;
        assert!(
            refs.contains("refs/heads/pack-coin-quest-42") && refs.contains("abc123"),
            "{refs}"
        );
        let put: Value = serde_json::from_str(
            &seen[at("PUT /repos/sam/opentrainer-packs/contents/packs/coin-quest/pack.json")].2,
        )
        .unwrap();
        let content = base64::engine::general_purpose::STANDARD
            .decode(put["content"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            String::from_utf8(content).unwrap(),
            text,
            "the exact pack text is shared"
        );
        assert_eq!(put["branch"], "pack-coin-quest-42");
        let pr: Value =
            serde_json::from_str(&seen[at("POST /repos/jackuri/opentrainer-packs/pulls")].2)
                .unwrap();
        assert_eq!(pr["head"], "sam:pack-coin-quest-42");
        assert_eq!(pr["base"], "main");
        assert_eq!(pr["title"], "Add pack: Coin Quest");
        let body = pr["body"].as_str().unwrap();
        assert!(
            body.contains("Steam, v1.2")
                && body.contains("@sam")
                && body.contains("Infinite Health"),
            "{body}"
        );
        // The token went to GitHub only as a header, never in a body or address.
        assert!(
            seen.iter()
                .all(|(_, p, b)| !p.contains("tok") && !b.contains("\"tok\""))
        );
    }

    #[test]
    fn the_owner_shares_from_a_branch_and_updates_say_so() {
        let (hub, seen) = fake(github_like("jackuri", true));
        let done = run(share(&hub, "tok", &pack_text("jackuri"), &req(false), 1)).unwrap();
        assert!(done.update);
        let seen = lock(&seen).clone();
        assert!(!seen.iter().any(|(_, p, _)| p.ends_with("/forks")));
        let pr = seen.iter().find(|(_, p, _)| p.ends_with("/pulls")).unwrap();
        let pr: Value = serde_json::from_str(&pr.2).unwrap();
        assert_eq!(pr["head"], "pack-coin-quest-1");
        assert_eq!(pr["title"], "Update pack: Coin Quest");
    }

    #[test]
    fn table_packs_need_permission_and_bad_packs_are_refused() {
        let (hub, seen) = fake(github_like("sam", false));
        let table = pack_text(TABLE_AUTHOR);
        let e = run(share(&hub, "tok", &table, &req(false), 1)).unwrap_err();
        assert_eq!(e.kind, "needsPermission");
        assert!(
            lock(&seen).is_empty(),
            "nothing is sent before the checks pass"
        );
        let ok = run(share(&hub, "tok", &table, &req(true), 1)).unwrap();
        assert_eq!(ok.number, 7);
        let pr = lock(&seen)
            .iter()
            .find(|(_, p, _)| p.ends_with("/pulls"))
            .unwrap()
            .2
            .clone();
        assert!(pr.contains("Cheat Engine table"), "{pr}");
        assert_eq!(shareable("{}", &req(true)).unwrap_err().kind, "invalidPack");
    }

    #[test]
    fn an_expired_sign_in_is_reported() {
        let (hub, _) = fake(|_, _, _| (401, json!({ "message": "Bad credentials" }).to_string()));
        let e = run(share(&hub, "tok", &pack_text("sam"), &req(false), 1)).unwrap_err();
        assert_eq!(e.kind, "signedOut");
    }

    #[test]
    fn signs_in_with_a_code() {
        let polls = Arc::new(Mutex::new(0));
        let n = Arc::clone(&polls);
        let (hub, seen) = fake(move |_, p, _| {
            match p {
            "/login/device/code" => (
                200,
                json!({ "device_code": "dev1", "user_code": "ABCD-1234", "interval": 1, "expires_in": 900 }).to_string(),
            ),
            "/login/oauth/access_token" => {
                let mut k = lock(&n);
                *k += 1;
                let v = match *k {
                    1 => json!({ "error": "authorization_pending" }),
                    2 => json!({ "error": "slow_down", "interval": 1 }),
                    _ => json!({ "access_token": "gho_x", "token_type": "bearer" }),
                };
                (200, v.to_string())
            }
            _ => (404, "{}".into()),
        }
        });
        let (code, device, interval) = run(start_device(&hub, "cid")).unwrap();
        assert_eq!(
            (code.user_code.as_str(), device.as_str(), interval),
            ("ABCD-1234", "dev1", 1)
        );
        let cancel = AtomicBool::new(false);
        assert_eq!(
            run(wait_for_token(&hub, "cid", &device, 1, &cancel, 10)).unwrap(),
            "gho_x"
        );
        let seen = lock(&seen).clone();
        assert!(seen[0].2.contains("scope=public_repo") && seen[0].2.contains("client_id=cid"));
        assert!(
            seen[1]
                .2
                .contains("grant_type=urn:ietf:params:oauth:grant-type:device_code")
        );
    }

    #[test]
    fn sign_in_stops_when_refused_or_cancelled() {
        let (hub, _) = fake(|_, _, _| (200, json!({ "error": "access_denied" }).to_string()));
        let cancel = AtomicBool::new(false);
        assert_eq!(
            run(wait_for_token(&hub, "c", "d", 1, &cancel, 5))
                .unwrap_err()
                .kind,
            "denied"
        );
        cancel.store(true, Ordering::SeqCst);
        assert_eq!(
            run(wait_for_token(&hub, "c", "d", 1, &cancel, 5))
                .unwrap_err()
                .kind,
            "cancelled"
        );
    }

    #[test]
    fn opens_only_community_pull_requests() {
        assert!(is_pr_url(
            "https://github.com/jackuri/opentrainer-packs/pull/7"
        ));
        assert!(!is_pr_url(
            "https://github.com/jackuri/opentrainer-packs/pull/"
        ));
        assert!(!is_pr_url(
            "https://github.com/jackuri/opentrainer-packs/pull/7?x=1"
        ));
        assert!(!is_pr_url(
            "https://evil.example/jackuri/opentrainer-packs/pull/7"
        ));
        assert!(!is_pr_url(
            "https://github.com/jackuri/opentrainer-packs/pull/7/../../../x"
        ));
    }

    #[test]
    fn keeps_the_sign_in_in_credential_manager() {
        let target = format!("OpenTrainer/test-{}", std::process::id());
        vault::save(&target, "secret-token").unwrap();
        assert_eq!(vault::load(&target).as_deref(), Some("secret-token"));
        vault::delete(&target);
        assert_eq!(vault::load(&target), None);
    }
}
