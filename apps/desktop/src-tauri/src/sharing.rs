//! Sharing packs: `.otpack` export / import, removing packs, and the
//! community pack list (PLAN.md §2.5, §8.3). Every pack that comes in goes
//! through the normal validator, and a different pack already installed for
//! the same game is only replaced after the player says so.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ot_packs::community::{self, Entry};
use ot_packs::store::{self, Installed, OriginKind};
use ot_packs::{Issue, Pack, share};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::commands::{CmdResult, CommandError};

/// Event the UI gets when an `.otpack` is opened from Explorer.
pub const OPEN_EVENT: &str = "otpack-open";

#[derive(Debug, Default)]
pub struct SharingState {
    /// An `.otpack` given on the command line before the UI was ready.
    pending: Mutex<Option<String>>,
    /// The last community list download, for a few minutes.
    index: Mutex<Option<Cached>>,
}

#[derive(Debug, Clone)]
struct Cached {
    at: Instant,
    url: String,
    index: community::Index,
    skipped: Vec<String>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn io(e: impl std::fmt::Display) -> CommandError {
    CommandError::new("io", e.to_string())
}

fn user_dir() -> CmdResult<PathBuf> {
    store::user_dir().ok_or_else(|| CommandError::new("noAppData", "%APPDATA% is not set"))
}

/// The `.otpack` among command-line arguments, if any.
pub fn otpack_arg<I: IntoIterator<Item = String>>(args: I) -> Option<String> {
    args.into_iter().skip(1).find(|a| {
        Path::new(a)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(share::EXTENSION))
    })
}

/// Hands an `.otpack` opened from Explorer to the UI.
pub fn open_from_shell(app: &AppHandle, path: String) {
    *lock(&app.state::<SharingState>().pending) = Some(path.clone());
    let _ = app.emit(OPEN_EVENT, path);
}

/// The `.otpack` opened from Explorer that the UI hasn't shown yet.
#[tauri::command]
pub fn packs_pending_open(state: State<'_, SharingState>) -> Option<String> {
    lock(&state.pending).take()
}

// ----- export ---------------------------------------------------------------

fn pack_text(key: &str) -> CmdResult<String> {
    let found = store::all()
        .into_iter()
        .find(|f| f.key == key)
        .ok_or_else(|| CommandError::new("notFound", format!("no pack '{key}'")))?;
    found.text.map_err(io)
}

/// Saves a pack as an `.otpack` where the player picks. `None` if they
/// cancelled.
#[tauri::command]
pub async fn packs_export(key: String, app: AppHandle) -> CmdResult<Option<String>> {
    let text = pack_text(&key)?;
    let (pack, _) =
        ot_packs::parse(&text).map_err(|e| CommandError::new("invalidPack", e.to_string()))?;
    let bytes = share::export(&text, env!("CARGO_PKG_VERSION"), &share::today())
        .map_err(|e| CommandError::new("invalidPack", e.to_string()))?;
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Save the pack to share")
        .set_file_name(share::file_name(&pack))
        .add_filter("OpenTrainer pack", &[share::EXTENSION]);
    if let Ok(docs) = app.path().document_dir() {
        dialog = dialog.set_directory(docs);
    }
    let Some(path) = dialog.blocking_save_file() else {
        return Ok(None);
    };
    let mut path = path.into_path().map_err(io)?;
    if path.extension().is_none() {
        path.set_extension(share::EXTENSION);
    }
    std::fs::write(&path, bytes).map_err(|e| io(format!("{}: {e}", path.display())))?;
    Ok(Some(path.display().to_string()))
}

/// Shows a file in Explorer.
#[tauri::command]
pub async fn packs_show_file(path: String) -> CmdResult<()> {
    std::process::Command::new("explorer.exe")
        .arg(format!("/select,{path}"))
        .spawn()
        .map_err(io)?;
    Ok(())
}

// ----- import ---------------------------------------------------------------

