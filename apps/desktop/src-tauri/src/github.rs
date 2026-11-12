//! Sharing a pack with the community list on GitHub, done by the app.
//!
//! The player signs in once with GitHub's device flow (they type a short code
//! on github.com; JuiceBox Trainer never sees their password). The token, scoped to
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
use jb_packs::{community, store};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::State;

use crate::commands::{CmdResult, CommandError};

/// The community repository (`owner/name`).
pub const COMMUNITY_REPO: &str = "jackuri/juicebox-packs";

/// `http://127.0.0.1:<port>` or `http://localhost:<port>` (nothing else).
fn is_loopback(url: &str) -> bool {
    ["http://127.0.0.1:", "http://localhost:"].iter().any(|p| {
        url.strip_prefix(p)
            .is_some_and(|port| !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()))
    })
}
const COMMUNITY_OWNER: &str = "jackuri";
/// JuiceBox Trainer's GitHub OAuth app (public, not a secret; owned by jackuri,
/// device flow on, tokens don't expire). `JB_GITHUB_CLIENT_ID` overrides it
/// for testing.
const CLIENT_ID: &str = "Ov23liChvIJtZaXBnMC4";
/// Where the player types the code. Only this page is opened.
pub const DEVICE_PAGE: &str = "https://github.com/login/device";
/// Pull requests the app may open in the browser start with this.
const PR_PREFIX: &str = "https://github.com/jackuri/juicebox-packs/pull/";
/// Write access to public repositories: needed to fork and open the request.
const SCOPE: &str = "public_repo";
const CREDENTIAL: &str = "JuiceBoxTrainer/github";
/// Where OpenTrainer (before 1.23.0) kept the sign-in.
const LEGACY_CREDENTIAL: &str = "OpenTrainer/github";

/// Moves a sign-in kept under OpenTrainer's name to JuiceBox Trainer's
/// (v1.23.0), so the player stays signed in. Called once at startup.
pub fn move_legacy_sign_in() {
    if vault::load(CREDENTIAL).is_none()
        && let Some(token) = vault::load(LEGACY_CREDENTIAL)
        && vault::save(CREDENTIAL, &token).is_ok()
    {
        vault::delete(LEGACY_CREDENTIAL);
    }
}
const MAX_NOTE: usize = 500;
/// Author text of packs converted from a Cheat Engine table (`jb_packs::ct`).
const TABLE_AUTHOR: &str = "Cheat Engine table (imported)";

fn client_id() -> Option<String> {
    std::env::var("JB_GITHUB_CLIENT_ID")
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
        // whose blob holds `CredentialBlobSize` bytes (or is null when empty:
        // any program of this user can write such a credential).
        let secret = unsafe {
            let c = &*out;
            let s = if c.CredentialBlob.is_null() || c.CredentialBlobSize == 0 {
                None
            } else {
                let bytes =
                    std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
                String::from_utf8(bytes.to_vec()).ok()
            };
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
        // Tests of the built app only: a GitHub stand-in on this PC (v1.33.0), so a test never
        // acts on the player's real account. Anything but a loopback address is ignored.
        if let Some(api) = std::env::var("JB_GITHUB_API")
            .ok()
            .filter(|a| is_loopback(a))
        {
            return Self::new(&api, &api);
        }
        Self::new("https://github.com", "https://api.github.com")
    }

    fn new(web: &str, api: &str) -> Result<Self, CommandError> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder()
            .user_agent(concat!("JuiceBoxTrainer/", env!("CARGO_PKG_VERSION")))
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
    /// This build knows JuiceBox Trainer's GitHub app.
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
                return Err(err("denied", "you didn't allow JuiceBox Trainer on GitHub"));
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
    /// The pack came from someone else: a file (`.juice`, `pack.json`) or a
    /// Cheat Engine table. Set by the app from where the pack was installed from.
    pub from_someone_else: bool,
    /// The player made it, or its maker allows sharing it.
    pub permission: bool,
}

/// Checks a pack may be shared; returns the parsed pack.
pub fn shareable(text: &str, req: &ShareRequest) -> Result<jb_packs::Pack, CommandError> {
    let (pack, _) = jb_packs::parse(text)
        .map_err(|e| err("invalidPack", format!("the pack isn't valid: {e}")))?;
    if pack.cheats.is_empty() {
        return Err(err("invalidPack", "the pack has no cheats"));
    }
    if (req.from_someone_else || pack.meta.author == TABLE_AUTHOR) && !req.permission {
        return Err(err(
            "needsPermission",
            "this pack came from someone else (a file or a Cheat Engine table): share it only if you made it or its maker allows it",
        ));
    }
    Ok(pack)
}

