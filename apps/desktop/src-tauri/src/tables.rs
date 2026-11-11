//! Getting cheats for a game from a Cheat Engine table (v1.15.0): open a
//! search for the game's tables on a free site in the player's browser, then
//! notice the `.CT` files the player downloaded so they can be converted in one
//! click. JuiceBox Trainer never downloads or re-uploads tables itself: the player
//! gets them from the maker's own page, and the pack keeps the maker's credit.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jb_packs::{ct, share};
use serde::Serialize;
use tauri::State;

use crate::commands::{CmdResult, CommandError};

/// Tables older than this aren't offered.
const RECENT: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// At most this many are offered (newest first).
const MAX_TABLES: usize = 20;
/// Folders inside Downloads looked into (unzipped downloads), at most.
const MAX_SUBFOLDERS: usize = 200;

/// Where a table search can go. The game's name is the only thing sent.
fn search_url(site: &str, game: &str) -> Option<String> {
    let game = game.trim();
    if game.is_empty() || game.chars().count() > 200 {
        return None;
    }
    match site {
        // FearLess Revolution's own search (phpBB), topic titles only.
        "fearless" => Some(format!(
            "https://fearlessrevolution.com/search.php?keywords={}&terms=all&sf=titleonly&sr=topics",
            encode(game)
        )),
        // Any other free site.
        "web" => Some(format!(
            "https://duckduckgo.com/?q={}",
            encode(&format!("{game} cheat engine table"))
        )),
        _ => None,
    }
}

/// Percent-encodes a URL query value (UTF-8; spaces as `+`).
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Opens a search for the game's cheat tables in the player's browser.
/// `site` is `fearless` or `web`.
#[tauri::command]
pub async fn tables_search(site: String, game: String) -> CmdResult<()> {
    let url = search_url(&site, &game)
        .ok_or_else(|| CommandError::new("badRequest", "unknown site or game name"))?;
    crate::browser::open_url(&url)?;
    Ok(())
}

/// A Cheat Engine table in the player's Downloads folder.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecentTable {
    pub path: String,
    pub file: String,
    /// Last changed, milliseconds since 1970.
    pub modified: u64,
    /// The game, from the file name.
    pub game: String,
    /// The game's exe when the table says which it is.
    pub exe: Option<String>,
    /// Cheats JuiceBox Trainer can use from it.
    pub cheats: usize,
    /// Why it can't be read, if it can't.
    pub error: Option<String>,
}

/// Converted tables, by path, last change and size (so polling stays cheap).
#[derive(Default)]
pub struct TablesState(Mutex<HashMap<(PathBuf, u64, u64), RecentTable>>);

fn downloads() -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Downloads, KNOWN_FOLDER_FLAG, SHGetKnownFolderPath};
    // SAFETY: a valid GUID; the returned string is freed below.
    let path =
        unsafe { SHGetKnownFolderPath(&FOLDERID_Downloads, KNOWN_FOLDER_FLAG(0), None) }.ok()?;
    // SAFETY: on success `path` is a valid NUL-terminated wide string.
    let s = unsafe { path.to_string() }.ok();
    // SAFETY: allocated by SHGetKnownFolderPath with the COM allocator.
    unsafe { CoTaskMemFree(Some(path.0 as *const _)) };
    s.map(PathBuf::from)
}

fn is_table(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ct::EXTENSION))
}

