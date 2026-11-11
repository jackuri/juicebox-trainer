//! Installed games: Steam, Epic, GOG and Xbox (PC Game Pass) installs, plus
//! games the player adds by hand.
//!
//! Only the stores' own listing files and registry keys are read (Steam's
//! `.acf` / `.vdf` text, Epic's `.item` JSON, GOG's registry entries). No game
//! file is opened except to look at exe names and sizes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW, RegGetValueW,
    RegOpenKeyExW,
};
use windows::core::{HSTRING, PCWSTR, PWSTR};

use crate::steam::{kv_pairs, steam_dir};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StoreKind {
    Steam,
    Epic,
    Gog,
    Xbox,
    /// Added by the player.
    Own,
}

/// How to start a game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "target")]
pub enum Launch {
    /// A store link (`steam://…`, `com.epicgames.launcher://…`).
    Uri(String),
    /// Start this exe in its folder.
    Exe(String),
    /// Start it yourself (e.g. from the Xbox app); JuiceBox Trainer waits for it.
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Install {
    /// `steam:620`, `epic:<AppName>`, `gog:<id>`, `xbox:<folder>`, `own:<exe>`.
    pub id: String,
    pub name: String,
    pub store: StoreKind,
    pub folder: String,
    /// The game's main exe if the store says so (Epic, GOG, own games).
    pub exe: Option<String>,
    pub launch: Launch,
}

/// Every installed game found on this PC, by name. Missing folders are left
/// out.
pub fn detect() -> Vec<Install> {
    let mut all = Vec::new();
    if let Some(steam) = steam_dir() {
        all.extend(steam_installs(&steam));
    }
    if let Some(data) = std::env::var_os("ProgramData") {
        all.extend(epic_installs(
            &PathBuf::from(data).join(r"Epic\EpicGamesLauncher\Data\Manifests"),
        ));
    }
    all.extend(gog_installs());
    all.extend(xbox_installs());
    let mut seen = HashSet::new();
    all.retain(|g| Path::new(&g.folder).is_dir() && seen.insert(same_path(&g.folder)));
    all.sort_by_key(|g| g.name.to_lowercase());
    all
}

/// A path in one spelling: backslashes, lowercase, no trailing slash.
fn same_path(p: &str) -> String {
    p.replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

// ----- Steam -----------------------------------------------------------------

/// Steam tools that aren't games.
const STEAM_NOT_GAMES: &[&str] = &[
    "steamworks common redistributables",
    "proton",
    "steam linux runtime",
    "steamvr",
    "wallpaper engine",
];

/// Library folders listed in `libraryfolders.vdf` (`"path"` entries).
pub(crate) fn library_folders(vdf: &str) -> Vec<PathBuf> {
    kv_pairs(vdf)
        .into_iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("path"))
        .map(|(_, v)| PathBuf::from(v.replace("\\\\", "\\")))
        .collect()
}

/// `(app id, name, install dir)` from an `appmanifest_*.acf`.
pub(crate) fn parse_acf(text: &str) -> Option<(u32, String, String)> {
    let pairs = kv_pairs(text);
    let get = |k: &str| {
        pairs
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(k))
            .map(|(_, v)| v.clone())
    };
    Some((
        get("appid")?.parse().ok()?,
        get("name")?,
        get("installdir")?,
    ))
}