/// Lets the player pick an `.otpack` or a plain `pack.json`. `None` if they
/// cancelled.
#[tauri::command]
pub async fn packs_pick_otpack(app: AppHandle) -> CmdResult<Option<String>> {
    let picked = app
        .dialog()
        .file()
        .set_title("Open a pack")
        .add_filter(
            "OpenTrainer pack or Cheat Engine table",
            &[share::EXTENSION, "json", ot_packs::ct::EXTENSION, "CT"],
        )
        .blocking_pick_file();
    picked
        .map(|p| p.into_path().map(|p| p.display().to_string()).map_err(io))
        .transpose()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheatLine {
    pub name: String,
    pub ui: ot_packs::Ui,
    pub description: Option<String>,
    /// Changes game code (as opposed to values).
    pub patch: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Existing {
    pub key: String,
    pub name: String,
    pub version: String,
    pub ai: bool,
    /// Exactly the same pack.
    pub same: bool,
}

/// What a pack that is about to be installed contains.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackPreview {
    pub id: String,
    pub game: String,
    pub exe: String,
    pub author: String,
    pub ai: bool,
    pub version: String,
    pub notes: Option<String>,
    pub cheats: Vec<CheatLine>,
    pub warnings: Vec<Issue>,
    /// The pack this would replace.
    pub existing: Option<Existing>,
    /// Cheat Engine table entries that didn't come over, and why.
    pub skipped: Vec<ot_packs::ct::Skipped>,
    /// A converted table that doesn't say which exe it's for.
    pub needs_exe: bool,
}

fn preview(o: share::Opened) -> PackPreview {
    let share::Opened {
        pack,
        text,
        warnings,
        skipped,
        needs_exe,
        ..
    } = o;
    let mut p = preview_pack(&pack, &text, warnings);
    p.skipped = skipped;
    p.needs_exe = needs_exe;
    p
}

fn preview_pack(pack: &Pack, text: &str, warnings: Vec<Issue>) -> PackPreview {
    let existing = store::user_dir().and_then(|dir| {
        let path = dir.join(&pack.game.id).join("pack.json");
        let old = std::fs::read_to_string(&path).ok()?;
        let parsed = ot_packs::parse(&old).ok().map(|(p, _)| p);
        Some(Existing {
            key: path.display().to_string(),
            name: parsed.as_ref().map_or_else(
                || "a pack that doesn't load".into(),
                |p| p.game.name.clone(),
            ),
            version: parsed
                .as_ref()
                .map_or_else(|| "?".into(), |p| p.meta.pack_version.clone()),
            ai: parsed.as_ref().is_some_and(|p| p.meta.is_ai()),
            same: old == text,
        })
    });
    PackPreview {
        id: pack.game.id.clone(),
        game: pack.game.name.clone(),
        exe: pack.game.exe.clone(),
        author: pack.meta.author.clone(),
        ai: pack.meta.is_ai(),
        version: pack.meta.pack_version.clone(),
        notes: pack.meta.notes.clone(),
        cheats: pack
            .cheats
            .iter()
            .map(|c| CheatLine {
                name: c.name.clone(),
                ui: c.ui,
                description: c.description.clone(),
                patch: matches!(c.implementation, ot_packs::Impl::Patch(_)),
            })
            .collect(),
        warnings,
        existing,
        skipped: Vec::new(),
        needs_exe: false,
    }
}

/// Opens an `.otpack`, a plain `pack.json` or a Cheat Engine table. `exe`
/// names the game's exe for a table that doesn't say.
fn open_otpack(path: &str, exe: Option<&str>) -> CmdResult<share::Opened> {
    share::open_path(Path::new(path), exe)
        .map_err(|e| CommandError::new("notOtpack", e.to_string()))
}

/// What an `.otpack`, `pack.json` or Cheat Engine table contains.
#[tauri::command]
pub async fn packs_preview(path: String, exe: Option<String>) -> CmdResult<PackPreview> {
    let o = open_otpack(&path, exe.as_deref())?;
    Ok(preview(o))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    /// Pass to `trainer_load`.
    pub key: String,
    pub installed: Installed,
}

fn install(
    pack: &Pack,
    text: &str,
    kind: OriginKind,
    from: Option<String>,
    replace: bool,
) -> CmdResult<ImportResult> {
    let dir = user_dir()?;
    match store::install(&dir, pack, text, kind, from, replace) {
        Ok((path, installed)) => Ok(ImportResult {
            key: path.display().to_string(),
            installed,
        }),
        Err(e @ store::InstallError::Exists { .. }) => {
            Err(CommandError::new("packExists", e.to_string()))
        }
        Err(e) => Err(io(e)),
    }
}

/// Installs an `.otpack`, `pack.json` or Cheat Engine table. A different pack
/// for the same game is only replaced with `replace`.
#[tauri::command]
pub async fn packs_import(
    path: String,
    replace: bool,
    exe: Option<String>,
) -> CmdResult<ImportResult> {
    let o = open_otpack(&path, exe.as_deref())?;
    if o.needs_exe {
        return Err(CommandError::new(
            "needsExe",
            "choose the game's exe first: the table doesn't say which game it's for",
        ));
    }
    if o.pack.cheats.is_empty() {
        return Err(CommandError::new(
            "invalidPack",
            "nothing in this table could be imported",
        ));
    }
    let name = Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned());
    install(&o.pack, &o.text, OriginKind::File, name, replace)
}