fn millis(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// `.CT` files changed in the last 30 days, in `dir` and the folders directly
/// inside it (unzipped downloads), newest first.
fn find_tables(dir: &Path, now: SystemTime) -> Vec<(PathBuf, u64, u64)> {
    let mut found = Vec::new();
    let mut look = |d: &Path, subfolders: &mut Vec<PathBuf>| {
        let Ok(entries) = std::fs::read_dir(d) else {
            return;
        };
        for e in entries.flatten() {
            let Ok(meta) = e.metadata() else { continue };
            let path = e.path();
            if meta.is_dir() {
                if subfolders.len() < MAX_SUBFOLDERS {
                    subfolders.push(path);
                }
                continue;
            }
            if !meta.is_file() || !is_table(&path) || meta.len() > ct::MAX_TABLE_BYTES {
                continue;
            }
            let Ok(modified) = meta.modified() else {
                continue;
            };
            if now.duration_since(modified).is_ok_and(|age| age > RECENT) {
                continue;
            }
            found.push((path, millis(modified), meta.len()));
        }
    };
    let mut subfolders = Vec::new();
    look(dir, &mut subfolders);
    let mut ignored = Vec::new();
    for sub in subfolders {
        look(&sub, &mut ignored);
    }
    found.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    found.truncate(MAX_TABLES);
    found
}

/// What JuiceBox Trainer can use from one table.
fn read_table(path: &Path, modified: u64) -> RecentTable {
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let base = RecentTable {
        path: path.display().to_string(),
        file,
        modified,
        game: stem.replace(['_', '-'], " ").trim().to_string(),
        exe: None,
        cheats: 0,
        error: None,
    };
    match share::open_path(path, None) {
        Ok(o) => RecentTable {
            game: o.pack.game.name.clone(),
            exe: (!o.needs_exe).then(|| o.pack.game.exe.clone()),
            cheats: o.pack.cheats.len(),
            ..base
        },
        Err(e) => RecentTable {
            error: Some(e.to_string()),
            ..base
        },
    }
}

fn recent_in(
    dir: &Path,
    now: SystemTime,
    cache: &Mutex<HashMap<(PathBuf, u64, u64), RecentTable>>,
) -> Vec<RecentTable> {
    let found = find_tables(dir, now);
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    // Forget files that are gone or changed.
    cache.retain(|k, _| found.contains(k));
    found
        .into_iter()
        .map(|key| {
            cache
                .entry(key.clone())
                .or_insert_with(|| read_table(&key.0, key.1))
                .clone()
        })
        .collect()
}

/// Cheat Engine tables the player downloaded in the last 30 days (Downloads
/// and the folders directly inside it), newest first.
#[tauri::command]
pub async fn tables_recent(state: State<'_, TablesState>) -> CmdResult<Vec<RecentTable>> {
    let Some(dir) = downloads() else {
        return Ok(Vec::new());
    };
    Ok(recent_in(&dir, SystemTime::now(), &state.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_urls_only_carry_the_encoded_name() {
        assert_eq!(
            search_url("fearless", "Hollow Knight").as_deref(),
            Some(
                "https://fearlessrevolution.com/search.php?keywords=Hollow+Knight&terms=all&sf=titleonly&sr=topics"
            )
        );
        assert_eq!(
            search_url("web", "Baldur's Gate 3").as_deref(),
            Some("https://duckduckgo.com/?q=Baldur%27s+Gate+3+cheat+engine+table")
        );
        let tricky = search_url("fearless", "a&b=c#d \"e\" é").unwrap();
        assert!(
            tricky.contains("keywords=a%26b%3Dc%23d+%22e%22+%C3%A9&"),
            "{tricky}"
        );
        assert_eq!(search_url("evil", "x"), None);
        assert_eq!(search_url("web", "  "), None);
        assert_eq!(search_url("web", &"x".repeat(201)), None);
    }

    const TABLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<CheatTable CheatEngineTableVersion="45">
  <CheatEntries>
    <CheatEntry>
      <ID>1</ID>
      <Description>"Gold"</Description>
      <VariableType>4 Bytes</VariableType>
      <Address>"Game.exe"+1234</Address>
    </CheatEntry>
  </CheatEntries>
</CheatTable>
"#;

    #[test]
    fn finds_recent_tables_in_downloads_and_unzipped_folders() {
        let dir = std::env::temp_dir().join(format!("ot-tables-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Unzipped").join("deeper")).unwrap();
        std::fs::write(dir.join("Hollow_Knight.CT"), TABLE).unwrap();
        std::fs::write(dir.join("Unzipped").join("Other Game.ct"), "not a table").unwrap();
        std::fs::write(
            dir.join("Unzipped").join("deeper").join("Too Deep.CT"),
            TABLE,
        )
        .unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();

        let cache = Mutex::new(HashMap::new());
        let now = SystemTime::now() + Duration::from_secs(5);
        let got = recent_in(&dir, now, &cache);
        assert_eq!(got.len(), 2, "{got:#?}");
        let hk = got.iter().find(|t| t.file == "Hollow_Knight.CT").unwrap();
        assert_eq!(hk.game, "Hollow Knight");
        assert_eq!(hk.exe.as_deref(), Some("Game.exe"));
        assert_eq!(hk.cheats, 1);
        assert!(hk.error.is_none());
        let bad = got.iter().find(|t| t.file == "Other Game.ct").unwrap();
        assert!(bad.error.is_some());
        assert_eq!(bad.cheats, 0);
        assert_eq!(cache.lock().unwrap().len(), 2);

        // A month later they're no longer offered.
        let later = SystemTime::now() + RECENT + Duration::from_secs(60);
        assert!(recent_in(&dir, later, &cache).is_empty());
        assert!(cache.lock().unwrap().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