pub fn steam_installs(steam: &Path) -> Vec<Install> {
    let mut libraries = vec![PathBuf::from(steam.to_string_lossy().replace('/', "\\"))];
    if let Ok(vdf) = std::fs::read_to_string(steam.join(r"steamapps\libraryfolders.vdf")) {
        libraries.extend(library_folders(&vdf));
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for lib in libraries {
        if !seen.insert(same_path(&lib.to_string_lossy())) {
            continue;
        }
        let apps = lib.join("steamapps");
        let Ok(entries) = std::fs::read_dir(&apps) else {
            continue;
        };
        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !(file.starts_with("appmanifest_") && file.ends_with(".acf")) {
                continue;
            }
            let Some((app_id, name, dir)) = std::fs::read_to_string(entry.path())
                .ok()
                .as_deref()
                .and_then(parse_acf)
            else {
                continue;
            };
            let lower = name.to_lowercase();
            if STEAM_NOT_GAMES.iter().any(|n| lower.contains(n)) {
                continue;
            }
            out.push(Install {
                id: format!("steam:{app_id}"),
                name,
                store: StoreKind::Steam,
                folder: apps.join("common").join(dir).display().to_string(),
                exe: None,
                launch: Launch::Uri(format!("steam://rungameid/{app_id}")),
            });
        }
    }
    out
}

// ----- Epic ------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EpicItem {
    display_name: String,
    install_location: String,
    #[serde(default)]
    launch_executable: String,
    app_name: String,
    #[serde(default)]
    catalog_namespace: String,
    #[serde(default)]
    catalog_item_id: String,
    #[serde(default)]
    app_categories: Vec<String>,
    #[serde(default, rename = "bIsIncompleteInstall")]
    incomplete: bool,
}

pub(crate) fn parse_epic_item(text: &str) -> Option<Install> {
    let item: EpicItem = serde_json::from_str(text).ok()?;
    if item.incomplete
        || (!item.app_categories.is_empty() && !item.app_categories.iter().any(|c| c == "games"))
    {
        return None;
    }
    let exe = (!item.launch_executable.is_empty()).then(|| {
        Path::new(&item.install_location)
            .join(&item.launch_executable)
            .display()
            .to_string()
    });
    let uri = format!(
        "com.epicgames.launcher://apps/{}%3A{}%3A{}?action=launch&silent=true",
        item.catalog_namespace, item.catalog_item_id, item.app_name
    );
    Some(Install {
        id: format!("epic:{}", item.app_name),
        name: item.display_name,
        store: StoreKind::Epic,
        folder: item.install_location,
        exe,
        launch: Launch::Uri(uri),
    })
}

pub fn epic_installs(manifests: &Path) -> Vec<Install> {
    let Ok(entries) = std::fs::read_dir(manifests) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("item"))
        })
        .filter_map(|e| parse_epic_item(&std::fs::read_to_string(e.path()).ok()?))
        .collect()
}

// ----- GOG -------------------------------------------------------------------

const GOG_KEY: &str = r"SOFTWARE\WOW6432Node\GOG.com\Games";

fn hklm_string(key: &str, value: &str) -> Option<String> {
    let key = HSTRING::from(key);
    let value = HSTRING::from(value);
    let mut buf = vec![0u16; 2048];
    let mut len = (buf.len() * 2) as u32;
    // SAFETY: `buf` is writable for `len` bytes; key and value names are
    // NUL-terminated wide strings that outlive the call.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut len),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let chars = (len as usize / 2).min(buf.len());
    let s = String::from_utf16_lossy(&buf[..chars])
        .trim_end_matches('\0')
        .to_string();
    (!s.is_empty()).then_some(s)
}

/// Names of the subkeys of `HKLM\<key>`.
fn hklm_subkeys(key: &str) -> Vec<String> {
    let path = HSTRING::from(key);
    let mut handle = HKEY::default();
    // SAFETY: opens a key for reading; the handle is closed below.
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(path.as_ptr()),
            None,
            KEY_READ,
            &mut handle,
        )
    };
    if status != ERROR_SUCCESS {
        return Vec::new();
    }
    let mut out = Vec::new();
    for index in 0..4096u32 {
        let mut name = [0u16; 256];
        let mut len = name.len() as u32;
        // SAFETY: `name` is writable for `len` characters.
        let status = unsafe {
            RegEnumKeyExW(
                handle,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &mut len,
                None,
                None,
                None,
                None,
            )
        };
        if status != ERROR_SUCCESS {
            break;
        }
        out.push(String::from_utf16_lossy(&name[..len as usize]));
    }
    // SAFETY: closes the key opened above, once.
    let _ = unsafe { RegCloseKey(handle) };
    out
}