/// Moves one of the player's packs out of the pack folder (kept in
/// `packs-removed`, not deleted).
#[tauri::command]
pub async fn packs_remove(key: String) -> CmdResult<String> {
    let dir = user_dir()?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    store::remove(&dir, Path::new(&key), &stamp.to_string())
        .map(|p| p.display().to_string())
        .map_err(|e| CommandError::new("notYours", e))
}

// ----- community list -------------------------------------------------------

/// The list's address: the public repository, or `OT_PACK_INDEX` (https, or a
/// local test server).
fn index_url() -> String {
    std::env::var("OT_PACK_INDEX")
        .ok()
        .filter(|u| {
            u.starts_with("https://")
                || u.starts_with("http://127.0.0.1:")
                || u.starts_with("http://localhost:")
        })
        .unwrap_or_else(|| community::INDEX_URL.to_string())
}

/// Downloads `url`, refusing more than `limit` bytes.
async fn fetch(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .user_agent(concat!("OpenTrainer/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let offline = |e: reqwest::Error| {
        if e.is_connect() || e.is_timeout() {
            "couldn't reach the pack list; check your internet connection".to_string()
        } else {
            e.to_string()
        }
    };
    let mut resp = client.get(url).send().await.map_err(offline)?;
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err("the pack list isn't online yet".into());
    }
    if !status.is_success() {
        return Err(format!("the pack list answered {status}"));
    }
    if resp.content_length().is_some_and(|n| n > limit) {
        return Err("the download is too big".into());
    }
    let mut out = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(offline)? {
        out.extend_from_slice(&chunk);
        if out.len() as u64 > limit {
            return Err("the download is too big".into());
        }
    }
    Ok(out)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityPack {
    #[serde(flatten)]
    pub entry: Entry,
    /// `none`, `installed` (this exact pack), `update` (an older or
    /// different copy from the list) or `other` (another pack for the game).
    pub installed: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityList {
    pub url: String,
    pub contribute_url: &'static str,
    pub updated: Option<String>,
    pub packs: Vec<CommunityPack>,
    /// Entries left out because they were broken.
    pub skipped: Vec<String>,
}

fn installed_state(entry: &Entry) -> &'static str {
    let Some(dir) = store::user_dir() else {
        return "none";
    };
    let path = dir.join(&entry.id).join("pack.json");
    if !path.is_file() {
        return "none";
    }
    match store::origin_of(&path) {
        Some(o) if o.kind == OriginKind::Community && o.sha256 == entry.sha256 => "installed",
        Some(o) if o.kind == OriginKind::Community => "update",
        _ => "other",
    }
}

const CACHE_FOR: Duration = Duration::from_secs(600);

async fn index(state: &SharingState, refresh: bool) -> CmdResult<Cached> {
    let url = index_url();
    if !refresh
        && let Some(c) = lock(&state.index).as_ref()
        && c.at.elapsed() < CACHE_FOR
        && c.url == url
    {
        return Ok(c.clone());
    }
    let bytes = fetch(&url, community::MAX_INDEX_BYTES)
        .await
        .map_err(|e| CommandError::new("offline", e))?;
    let (index, skipped) =
        community::parse_index(&bytes).map_err(|e| CommandError::new("badIndex", e))?;
    let cached = Cached {
        at: Instant::now(),
        url,
        index,
        skipped,
    };
    *lock(&state.index) = Some(cached.clone());
    Ok(cached)
}

/// The community pack list (downloaded at most every 10 minutes unless
/// `refresh`).
#[tauri::command]
pub async fn community_list(
    refresh: bool,
    state: State<'_, SharingState>,
) -> CmdResult<CommunityList> {
    let Cached {
        url,
        index,
        skipped,
        ..
    } = index(&state, refresh).await?;
    Ok(CommunityList {
        url,
        contribute_url: community::CONTRIBUTE_URL,
        updated: index.updated,
        packs: index
            .packs
            .into_iter()
            .map(|entry| CommunityPack {
                installed: installed_state(&entry),
                entry,
            })
            .collect(),
        skipped,
    })
}

async fn download(state: &SharingState, id: &str) -> CmdResult<(String, Pack, String, Vec<Issue>)> {
    let Cached { url, index, .. } = index(state, false).await?;
    let entry = index
        .packs
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| CommandError::new("notFound", format!("{id} isn't on the list")))?;
    let bytes = fetch(&community::pack_url(&url, entry), store::MAX_PACK_BYTES)
        .await
        .map_err(|e| CommandError::new("offline", e))?;
    let (pack, text, warnings) =
        community::check_download(entry, &bytes).map_err(|e| CommandError::new("badPack", e))?;
    Ok((url, pack, text, warnings))
}