/// Pack text for the pull request: one line, capped, with markdown escaped
/// and `@` defused, so a pack can't mention people or add links.
fn plain(s: &str, max: usize) -> String {
    let mut out = String::new();
    for c in s.chars().take(max) {
        match c {
            c if c.is_control() => out.push(' '),
            '@' => out.push_str("@\u{200B}"),
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '|' | '!' | '~' | '(' | ')' => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.trim().to_string()
}

fn pr_text(
    pack: &jb_packs::Pack,
    req: &ShareRequest,
    update: bool,
    sharer: &str,
) -> (String, String) {
    let g = &pack.game;
    let title = format!(
        "{} pack: {}",
        if update { "Update" } else { "Add" },
        g.name
            .chars()
            .take(80)
            .collect::<String>()
            .replace('@', "@\u{200B}")
            .replace(['\r', '\n'], " ")
    );
    let names: Vec<String> = pack
        .cheats
        .iter()
        .take(20)
        .map(|c| plain(&c.name, 80))
        .collect();
    let more = pack.cheats.len().saturating_sub(names.len());
    let note = plain(&req.note, MAX_NOTE);
    let mut body = format!(
        "Shared from JuiceBox Trainer {} by @{sharer}.\n\n\
         - **Game:** {} ({}{})\n\
         - **Cheats ({}):** {}{}\n\
         - **Made by:** {}{}\n\
         - **Tested on:** {}\n\n\
         The sharer says every cheat works and the game has no anti-cheat.",
        env!("CARGO_PKG_VERSION"),
        plain(&g.name, 80),
        plain(&g.exe, 80),
        g.arch.map_or(String::new(), |a| format!(
            ", {}",
            if a == jb_packs::schema::PackArch::X86 {
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
        plain(&pack.meta.author, 80),
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
    let src = pack.meta.source.as_ref();
    body.push_str(&format!(
        "

- **Terms:** {}",
        src.and_then(|s| s.licence.as_deref())
            .map_or_else(|| "MIT (the list's default)".to_string(), |l| plain(l, 100))
    ));
    if let Some(by) = src.and_then(|s| s.by.as_deref()) {
        body.push_str(&format!(
            "
- **Credit:** {}",
            plain(by, 100)
        ));
    }
    if let Some(support) = src.and_then(|s| s.support.as_deref()) {
        body.push_str(&format!(
            "
- **Support page:** {}",
            plain(support, 400)
        ));
    }
    if src.is_some_and(|s| s.from_maker) {
        body.push_str(
            "
- **Shared by its maker:** the sharer says they made these cheats. Maintainers: check this              (for example the maker's forum profile or thread links to this GitHub account) before merging,              because it shows a \"Shared by its maker\" badge.",
        );
    } else if pack.meta.author == TABLE_AUTHOR || src.is_some_and(|s| s.file.is_some()) {
        body.push_str(
            "

It was converted from a Cheat Engine table; the sharer says its maker allows sharing it.",
        );
    } else if req.from_someone_else {
        body.push_str(
            "

It came from a pack file; the sharer says they made it or its maker allows sharing it.",
        );
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
    let pack = shareable(text, req)?;
    let id = pack.game.id.clone();
    send_file(
        hub,
        token,
        &format!("packs/{id}/pack.json"),
        text,
        &format!("pack-{id}"),
        stamp,
        |update| format!("{} pack: {id}", if update { "Update" } else { "Add" }),
        |update, login| pr_text(&pack, req, update, login),
    )
    .await
}

/// Puts one file into the community list through a pull request: from the
/// player's copy of the list (or the list itself for its owner).
#[allow(clippy::too_many_arguments)]
async fn send_file(
    hub: &Hub,
    token: &str,
    file: &str,
    text: &str,
    branch_prefix: &str,
    stamp: u64,
    commit_message: impl FnOnce(bool) -> String,
    pr_text: impl FnOnce(bool, &str) -> (String, String),
) -> Result<Shared, CommandError> {
    send_files(
        hub,
        token,
        &[(file, text)],
        branch_prefix,
        stamp,
        commit_message,
        pr_text,
    )
    .await
}

/// Like [`send_file`] with several files in one pull request (v1.27.0: a
/// table link and the table itself). The first file says whether it is an
/// update.
#[allow(clippy::too_many_arguments)]
async fn send_files(
    hub: &Hub,
    token: &str,
    files: &[(&str, &str)],
    branch_prefix: &str,
    stamp: u64,
    commit_message: impl FnOnce(bool) -> String,
    pr_text: impl FnOnce(bool, &str) -> (String, String),
) -> Result<Shared, CommandError> {
    let Some(&(file, _)) = files.first() else {
        return Err(err("badRequest", "nothing to send"));
    };
    use reqwest::Method;
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

    let branch = format!("{branch_prefix}-{stamp}");
    hub.ok(
        token,
        Method::POST,
        &format!("/repos/{head_repo}/git/refs"),
        Some(&json!({ "ref": format!("refs/heads/{branch}"), "sha": sha })),
        "to start your change",
    )
    .await?;
    let message = commit_message(update);
    for (file, text) in files {
        let (status, existing) = hub
            .call(
                token,
                Method::GET,
                &format!("/repos/{head_repo}/contents/{file}?ref={branch}"),
                None,
            )
            .await?;
        let mut put = json!({
            "message": message,
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
            "to add the file",
        )
        .await?;
    }

    let (title, body) = pr_text(update, &login);
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
            "to send your change",
        )
        .await?;
    Ok(Shared {
        url: pr["html_url"].as_str().unwrap_or_default().to_string(),
        number: pr["number"].as_u64().unwrap_or(0),
        update,
    })
}

/// The terms the player picks when sharing (v1.17.0).
#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Terms {
    /// The player made the cheats (for a table: they made the table).
    #[serde(default)]
    pub from_maker: bool,
    /// The maker's terms; none means MIT on the community list.
    #[serde(default)]
    pub licence: Option<String>,
    /// Where players can support the maker.
    #[serde(default)]
    pub support: Option<String>,
}

/// The pack text with the chosen terms in `meta.source` (checked again).
pub fn with_terms(text: &str, terms: &Terms) -> Result<String, CommandError> {
    let licence = terms
        .licence
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let support = terms
        .support
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if !terms.from_maker && licence.is_none() && support.is_none() {
        return Ok(text.to_string());
    }
    let bad = |e: String| err("badRequest", e);
    let mut v: Value = serde_json::from_str(text).map_err(|e| bad(e.to_string()))?;
    let src = &mut v["meta"]["source"];
    if terms.from_maker {
        src["fromMaker"] = true.into();
    }
    if let Some(l) = licence {
        src["licence"] = l.into();
    }
    if let Some(s) = support {
        src["support"] = s.into();
    }
    let text = serde_json::to_string_pretty(&v).map_err(|e| bad(e.to_string()))?
        + "
";
    match jb_packs::parse(&text) {
        Ok(_) => Ok(text),
        Err(e) => {
            let e = e.to_string();
            Err(bad(if e.contains("meta.source.support") {
                "the support page must be an https page on Patreon, Ko-fi, Buy Me a Coffee, GitHub, YouTube or a table site".into()
            } else if e.contains("meta.source.licence") {
                "the terms must be 1 to 100 characters on one line".into()
            } else {
                e
            }))
        }
    }
}

/// Shares one of the player's packs with the community list.
#[tauri::command]
pub async fn community_share(
    key: String,
    note: String,
    permission: bool,
    terms: Option<Terms>,
    state: State<'_, GithubState>,
) -> CmdResult<Shared> {
    let found = store::all()
        .into_iter()
        .find(|f| f.key == key)
        .ok_or_else(|| err("notFound", format!("no pack '{key}'")))?;
    let from_someone_else = match found.source {
        store::Source::Bundled => {
            return Err(err("badRequest", "built-in examples are already shared"));
        }
        store::Source::Community => {
            return Err(err(
                "badRequest",
                "this pack came from the community list; it's already shared",
            ));
        }
        store::Source::Imported => true,
        store::Source::User => false,
    };
    let text = found.text.map_err(|e| err("io", e.to_string()))?;
    let terms = terms.unwrap_or_default();
    let text = with_terms(&text, &terms)?;
    let token =
        vault::load(CREDENTIAL).ok_or_else(|| err("signedOut", "sign in to GitHub first"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let result = share(
        &Hub::github()?,
        &token,
        &text,
        &ShareRequest {
            note,
            from_someone_else,
            // The maker sharing their own cheats needs nobody's permission.
            permission: permission || terms.from_maker,
        },
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

/// The community-list link for a table a pack was converted from: needs the
/// maker's name and a page on a known table site (no file is shared).
pub fn table_link(
    pack: &jb_packs::Pack,
    today: &str,
) -> Result<community::TableLink, CommandError> {
    let src = pack.meta.source.as_ref();
    let by = src.and_then(|s| s.by.clone()).ok_or_else(|| {
        err(
            "needsCredit",
            "say who made the table first (convert it again and fill in its maker)",
        )
    })?;
    let url = src.and_then(|s| s.url.clone()).unwrap_or_default();
    community::table_site(&url).ok_or_else(|| {
        err(
            "badRequest",
            format!(
                "only links to pages on {} can be listed",
                community::TABLE_SITES
                    .iter()
                    .filter(|s| !s.starts_with("www."))
                    .copied()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
    })?;
    let title = src
        .and_then(|s| s.file.as_deref())
        .map(|f| {
            f.trim_end_matches(".CT")
                .trim_end_matches(".ct")
                .replace('_', " ")
        })
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| format!("{} table", pack.game.name));
    let link = community::TableLink {
        id: community::table_id(&pack.game.name, &url),
        game: pack.game.name.chars().take(120).collect(),
        exe: (pack.game.exe != jb_packs::ct::UNKNOWN_EXE).then(|| pack.game.exe.clone()),
        title: title.chars().take(160).collect(),
        by: by.chars().take(100).collect(),
        url,
        cheats: Some(pack.cheats.len()),
        added: Some(today.to_string()),
        licence: None,
        file: None,
        sha256: None,
    };
    if let Some(why) = community::table_problem(&link) {
        return Err(err(
            "badRequest",
            format!("this link can't be listed: {why}"),
        ));
    }
    Ok(link)
}

fn table_pr_text(link: &community::TableLink, update: bool, sharer: &str) -> (String, String) {
    let title = format!(
        "{} table {}: {}",
        if update { "Update" } else { "Add" },
        if link.file.is_some() {
            "and link"
        } else {
            "link"
        },
        plain(&link.game, 80)
    );
    let what = if link.file.is_some() {
        format!(
            "The table itself is included (`{}`), under **{}**. Please check on its page that its \
             maker gave it this licence or said it may be shared here before merging.",
            link.file_path(),
            plain(link.licence.as_deref().unwrap_or("no licence"), 100)
        )
    } else {
        "Only a link: the table stays on its maker's page. Please check the page is the table's \
         own page on its maker's site."
            .to_string()
    };
    let body = format!(
        "Shared from JuiceBox Trainer {} by @{sharer}.\n\n\
         - **Game:** {}{}\n\
         - **Table:** {}\n\
         - **Made by:** {}\n\
         - **Page:** {}\n\
         - **Cheats JuiceBox Trainer can use:** {}\n\
         {}\n\
         {what}",
        env!("CARGO_PKG_VERSION"),
        plain(&link.game, 80),
        link.exe
            .as_deref()
            .map_or(String::new(), |e| format!(" ({})", plain(e, 80))),
        plain(&link.title, 120),
        plain(&link.by, 80),
        plain(&link.url, 400),
        link.cheats
            .map_or_else(|| "not said".to_string(), |c| c.to_string()),
        link.licence.as_deref().map_or(String::new(), |l| format!(
            "- **Terms:** {}\n",
            plain(l, 100)
        )),
    );
    (title, body)
}

/// Adds the table itself to a link (v1.27.0): the `.CT` the player picked,
/// when its maker gave it an open licence or permission. The file must be a
/// table JuiceBox Trainer can read, for the same game.
pub fn with_table_file(
    mut link: community::TableLink,
    exe: &str,
    bytes: &[u8],
    licence: &str,
) -> Result<(community::TableLink, String), CommandError> {
    if !community::HOSTING_LICENCES.contains(&licence) {
        return Err(err(
            "badRequest",
            "the table itself can only be shared under an open licence or with its maker's permission",
        ));
    }
    if bytes.len() as u64 > community::MAX_TABLE_FILE {
        return Err(err(
            "badRequest",
            "the table is too big to share (2 MB at most)",
        ));
    }
    let text = String::from_utf8(bytes.to_vec())
        .map_err(|_| err("badRequest", "that file isn't a Cheat Engine table"))?;
    let known_exe = (exe != jb_packs::ct::UNKNOWN_EXE).then_some(exe);
    // Read without naming the exe, so the table's own addresses say which game it is for.
    let converted = jb_packs::ct::convert(bytes, "table", None, &jb_packs::share::today())
        .map_err(|e| err("badRequest", e.to_string()))?;
    if converted.pack.cheats.is_empty() {
        return Err(err(
            "badRequest",
            "that table has nothing JuiceBox Trainer can use",
        ));
    }
    if converted.exe_known
        && known_exe.is_some_and(|e| !converted.pack.game.exe.eq_ignore_ascii_case(e))
    {
        return Err(err(
            "badRequest",
            format!("that table is for {}, not {exe}", converted.pack.game.exe),
        ));
    }
    link.licence = Some(licence.to_string());
    link.file = Some(link.file_path());
    link.sha256 = Some(jb_core::hash::sha256_bytes(bytes));
    if let Some(why) = community::table_problem(&link) {
        return Err(err(
            "badRequest",
            format!("this table can't be listed: {why}"),
        ));
    }
    Ok((link, text))
}

/// Adds the link to the table one of the player's packs was converted from
/// to the community list (a pull request with `tables/<id>.json`). With
/// `file` and `licence` (v1.27.0), the table itself goes too, as
/// `tables/<id>.CT`.
#[tauri::command]
pub async fn community_share_table(
    key: String,
    file: Option<String>,
    licence: Option<String>,
    state: State<'_, GithubState>,
) -> CmdResult<Shared> {
    let found = store::all()
        .into_iter()
        .find(|f| f.key == key)
        .ok_or_else(|| err("notFound", format!("no pack '{key}'")))?;
    let text = found.text.map_err(|e| err("io", e.to_string()))?;
    let (pack, _) = jb_packs::parse(&text)
        .map_err(|e| err("invalidPack", format!("the pack isn't valid: {e}")))?;
    let mut link = table_link(&pack, &jb_packs::share::today())?;
    let licence = licence
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty());
    let mut table_text = None;
    match (file, &licence) {
        (Some(path), Some(l)) => {
            let bytes = std::fs::read(&path).map_err(|e| err("io", format!("{path}: {e}")))?;
            let (with_file, t) = with_table_file(link, &pack.game.exe, &bytes, l)?;
            link = with_file;
            table_text = Some(t);
        }
        (Some(_), None) => {
            return Err(err(
                "badRequest",
                "choose the table's licence to share the table itself",
            ));
        }
        (None, Some(l)) => {
            link.licence = Some(
                l.chars()
                    .take(jb_packs::schema::MAX_SOURCE_LICENCE)
                    .collect(),
            );
        }
        (None, None) => {}
    }
    let json = serde_json::to_string_pretty(&link).map_err(|e| err("io", e.to_string()))? + "\n";
    let token =
        vault::load(CREDENTIAL).ok_or_else(|| err("signedOut", "sign in to GitHub first"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let id = link.id.clone();
    let link_file = format!("tables/{id}.json");
    let table_file = link.file_path();
    let mut files: Vec<(&str, &str)> = vec![(&link_file, &json)];
    if let Some(t) = &table_text {
        files.push((&table_file, t));
    }
    let result = send_files(
        &Hub::github()?,
        &token,
        &files,
        &format!("table-{}", &id[..id.len().min(40)]),
        stamp,
        |update| format!("{} table link: {id}", if update { "Update" } else { "Add" }),
        |update, login| table_pr_text(&link, update, login),
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

/// A rating is a reaction on the pack's rating issue (v1.29.0): 1 star
/// broken, 2 works with bugs, 3 works great.
pub const RATING_REACTIONS: [&str; 3] = ["-1", "confused", "+1"];

/// Rates a community pack for the signed-in player: their earlier rating of
/// it is taken back and the new one added, so each account counts once.
/// Only an open issue the list made for rating ("Rate: <id>", label
/// `rating`) can be rated.
pub async fn rate(hub: &Hub, token: &str, issue: u64, stars: u8) -> Result<u8, CommandError> {
    use reqwest::Method;
    let want = *RATING_REACTIONS
        .get(usize::from(stars).wrapping_sub(1))
        .ok_or_else(|| err("badRequest", "a rating is 1, 2 or 3 stars"))?;
    let me = hub
        .ok(token, Method::GET, "/user", None, "to say who you are")
        .await?;
    let login = me["login"]
        .as_str()
        .ok_or_else(|| err("github", "GitHub didn't say who you are"))?
        .to_string();
    let base = format!("/repos/{COMMUNITY_REPO}/issues/{issue}");
    let it = hub
        .ok(
            token,
            Method::GET,
            &base,
            None,
            "to find the pack's ratings",
        )
        .await?;
    let is_rating = it["title"]
        .as_str()
        .is_some_and(|t| t.starts_with("Rate: "))
        && it["state"] == "open"
        && it.get("pull_request").is_none_or(Value::is_null)
        && it["labels"]
            .as_array()
            .is_some_and(|l| l.iter().any(|x| x["name"] == "rating"));
    if !is_rating {
        return Err(err("badRequest", "that isn't a pack's rating page"));
    }
    // Take back this player's earlier rating of the pack (only the rating kinds).
    for page in 1..=10 {
        let list = hub
            .ok(
                token,
                Method::GET,
                &format!("{base}/reactions?per_page=100&page={page}"),
                None,
                "to read the ratings",
            )
            .await?;
        let list = list.as_array().cloned().unwrap_or_default();
        for r in &list {
            let mine = r["user"]["login"]
                .as_str()
                .is_some_and(|l| l.eq_ignore_ascii_case(&login));
            let content = r["content"].as_str().unwrap_or("");
            if mine
                && content != want
                && RATING_REACTIONS.contains(&content)
                && let Some(id) = r["id"].as_u64()
            {
                hub.ok(
                    token,
                    Method::DELETE,
                    &format!("{base}/reactions/{id}"),
                    None,
                    "to change your rating",
                )
                .await?;
            }
        }
        if list.len() < 100 {
            break;
        }
    }
    hub.ok(
        token,
        Method::POST,
        &format!("{base}/reactions"),
        Some(&json!({ "content": want })),
        "to add your rating",
    )
    .await?;
    Ok(stars)
}

/// Rates a community pack 1 to 3 stars (v1.29.0) through the player's
/// GitHub sign-in. Counted on the list within the hour.
#[tauri::command]
pub async fn community_rate(issue: u64, stars: u8, state: State<'_, GithubState>) -> CmdResult<u8> {
    let token =
        vault::load(CREDENTIAL).ok_or_else(|| err("signedOut", "sign in to GitHub first"))?;
    let result = rate(&Hub::github()?, &token, issue, stars).await;
    if let Err(e) = &result
        && e.kind == "signedOut"
    {
        vault::delete(CREDENTIAL);
        *lock(&state.login) = None;
    }
    result
}

/// A daily check-in (v1.33.0): `done` (a 👍 added to today's check-in issue),
/// `already` (checked in today) or `notYet` (today's issue isn't open yet).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckIn {
    /// The day (UTC), `YYYY-MM-DD`.
    pub day: String,
    pub status: &'static str,
}

/// Checks the player in for `today` (UTC): a 👍 on that day's "Check-in:
/// <day>" issue (label `check-in`) of the community list, which its XP
/// robot counts (2 XP a day, 10 more per 5 days in a row).
pub async fn check_in(hub: &Hub, token: &str, today: &str) -> Result<CheckIn, CommandError> {
    use reqwest::Method;
    let me = hub
        .ok(token, Method::GET, "/user", None, "to say who you are")
        .await?;
    let login = me["login"]
        .as_str()
        .ok_or_else(|| err("github", "GitHub didn't say who you are"))?
        .to_string();
    let title = format!("Check-in: {today}");
    let open = hub
        .ok(
            token,
            Method::GET,
            &format!("/repos/{COMMUNITY_REPO}/issues?labels=check-in&state=open&per_page=30"),
            None,
            "to find today's check-in",
        )
        .await?;
    let issue = open.as_array().into_iter().flatten().find_map(|i| {
        (i["title"].as_str() == Some(title.as_str())
            && i.get("pull_request").is_none_or(Value::is_null))
        .then(|| i["number"].as_u64())
        .flatten()
    });
    let Some(issue) = issue else {
        return Ok(CheckIn {
            day: today.into(),
            status: "notYet",
        });
    };
    let base = format!("/repos/{COMMUNITY_REPO}/issues/{issue}/reactions");
    for page in 1..=20 {
        let list = hub
            .ok(
                token,
                Method::GET,
                &format!("{base}?per_page=100&page={page}"),
                None,
                "to read today's check-ins",
            )
            .await?;
        let list = list.as_array().cloned().unwrap_or_default();
        let mine = list.iter().any(|r| {
            r["user"]["login"]
                .as_str()
                .is_some_and(|l| l.eq_ignore_ascii_case(&login))
                && r["created_at"]
                    .as_str()
                    .is_some_and(|t| t.starts_with(today))
        });
        if mine {
            return Ok(CheckIn {
                day: today.into(),
                status: "already",
            });
        }
        if list.len() < 100 {
            break;
        }
    }
    hub.ok(
        token,
        Method::POST,
        &base,
        Some(&json!({ "content": "+1" })),
        "to check you in",
    )
    .await?;
    Ok(CheckIn {
        day: today.into(),
        status: "done",
    })
}

/// Daily check-in (v1.33.0): see [`check_in`]. The player turns it on.
#[tauri::command]
pub async fn community_check_in(state: State<'_, GithubState>) -> CmdResult<CheckIn> {
    let token =
        vault::load(CREDENTIAL).ok_or_else(|| err("signedOut", "sign in to GitHub first"))?;
    let result = check_in(&Hub::github()?, &token, &jb_packs::share::today()).await;
    if let Err(e) = &result
        && e.kind == "signedOut"
    {
        vault::delete(CREDENTIAL);
        *lock(&state.login) = None;
    }
    result
}

/// The community-list pack of one of the player's web games: their saved
/// cheats, checked by the list's rules (https, at most 50 cheats).
pub fn web_shareable(
    dir: &std::path::Path,
    id: &str,
    by: &str,
    notes: Option<&str>,
    today: &str,
) -> Result<jb_packs::web::WebPack, CommandError> {
    let pack = crate::web::pack_for(dir, id, by, notes, today)?;
    jb_packs::web::check_pack(&pack, true).map_err(|e| err("badRequest", e))
}

fn web_pr_text(pack: &jb_packs::web::WebPack, update: bool, sharer: &str) -> (String, String) {
    let title = format!(
        "{} web cheats: {}",
        if update { "Update" } else { "Add" },
        plain(&pack.game, 80)
    );
    let names: Vec<String> = pack
        .cheats
        .iter()
        .take(20)
        .map(|c| plain(&c.name, 80))
        .collect();
    let more = pack.cheats.len().saturating_sub(names.len());
    let mut body = format!(
        "Shared from JuiceBox Trainer {} by @{sharer}.

         - **Web game:** {}
         - **Address:** {}
         - **Cheats ({}):** {}{}
         - **Found by:** {}
         - **Notes:** {}

         The sharer says every cheat works. Web cheats are for playing alone or with invited friends;          maintainers: check the address is the game's own page.",
        env!("CARGO_PKG_VERSION"),
        plain(&pack.game, 80),
        plain(&pack.url, 400),
        pack.cheats.len(),
        names.join(", "),
        if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        },
        plain(&pack.by, 100),
        pack.notes
            .as_deref()
            .map_or_else(|| "none".to_string(), |n| plain(n, 500)),
    );
    if update {
        body.push_str(
            "

The list already has these web cheats: this replaces them.",
        );
    }
    (title, body)
}

/// Shares the cheats saved for one of the player's web games with the
/// community list (a pull request with `web/<id>.json`).
#[tauri::command]
pub async fn community_share_web(
    id: String,
    by: String,
    notes: Option<String>,
    state: State<'_, GithubState>,
) -> CmdResult<Shared> {
    let pack = web_shareable(
        &crate::web::app_dir()?,
        &id,
        &by,
        notes.as_deref(),
        &jb_packs::share::today(),
    )?;
    let json = serde_json::to_string_pretty(&pack).map_err(|e| err("io", e.to_string()))?
        + "
";
    let token =
        vault::load(CREDENTIAL).ok_or_else(|| err("signedOut", "sign in to GitHub first"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let pid = pack.id.clone();
    let result = send_file(
        &Hub::github()?,
        &token,
        &format!("web/{pid}.json"),
        &json,
        &format!("web-{}", &pid[..pid.len().min(40)]),
        stamp,
        |update| {
            format!(
                "{} web cheats: {pid}",
                if update { "Update" } else { "Add" }
            )
        },
        |update, login| web_pr_text(&pack, update, login),
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
    crate::browser::open_url(url)?;
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
        let (mut pack, _) = jb_packs::parse(store::BUNDLED[0].1).unwrap();
        pack.game.id = "coin-quest".into();
        pack.game.name = "Coin Quest".into();
        pack.meta.author = author.into();
        serde_json::to_string_pretty(&pack).unwrap()
    }

    fn req(permission: bool) -> ShareRequest {
        ShareRequest {
            note: "Steam, v1.2".into(),
            from_someone_else: false,
            permission,
        }
    }

    fn github_like(
        login: &'static str,
        listed: bool,
    ) -> impl Fn(&str, &str, &str) -> (u16, String) + Send + 'static {
        move |m: &str, p: &str, _b: &str| {
            match (m, p) {
            ("GET", "/user") => (200, json!({ "login": login }).to_string()),
            ("POST", "/repos/jackuri/juicebox-packs/forks") => {
                (202, json!({ "full_name": format!("{login}/juicebox-packs") }).to_string())
            }
            ("GET", p) if p == format!("/repos/{login}/juicebox-packs") => (200, "{}".into()),
            ("POST", p) if p.ends_with("/merge-upstream") => (200, "{}".into()),
            ("GET", "/repos/jackuri/juicebox-packs/git/ref/heads/main") => {
                (200, json!({ "object": { "sha": "abc123" } }).to_string())
            }
            ("GET", "/repos/jackuri/juicebox-packs/contents/packs/coin-quest/pack.json") => {
                if listed { (200, json!({ "sha": "old" }).to_string()) } else { (404, "{}".into()) }
            }
            ("POST", p) if p.ends_with("/git/refs") => (201, "{}".into()),
            ("GET", p) if p.contains("/contents/packs/coin-quest/pack.json?ref=") => (404, "{}".into()),
            ("PUT", p) if p.ends_with("/contents/packs/coin-quest/pack.json") => (201, "{}".into()),
            ("PUT", p) if p.contains("/contents/tables/") || p.contains("/contents/web/") => (201, "{}".into()),
            ("POST", "/repos/jackuri/juicebox-packs/pulls") => (
                201,
                json!({ "html_url": "https://github.com/jackuri/juicebox-packs/pull/7", "number": 7 }).to_string(),
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
                url: "https://github.com/jackuri/juicebox-packs/pull/7".into(),
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
            at("POST /repos/jackuri/juicebox-packs/forks")
                < at("POST /repos/sam/juicebox-packs/git/refs")
        );
        let refs = &seen[at("POST /repos/sam/juicebox-packs/git/refs")].2;
        assert!(
            refs.contains("refs/heads/pack-coin-quest-42") && refs.contains("abc123"),
            "{refs}"
        );
        let put: Value = serde_json::from_str(
            &seen[at("PUT /repos/sam/juicebox-packs/contents/packs/coin-quest/pack.json")].2,
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
            serde_json::from_str(&seen[at("POST /repos/jackuri/juicebox-packs/pulls")].2).unwrap();
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

    fn credited(url: &str) -> jb_packs::Pack {
        let mut v: Value = serde_json::from_str(&pack_text("sam")).unwrap();
        v["meta"]["source"] =
            json!({ "by": "TableMaker", "url": url, "file": "Coin_Quest_v1.2.CT" });
        jb_packs::parse(&v.to_string()).unwrap().0
    }

    fn table_for(exe: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<CheatTable CheatEngineTableVersion=\"45\">\n  <CheatEntries>\n    <CheatEntry>\n      <ID>1</ID>\n      <Description>\"Gold\"</Description>\n      <VariableType>4 Bytes</VariableType>\n      <Address>\"{exe}\"+1234</Address>\n    </CheatEntry>\n  </CheatEntries>\n</CheatTable>\n"
        )
    }

    /// v1.27.0: the table itself, under an open licence, in the same pull request.
    #[test]
    fn shares_a_table_with_its_file_under_an_open_licence() {
        let link = table_link(
            &credited("https://fearlessrevolution.com/viewtopic.php?t=99"),
            "2026-11-12",
        )
        .unwrap();
        let table = table_for("testbed_native.exe");

        let closed = with_table_file(
            link.clone(),
            "testbed_native.exe",
            table.as_bytes(),
            "All rights reserved",
        )
        .unwrap_err();
        assert!(
            closed.message.contains("open licence"),
            "{}",
            closed.message
        );
        let other = with_table_file(
            link.clone(),
            "testbed_native.exe",
            table_for("Other.exe").as_bytes(),
            "MIT",
        )
        .unwrap_err();
        assert!(other.message.contains("Other.exe"), "{}", other.message);
        assert!(
            with_table_file(link.clone(), "testbed_native.exe", b"not a table", "MIT").is_err()
        );

        let (with_file, text) = with_table_file(
            link.clone(),
            "testbed_native.exe",
            table.as_bytes(),
            "CC-BY-4.0",
        )
        .unwrap();
        assert_eq!(
            with_file.file.as_deref(),
            Some(with_file.file_path().as_str())
        );
        assert_eq!(with_file.licence.as_deref(), Some("CC-BY-4.0"));
        assert_eq!(text, table);

        let (title, body) = table_pr_text(&with_file, false, "sam");
        assert!(title.starts_with("Add table and link"), "{title}");
        assert!(
            body.contains("CC-BY-4.0") && body.contains("check on its page"),
            "{body}"
        );

        let (hub, seen) = fake(github_like("sam", false));
        let json = serde_json::to_string_pretty(&with_file).unwrap() + "\n";
        let link_file = format!("tables/{}.json", with_file.id);
        let table_file = with_file.file_path();
        run(send_files(
            &hub,
            "tok",
            &[(&link_file, &json), (&table_file, &text)],
            "table-x",
            42,
            |_| "Add table link".into(),
            |u, l| table_pr_text(&with_file, u, l),
        ))
        .unwrap();
        let seen = lock(&seen).clone();
        let written: Vec<&String> = seen
            .iter()
            .filter(|(m, _, _)| m == "PUT")
            .map(|(_, p, _)| p)
            .collect();
        assert!(
            written.iter().any(|p| p.ends_with(&link_file)),
            "{written:?}"
        );
        let put = seen
            .iter()
            .find(|(m, p, _)| m == "PUT" && p.ends_with(&table_file))
            .expect("the table is written");
        let put: Value = serde_json::from_str(&put.2).unwrap();
        let content = base64::engine::general_purpose::STANDARD
            .decode(put["content"].as_str().unwrap())
            .unwrap();
        assert_eq!(content, table.as_bytes());
        assert_eq!(
            seen.iter()
                .filter(|(m, p, _)| m == "POST" && p.ends_with("/pulls"))
                .count(),
            1,
            "one pull request"
        );
    }

    #[test]
    fn table_links_need_the_maker_and_a_known_site() {
        let link = table_link(
            &credited("https://fearlessrevolution.com/viewtopic.php?t=99"),
            "2026-11-09",
        )
        .unwrap();
        assert_eq!(link.game, "Coin Quest");
        assert_eq!(link.title, "Coin Quest v1.2");
        assert_eq!(link.by, "TableMaker");
        assert_eq!(link.added.as_deref(), Some("2026-11-09"));
        assert!(link.id.starts_with("coin-quest-"), "{}", link.id);

        let elsewhere = table_link(&credited("https://example.com/t/1"), "2026-11-09").unwrap_err();
        assert!(
            elsewhere.message.contains("fearlessrevolution.com"),
            "{}",
            elsewhere.message
        );
        let (anon, _) = jb_packs::parse(&pack_text("sam")).unwrap();
        assert_eq!(
            table_link(&anon, "2026-11-09").unwrap_err().kind,
            "needsCredit"
        );

        // The pull request text can't mention people or add links.
        let mut sly = link.clone();
        sly.by = "@everyone [click](https://evil.example)".into();
        let (title, body) = table_pr_text(&sly, false, "sam");
        assert_eq!(title, "Add table link: Coin Quest");
        assert!(
            body.contains("@\u{200B}everyone") && body.contains("\\[click\\]\\(https"),
            "{body}"
        );
        assert!(body.contains("Only a link"));
    }

    #[test]
    fn the_maker_picks_terms_and_a_support_page() {
        let text = pack_text("sam");
        // No terms: the text is shared exactly as it is.
        assert_eq!(with_terms(&text, &Terms::default()).unwrap(), text);
        let terms = Terms {
            from_maker: true,
            licence: Some(jb_packs::schema::LICENCE_PERMISSION.into()),
            support: Some("https://ko-fi.com/sam".into()),
        };
        let shared = with_terms(&text, &terms).unwrap();
        let (pack, _) = jb_packs::parse(&shared).unwrap();
        let src = pack.meta.source.clone().unwrap();
        assert!(src.from_maker);
        assert_eq!(
            src.licence.as_deref(),
            Some(jb_packs::schema::LICENCE_PERMISSION)
        );
        let (_, body) = pr_text(&pack, &req(false), false, "sam");
        assert!(body.contains("all rights kept by the maker"), "{body}");
        assert!(
            body.contains("Shared by its maker") && body.contains("ko-fi.com/sam"),
            "{body}"
        );
        // A made-by-the-maker table pack needs no other permission.
        assert!(
            shareable(
                &shared,
                &ShareRequest {
                    note: String::new(),
                    from_someone_else: true,
                    permission: true
                }
            )
            .is_ok()
        );

        let bad = with_terms(
            &text,
            &Terms {
                support: Some("https://evil.example/pay".into()),
                ..Terms::default()
            },
        )
        .unwrap_err();
        assert!(bad.message.contains("Patreon"), "{}", bad.message);
        let (plain_pack, _) = jb_packs::parse(&text).unwrap();
        let (_, mit) = pr_text(&plain_pack, &req(false), false, "sam");
        assert!(mit.contains("MIT (the list's default)"), "{mit}");
    }

    #[test]
    fn only_a_loopback_stand_in_replaces_github() {
        assert!(is_loopback("http://127.0.0.1:5000"));
        assert!(is_loopback("http://localhost:8080"));
        assert!(!is_loopback("https://evil.example"));
        assert!(!is_loopback("http://127.0.0.1:5000/x"));
        assert!(!is_loopback("http://127.0.0.1.evil.example:80"));
        assert!(!is_loopback("http://localhost:"));
    }

    /// v1.33.0: the daily check-in adds one 👍 to today's check-in issue, once.
    #[test]
    fn checks_in_once_a_day() {
        let reacted = std::sync::Arc::new(std::sync::Mutex::new(false));
        let r2 = std::sync::Arc::clone(&reacted);
        let route = move |m: &str, p: &str, _b: &str| -> (u16, String) {
            match (m, p) {
                ("GET", "/user") => (200, json!({ "login": "Sam" }).to_string()),
                ("GET", p)
                    if p.starts_with("/repos/jackuri/juicebox-packs/issues?labels=check-in") =>
                {
                    (
                        200,
                        json!([
                            { "number": 40, "title": "Check-in: 2026-11-12" },
                            { "number": 41, "title": "Check-in: 2026-11-13" }
                        ])
                        .to_string(),
                    )
                }
                ("GET", p)
                    if p.starts_with("/repos/jackuri/juicebox-packs/issues/41/reactions") =>
                {
                    let mut list = vec![
                        json!({ "content": "+1", "user": { "login": "alex" }, "created_at": "2026-11-13T01:00:00Z" }),
                    ];
                    if *lock(&r2) {
                        list.push(json!({ "content": "+1", "user": { "login": "sam" }, "created_at": "2026-11-13T09:00:00Z" }));
                    }
                    (200, Value::Array(list).to_string())
                }
                ("POST", "/repos/jackuri/juicebox-packs/issues/41/reactions") => {
                    *lock(&r2) = true;
                    (201, "{}".into())
                }
                _ => (404, json!({ "message": "Not Found" }).to_string()),
            }
        };
        let (hub, seen) = fake(route);
        assert_eq!(
            run(check_in(&hub, "tok", "2026-11-13")).unwrap().status,
            "done"
        );
        let post = lock(&seen)
            .iter()
            .find(|(m, _, _)| m == "POST")
            .cloned()
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&post.2).unwrap()["content"],
            "+1"
        );
        assert_eq!(
            run(check_in(&hub, "tok", "2026-11-13")).unwrap().status,
            "already"
        );
        assert_eq!(
            lock(&seen).iter().filter(|(m, _, _)| m == "POST").count(),
            1,
            "checked in once"
        );
        assert_eq!(
            run(check_in(&hub, "tok", "2026-11-14")).unwrap().status,
            "notYet"
        );
    }

    /// v1.29.0: a rating replaces the player's earlier one on the pack's rating issue.
    #[test]
    fn rates_a_pack_once_per_player() {
        let route = |m: &str, p: &str, _b: &str| -> (u16, String) {
            match (m, p) {
                ("GET", "/user") => (200, json!({ "login": "Sam" }).to_string()),
                ("GET", "/repos/jackuri/juicebox-packs/issues/12") => (
                    200,
                    json!({ "title": "Rate: coin-quest", "state": "open", "labels": [{ "name": "rating" }] }).to_string(),
                ),
                ("GET", "/repos/jackuri/juicebox-packs/issues/13") => (
                    200,
                    json!({ "title": "Fix the installer", "state": "open", "labels": [{ "name": "bug" }] }).to_string(),
                ),
                ("GET", p) if p.starts_with("/repos/jackuri/juicebox-packs/issues/12/reactions") => (
                    200,
                    json!([
                        { "id": 1, "content": "-1", "user": { "login": "sam" } },
                        { "id": 2, "content": "heart", "user": { "login": "sam" } },
                        { "id": 3, "content": "-1", "user": { "login": "alex" } },
                        { "id": 4, "content": "+1", "user": { "login": "sam" } }
                    ])
                    .to_string(),
                ),
                ("DELETE", _) => (204, String::new()),
                ("POST", "/repos/jackuri/juicebox-packs/issues/12/reactions") => (201, "{}".into()),
                _ => (404, json!({ "message": "Not Found" }).to_string()),
            }
        };
        let (hub, seen) = fake(route);
        assert_eq!(run(rate(&hub, "tok", 12, 2)).unwrap(), 2);
        let seen = lock(&seen).clone();
        let deleted: Vec<&String> = seen
            .iter()
            .filter(|(m, _, _)| m == "DELETE")
            .map(|(_, p, _)| p)
            .collect();
        // Sam's earlier 1-star and 3-star ratings go; their heart and Alex's rating stay.
        assert_eq!(
            deleted,
            [
                "/repos/jackuri/juicebox-packs/issues/12/reactions/1",
                "/repos/jackuri/juicebox-packs/issues/12/reactions/4"
            ]
        );
        let post = seen
            .iter()
            .find(|(m, p, _)| m == "POST" && p.ends_with("/12/reactions"))
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&post.2).unwrap()["content"],
            "confused"
        );

        let (hub, seen) = fake(route);
        let e = run(rate(&hub, "tok", 13, 3)).unwrap_err();
        assert!(e.message.contains("rating page"), "{}", e.message);
        assert!(
            lock(&seen).iter().all(|(m, _, _)| m == "GET"),
            "nothing changed on another issue"
        );
        assert!(run(rate(&hub, "tok", 12, 4)).is_err());
        assert!(run(rate(&hub, "tok", 12, 0)).is_err());
    }

    #[test]
    fn shares_a_table_link_as_one_file() {
        let (hub, seen) = fake(github_like("sam", false));
        let link = table_link(
            &credited("https://fearlessrevolution.com/viewtopic.php?t=99"),
            "2026-11-09",
        )
        .unwrap();
        let json = serde_json::to_string_pretty(&link).unwrap() + "\n";
        let file = format!("tables/{}.json", link.id);
        let done = run(send_file(
            &hub,
            "tok",
            &file,
            &json,
            "table-x",
            42,
            |_| "Add table link".into(),
            |u, l| table_pr_text(&link, u, l),
        ))
        .unwrap();
        assert_eq!(done.number, 7);
        let seen = lock(&seen).clone();
        let put = seen
            .iter()
            .find(|(m, p, _)| m == "PUT" && p.ends_with(&file))
            .expect("the link file is written");
        let put: Value = serde_json::from_str(&put.2).unwrap();
        let content = base64::engine::general_purpose::STANDARD
            .decode(put["content"].as_str().unwrap())
            .unwrap();
        let back: community::TableLink = serde_json::from_slice(&content).unwrap();
        assert_eq!(back, link);
        assert!(
            seen.iter()
                .all(|(m, p, _)| m != "PUT" || !p.contains("/packs/"))
        );
    }

    #[test]
    fn shares_web_cheats_as_one_file_by_the_list_rules() {
        let dir = std::env::temp_dir().join(format!("ot-share-web-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("web-cheats")).unwrap();
        let games = |url: &str| {
            json!([{ "id": "outbreak-1", "name": "Outbreak", "url": url, "added": "2026-11-10" }])
                .to_string()
        };
        std::fs::write(
            dir.join("web-games.json"),
            games("https://outbreakweb.com/"),
        )
        .unwrap();
        let cheat = json!({ "name": "Gold @everyone", "target": { "k": "j", "path": ["game", "gold"] }, "value": 999 });
        std::fs::write(
            dir.join("web-cheats").join("outbreak-1.json"),
            json!([cheat]).to_string(),
        )
        .unwrap();

        let pack = web_shareable(&dir, "outbreak-1", "Sam", Some("level 2"), "2026-11-10").unwrap();
        let (title, body) = web_pr_text(&pack, false, "sam");
        assert_eq!(title, "Add web cheats: Outbreak");
        assert!(body.contains("https://outbreakweb.com/"), "{body}");
        assert!(body.contains("@\u{200B}everyone"), "no mentions: {body}");
        assert!(body.contains("invited friends"));

        let (hub, seen) = fake(github_like("sam", false));
        let json = serde_json::to_string_pretty(&pack).unwrap();
        let file = format!("web/{}.json", pack.id);
        run(send_file(
            &hub,
            "tok",
            &file,
            &json,
            "web-x",
            42,
            |_| "Add web cheats".into(),
            |u, l| web_pr_text(&pack, u, l),
        ))
        .unwrap();
        let seen = lock(&seen).clone();
        let put = seen
            .iter()
            .find(|(m, p, _)| m == "PUT" && p.ends_with(&file))
            .expect("the file is written");
        let put: Value = serde_json::from_str(&put.2).unwrap();
        let content = base64::engine::general_purpose::STANDARD
            .decode(put["content"].as_str().unwrap())
            .unwrap();
        assert_eq!(jb_packs::web::parse_pack(&content).unwrap(), pack);

        // A game on this PC isn't shared with everyone (files to friends still work).
        std::fs::write(dir.join("web-games.json"), games("http://127.0.0.1:8080/")).unwrap();
        assert!(web_shareable(&dir, "outbreak-1", "Sam", None, "2026-11-10").is_err());
        assert!(crate::web::pack_for(&dir, "outbreak-1", "Sam", None, "2026-11-10").is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
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
    fn pack_text_cant_mention_people_or_add_links() {
        let (mut pack, _) = jb_packs::parse(&pack_text("sam")).unwrap();
        pack.cheats[0].name = "@org/team see [this](https://evil.example) <img>".into();
        pack.game.name = "Game @everyone\nsecond line".into();
        let r = ShareRequest {
            note: "**bold** @someone `x`".into(),
            from_someone_else: false,
            permission: false,
        };
        let (title, body) = pr_text(&pack, &r, false, "sam");
        assert!(
            !title.contains('\n') && title.contains("@\u{200B}everyone"),
            "{title}"
        );
        assert!(
            body.contains("@\u{200B}org") && body.contains("@\u{200B}someone"),
            "{body}"
        );
        assert!(
            !body.contains("](") && !body.contains("<img>") && !body.contains("**bold**"),
            "{body}"
        );
        assert!(
            body.contains("by @sam."),
            "the sharer is still named: {body}"
        );
    }

    #[test]
    fn packs_from_someone_else_need_permission() {
        let text = pack_text("someone");
        let mut r = ShareRequest {
            note: String::new(),
            from_someone_else: true,
            permission: false,
        };
        assert_eq!(shareable(&text, &r).unwrap_err().kind, "needsPermission");
        r.permission = true;
        assert!(shareable(&text, &r).is_ok());
        r.from_someone_else = false;
        r.permission = false;
        assert!(
            shareable(&text, &r).is_ok(),
            "the player's own pack needs no extra promise"
        );
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
            "https://github.com/jackuri/juicebox-packs/pull/7"
        ));
        assert!(!is_pr_url(
            "https://github.com/jackuri/juicebox-packs/pull/"
        ));
        assert!(!is_pr_url(
            "https://github.com/jackuri/juicebox-packs/pull/7?x=1"
        ));
        assert!(!is_pr_url(
            "https://evil.example/jackuri/juicebox-packs/pull/7"
        ));
        assert!(!is_pr_url(
            "https://github.com/jackuri/juicebox-packs/pull/7/../../../x"
        ));
    }

    #[test]
    fn keeps_the_sign_in_in_credential_manager() {
        let target = format!("JuiceBoxTrainer/test-{}", std::process::id());
        vault::save(&target, "secret-token").unwrap();
        assert_eq!(vault::load(&target).as_deref(), Some("secret-token"));
        vault::delete(&target);
        assert_eq!(vault::load(&target), None);
    }

    /// Runs the real sign-in and share against GitHub. A person approves the
    /// code (written to `JB_LIVE_CODE_FILE`); shares the pack at `JB_LIVE_PACK`.
    #[test]
    #[ignore = "talks to the real GitHub and needs a person to approve the code"]
    fn live_share() {
        let pack = std::path::PathBuf::from(std::env::var("JB_LIVE_PACK").unwrap());
        let code_file = std::env::var("JB_LIVE_CODE_FILE").unwrap();
        let text = jb_packs::share::open_path(&pack, None).unwrap().text;
        let hub = Hub::github().unwrap();
        let client = client_id().unwrap();
        let (code, device, interval) = run(start_device(&hub, &client)).unwrap();
        std::fs::write(&code_file, &code.user_code).unwrap();
        let cancel = AtomicBool::new(false);
        let token = run(wait_for_token(
            &hub, &client, &device, interval, &cancel, 120,
        ))
        .unwrap();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let note = ShareRequest {
            note: "Test of the app's sharing by JuiceBox Trainer's developer; will be closed."
                .into(),
            from_someone_else: true,
            permission: true,
        };
        let done = run(share(&hub, &token, &text, &note, stamp)).unwrap();
        std::fs::write(
            format!("{code_file}.done"),
            serde_json::to_string(&done).unwrap(),
        )
        .unwrap();
    }
}