pub fn gog_installs() -> Vec<Install> {
    hklm_subkeys(GOG_KEY)
        .into_iter()
        .filter_map(|id| {
            let key = format!(r"{GOG_KEY}\{id}");
            let name = hklm_string(&key, "gameName")?;
            let folder = hklm_string(&key, "path")?;
            let exe = hklm_string(&key, "exe");
            Some(Install {
                id: format!("gog:{id}"),
                name,
                store: StoreKind::Gog,
                folder,
                launch: exe.clone().map_or(Launch::Manual, Launch::Exe),
                exe,
            })
        })
        .collect()
}

// ----- Xbox ------------------------------------------------------------------

/// `X:\XboxGames\<Game>\Content` folders on fixed drives. These games start
/// from the Xbox app (their exes can't be started directly), so JuiceBox Trainer
/// waits for them instead.
pub fn xbox_installs() -> Vec<Install> {
    let mut out = Vec::new();
    for letter in b'C'..=b'Z' {
        let root = PathBuf::from(format!("{}:\\XboxGames", letter as char));
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for e in entries.flatten() {
            let content = e.path().join("Content");
            if !content.is_dir() {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            if name.eq_ignore_ascii_case("GameSave") {
                continue;
            }
            out.push(Install {
                id: format!("xbox:{}", content.display()),
                name,
                store: StoreKind::Xbox,
                folder: content.display().to_string(),
                exe: None,
                launch: Launch::Manual,
            });
        }
    }
    out
}

// ----- the player's own games -----------------------------------------------

/// A game added by hand: just its exe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnGame {
    pub name: String,
    pub exe: String,
}

impl OwnGame {
    pub fn install(&self) -> Install {
        let exe = Path::new(&self.exe);
        Install {
            id: format!("own:{}", self.exe.to_lowercase()),
            name: self.name.clone(),
            store: StoreKind::Own,
            folder: exe
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            exe: Some(self.exe.clone()),
            launch: Launch::Exe(self.exe.clone()),
        }
    }
}