/// Downloads a community pack and shows what it contains (not installed).
#[tauri::command]
pub async fn community_preview(
    id: String,
    state: State<'_, SharingState>,
) -> CmdResult<PackPreview> {
    let (_, pack, text, warnings) = download(&state, &id).await?;
    Ok(preview_pack(&pack, &text, warnings))
}

/// Downloads and installs a community pack.
#[tauri::command]
pub async fn community_install(
    id: String,
    replace: bool,
    state: State<'_, SharingState>,
) -> CmdResult<ImportResult> {
    let (url, pack, text, _) = download(&state, &id).await?;
    install(&pack, &text, OriginKind::Community, Some(url), replace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_an_otpack_argument() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            otpack_arg(args(&["ot.exe", r"C:\x\My Pack.OTPACK"])),
            Some(r"C:\x\My Pack.OTPACK".into())
        );
        assert_eq!(otpack_arg(args(&["ot.exe", "--mcp"])), None);
        // The program itself never counts.
        assert_eq!(otpack_arg(args(&["a.otpack"])), None);
    }

    #[test]
    fn previews_say_what_a_pack_does() {
        let text = store::BUNDLED[0].1;
        let (pack, warnings) = ot_packs::parse(text).unwrap();
        let p = preview_pack(&pack, text, warnings);
        assert_eq!(p.id, "testbed-native");
        assert!(p.cheats.iter().any(|c| c.name == "Infinite Health"));
    }

    #[test]
    fn downloads_with_a_size_limit() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for _ in 0..3 {
                let Ok((mut s, _)) = listener.accept() else {
                    return;
                };
                let mut buf = [0u8; 2048];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let body = if req.starts_with("GET /big") {
                    "x".repeat(5000)
                } else {
                    "hello".to_string()
                };
                let status = if req.starts_with("GET /missing") {
                    "404 Not Found"
                } else {
                    "200 OK"
                };
                let _ = write!(
                    s,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        let base = format!("http://127.0.0.1:{port}");
        let get = |path: &str| tauri::async_runtime::block_on(fetch(&format!("{base}{path}"), 100));
        assert_eq!(get("/ok").unwrap(), b"hello");
        assert!(get("/big").unwrap_err().contains("too big"));
        assert!(get("/missing").unwrap_err().contains("isn't online yet"));
    }
}