/// The player's own games, from `own-games.json` in `dir`.
pub fn load_own(dir: &Path) -> Vec<OwnGame> {
    std::fs::read_to_string(dir.join("own-games.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_own(dir: &Path, games: &[OwnGame]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let text = serde_json::to_string_pretty(games).map_err(std::io::Error::other)?;
    let path = dir.join("own-games.json");
    let tmp = dir.join("own-games.json.tmp");
    std::fs::write(&tmp, text + "\n")?;
    std::fs::rename(&tmp, path)
}

/// A game from its exe path; the name is the exe's name until the player
/// changes it.
pub fn own_game(exe: &Path) -> Result<OwnGame, String> {
    let is_exe = exe
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("exe"));
    if !is_exe || !exe.is_file() {
        return Err(format!("{} is not an .exe file on this PC", exe.display()));
    }
    let name = exe
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(OwnGame {
        name,
        exe: exe.display().to_string(),
    })
}

// ----- exes in an install folder --------------------------------------------

/// Exe names that aren't the game itself.
const NOT_THE_GAME: &[&str] = &[
    "unins",
    "crash",
    "redist",
    "setup",
    "vc_redist",
    "dxsetup",
    "dotnet",
    "prereq",
    "install",
    "report",
    "helper",
    "easyanticheat",
    "battleye",
    "cefprocess",
    "webhelper",
];

/// Exes under `folder` (a few levels deep), biggest first. Bounded, so a
/// huge folder doesn't stall the library.
pub fn exes_in(folder: &Path) -> Vec<(PathBuf, u64)> {
    let mut out = Vec::new();
    let mut stack = vec![(folder.to_path_buf(), 0usize)];
    let mut visited = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            visited += 1;
            if visited > 5000 {
                break;
            }
            let Ok(ft) = e.file_type() else { continue };
            let path = e.path();
            if ft.is_dir() && depth < 4 {
                stack.push((path, depth + 1));
            } else if ft.is_file()
                && path
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("exe"))
            {
                let lower = e.file_name().to_string_lossy().to_lowercase();
                if NOT_THE_GAME.iter().any(|n| lower.contains(n)) {
                    continue;
                }
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                out.push((path, size));
            }
        }
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.1));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_steam_library_files() {
        let vdf = r#""libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"apps" { "228980" "1" }
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
	}
}"#;
        assert_eq!(
            library_folders(vdf),
            [
                PathBuf::from(r"C:\Program Files (x86)\Steam"),
                PathBuf::from(r"D:\SteamLibrary")
            ]
        );
        let acf = r#""AppState"
{
	"appid"		"620"
	"name"		"Portal 2"
	"installdir"		"Portal 2"
}"#;
        assert_eq!(
            parse_acf(acf),
            Some((620, "Portal 2".into(), "Portal 2".into()))
        );
        assert_eq!(parse_acf("{}"), None);
    }

    #[test]
    fn reads_epic_manifests() {
        let item = r#"{
            "DisplayName": "Some Game", "InstallLocation": "D:\\Epic\\SomeGame",
            "LaunchExecutable": "Bin\\Game.exe", "AppName": "abc123",
            "CatalogNamespace": "ns", "CatalogItemId": "item", "AppCategories": ["public", "games"],
            "bIsIncompleteInstall": false, "Other": 1
        }"#;
        let g = parse_epic_item(item).unwrap();
        assert_eq!(g.id, "epic:abc123");
        assert_eq!(g.exe.as_deref(), Some(r"D:\Epic\SomeGame\Bin\Game.exe"));
        assert_eq!(
            g.launch,
            Launch::Uri(
                "com.epicgames.launcher://apps/ns%3Aitem%3Aabc123?action=launch&silent=true".into()
            )
        );
        let tool = item.replace("\"games\"", "\"applications\"");
        assert_eq!(parse_epic_item(&tool), None);
        let partial = item.replace(
            "\"bIsIncompleteInstall\": false",
            "\"bIsIncompleteInstall\": true",
        );
        assert_eq!(parse_epic_item(&partial), None);
    }

    #[test]
    fn own_games_round_trip_and_pick_biggest_exe() {
        let dir = crate::test_dir("installs-own");
        let game = dir.join("Game");
        std::fs::create_dir_all(game.join("Binaries")).unwrap();
        std::fs::write(game.join("Game.exe"), [0u8; 10]).unwrap();
        std::fs::write(game.join(r"Binaries\Game-Shipping.exe"), [0u8; 100]).unwrap();
        std::fs::write(game.join("unins000.exe"), [0u8; 1000]).unwrap();
        let exes = exes_in(&game);
        assert_eq!(exes.len(), 2);
        assert!(exes[0].0.ends_with(r"Binaries\Game-Shipping.exe"));

        let own = own_game(&game.join("Game.exe")).unwrap();
        assert_eq!(own.name, "Game");
        assert!(own_game(&game.join("nope.exe")).is_err());
        save_own(&dir, std::slice::from_ref(&own)).unwrap();
        assert_eq!(load_own(&dir), std::slice::from_ref(&own));
        let i = own.install();
        assert_eq!(i.store, StoreKind::Own);
        assert_eq!(i.folder, game.display().to_string());
    }

    #[test]
    fn detect_runs_on_this_pc() {
        // Whatever is installed here; just make sure it doesn't fail and
        // every folder exists.
        for g in detect() {
            assert!(Path::new(&g.folder).is_dir(), "{g:?}");
        }
    }
}

#[cfg(test)]
mod show {
    #[test]
    #[ignore = "prints this PC's games"]
    fn show_installs() {
        for g in super::detect() {
            println!("{:?} | {} | {} | {:?}", g.store, g.name, g.folder, g.launch);
        }
    }
}
