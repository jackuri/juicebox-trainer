//! Finds where a game keeps its saves (PLAN.md §2.3), from its exe path.
//!
//! Checked, in this order:
//! - Unity: `<exe>_Data\app.info` names the company and product; saves are in
//!   `AppData\LocalLow\<company>\<product>`.
//! - Unreal: `<Project>\Binaries\Win64\<Project>-Win64-Shipping.exe` saves in
//!   `%LOCALAPPDATA%\<Project>\Saved\SaveGames` or `<Project>\Saved\SaveGames`.
//! - Steam Cloud: `Steam\userdata\<account>\<appid>\remote`.
//! - A folder named like the game in `Documents\My Games`, `Documents`,
//!   `Saved Games`, `%APPDATA%`, `%LOCALAPPDATA%` or `LocalLow\<any company>`.
//!
//! Only folders that exist are returned. Nothing here writes anything.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::folders::Roots;
use crate::steam::{self, SteamApp};

/// One folder of save files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveFolder {
    pub path: String,
    /// Why it was picked, e.g. "Unity save folder (TestCo / Testbed)".
    pub reason: String,
    /// Added by the player rather than found.
    pub user_added: bool,
    /// Inside a Steam Cloud `remote` folder.
    pub steam_cloud: bool,
}

/// Steam Cloud status of a game.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudInfo {
    /// Steam may sync this game's saves.
    pub synced: bool,
    pub reason: Option<String>,
}

/// What the locator found for an exe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Located {
    pub name: String,
    pub steam: Option<SteamApp>,
    pub cloud: CloudInfo,
    pub folders: Vec<SaveFolder>,
}

/// Folder names that say nothing about the game.
const GENERIC: &[&str] = &[
    "bin",
    "bin64",
    "bin32",
    "x64",
    "x86",
    "win64",
    "win32",
    "wingdk",
    "binaries",
    "debug",
    "release",
    "game",
    "games",
    "common",
    "steamapps",
    "program files",
    "program files (x86)",
    "retail",
    "shipping",
    "build",
    "builds",
    "app",
    "client",
];

const SHIPPING_SUFFIXES: &[&str] = &[
    "-win64-shipping",
    "-win32-shipping",
    "-wingdk-shipping",
    "-shipping",
];

/// Lowercase letters and digits only: "My Game: Deluxe" → "mygamedeluxe".
pub(crate) fn norm(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_generic(name: &str) -> bool {
    GENERIC.iter().any(|g| g.eq_ignore_ascii_case(name))
}

fn file_name(p: &Path) -> Option<String> {
    p.file_name().map(|n| n.to_string_lossy().into_owned())
}

/// Unity's `<stem>_Data\app.info`: company on line 1, product on line 2.
fn unity_info(exe: &Path) -> Option<(String, String)> {
    let stem = exe.file_stem()?.to_string_lossy().into_owned();
    let info = exe.parent()?.join(format!("{stem}_Data")).join("app.info");
    let text = std::fs::read_to_string(info).ok()?;
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let company = lines.next()?.to_string();
    let product = lines.next()?.to_string();
    let safe = |s: &str| !s.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']) && s != "..";
    (safe(&company) && safe(&product)).then_some((company, product))
}

/// Unreal: the project folder above `Binaries\<platform>`.
fn unreal_project(exe: &Path) -> Option<PathBuf> {
    let platform = exe.parent()?;
    let binaries = platform.parent()?;
    let is_binaries = file_name(binaries).is_some_and(|n| n.eq_ignore_ascii_case("binaries"));
    is_binaries.then(|| binaries.parent().map(Path::to_path_buf))?
}

fn exe_stem(exe: &Path) -> String {
    let stem = exe
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lower = stem.to_ascii_lowercase();
    for suffix in SHIPPING_SUFFIXES {
        if lower.ends_with(suffix) {
            return stem[..stem.len() - suffix.len()].to_string();
        }
    }
    stem
}

struct Finder<'a> {
    exe: &'a Path,
    found: Vec<SaveFolder>,
}

impl Finder<'_> {
    /// Adds `path` if it exists, isn't (inside) the install folder and doesn't
    /// overlap a folder already found.
    fn add(&mut self, path: PathBuf, reason: String, steam_cloud: bool) {
        if !path.is_dir() {
            return;
        }
        let key = path.to_string_lossy().to_lowercase();
        let exe = self.exe.to_string_lossy().to_lowercase();
        if exe.starts_with(&format!("{key}\\")) {
            return;
        }
        let overlaps = self.found.iter().any(|f| {
            let other = f.path.to_lowercase();
            key == other
                || key.starts_with(&format!("{other}\\"))
                || other.starts_with(&format!("{key}\\"))
        });
        if overlaps {
            return;
        }
        self.found.push(SaveFolder {
            path: path.to_string_lossy().into_owned(),
            reason,
            user_added: false,
            steam_cloud,
        });
    }
}

/// Subfolders of `dir` whose normalised name is in `names`.
fn named_children(dir: &Path, names: &BTreeSet<String>) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| names.contains(&norm(&e.file_name().to_string_lossy())))
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

/// Finds save folders for `exe`.
pub fn locate(exe: &Path, roots: &Roots) -> Located {
    let mut f = Finder {
        exe,
        found: Vec::new(),
    };
    let stem = exe_stem(exe);
    let unity = unity_info(exe);
    let unreal = unreal_project(exe);
    let steam_app = steam::app_for_exe(exe);

    // Names the game may go by, best first.
    let mut names: Vec<String> = Vec::new();
    if let Some(n) = steam_app.as_ref().and_then(|a| a.name.clone()) {
        names.push(n);
    }
    if let Some((_, product)) = &unity {
        names.push(product.clone());
    }
    if let Some(project) = unreal.as_deref().and_then(file_name) {
        names.push(project);
    }
    let install = unreal
        .as_deref()
        .and_then(Path::parent)
        .or_else(|| exe.parent())
        .and_then(file_name)
        .filter(|n| !is_generic(n));
    names.extend(install);
    names.push(stem.clone());
    let display = names.first().cloned().unwrap_or_else(|| stem.clone());
    let keys: BTreeSet<String> = names
        .iter()
        .map(|n| norm(n))
        .filter(|k| k.chars().count() >= 4 && k != "opentrainer")
        .collect();

    if let (Some((company, product)), Some(low)) = (&unity, &roots.local_low) {
        f.add(
            low.join(company).join(product),
            format!("Unity save folder ({company} / {product})"),
            false,
        );
    }
    if let Some(project_dir) = &unreal {
        if let (Some(project), Some(local)) = (file_name(project_dir), &roots.local) {
            f.add(
                local.join(&project).join("Saved").join("SaveGames"),
                "Unreal save folder".into(),
                false,
            );
        }
        f.add(
            project_dir.join("Saved").join("SaveGames"),
            "Unreal save folder (next to the game)".into(),
            false,
        );
    }

    let mut cloud = CloudInfo::default();
    if let (Some(app), Some(steam_dir)) = (&steam_app, &roots.steam) {
        let c = steam::cloud_folders(steam_dir, app.app_id);
        for remote in c.remote {
            f.add(
                remote,
                format!("Steam Cloud folder (app {})", app.app_id),
                true,
            );
        }
        if c.synced {
            cloud = CloudInfo {
                synced: true,
                reason: Some(format!(
                    "Steam keeps cloud data for this game (app {})",
                    app.app_id
                )),
            };
        }
    }

    let mut search: Vec<(PathBuf, &str)> = Vec::new();
    if let Some(d) = &roots.documents {
        search.push((d.join("My Games"), r"Documents\My Games"));
        search.push((d.clone(), "Documents"));
    }
    if let Some(d) = &roots.saved_games {
        search.push((d.clone(), "Saved Games"));
    }
    if let Some(d) = &roots.roaming {
        search.push((d.clone(), r"AppData\Roaming"));
    }
    if let Some(d) = &roots.local {
        search.push((d.clone(), r"AppData\Local"));
    }
    if let Some(low) = &roots.local_low {
        search.push((low.clone(), r"AppData\LocalLow"));
        if let Ok(companies) = std::fs::read_dir(low) {
            let mut companies: Vec<PathBuf> = companies
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| e.path())
                .collect();
            companies.sort();
            for c in companies {
                search.push((c, r"AppData\LocalLow"));
            }
        }
    }
    for (dir, label) in search {
        for hit in named_children(&dir, &keys) {
            f.add(
                hit,
                format!(r"Folder named like the game in {label}"),
                false,
            );
        }
    }

    Located {
        name: display,
        steam: steam_app,
        cloud,
        folders: f.found,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        base: PathBuf,
        roots: Roots,
    }

    impl Fake {
        fn new(name: &str) -> Self {
            let base = crate::test_dir(name);
            let roots = Roots {
                documents: Some(base.join("Documents")),
                saved_games: Some(base.join("Saved Games")),
                roaming: Some(base.join("Roaming")),
                local: Some(base.join("Local")),
                local_low: Some(base.join("LocalLow")),
                steam: Some(base.join("Steam")),
            };
            Self { base, roots }
        }

        fn dir(&self, rel: &str) -> PathBuf {
            let p = self.base.join(rel);
            std::fs::create_dir_all(&p).unwrap();
            p
        }

        fn file(&self, rel: &str, text: &str) -> PathBuf {
            let p = self.base.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, text).unwrap();
            p
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    fn paths(l: &Located) -> Vec<String> {
        l.folders.iter().map(|f| f.path.clone()).collect()
    }

    #[test]
    fn unity_game() {
        let t = Fake::new("loc-unity");
        let exe = t.file(r"Games\Testbed\testbed_native.exe", "");
        t.file(
            r"Games\Testbed\testbed_native_Data\app.info",
            "TestCo\nTestbed\n",
        );
        let saves = t.dir(r"LocalLow\TestCo\Testbed");
        let l = locate(&exe, &t.roots);
        assert_eq!(l.name, "Testbed");
        assert_eq!(paths(&l), vec![saves.to_string_lossy().into_owned()]);
        assert_eq!(l.folders[0].reason, "Unity save folder (TestCo / Testbed)");
        assert!(!l.cloud.synced);
    }

    #[test]
    fn unreal_game() {
        let t = Fake::new("loc-unreal");
        let exe = t.file(
            r"Games\Space Thing\Spacey\Binaries\Win64\Spacey-Win64-Shipping.exe",
            "",
        );
        let local = t.dir(r"Local\Spacey\Saved\SaveGames");
        t.dir(r"Local\Spacey\Saved\Logs");
        let l = locate(&exe, &t.roots);
        assert_eq!(l.name, "Spacey");
        // The specific SaveGames folder wins over the "Spacey" folder in Local.
        assert_eq!(paths(&l), vec![local.to_string_lossy().into_owned()]);
    }

    #[test]
    fn named_folders_and_steam_cloud() {
        let t = Fake::new("loc-named");
        let lib = r"Steam\steamapps";
        let exe = t.file(&format!(r"{lib}\common\Hollow Thing\hollow.exe"), "");
        t.file(
            &format!(r"{lib}\appmanifest_99.acf"),
            "\"appid\" \"99\"\n\"name\" \"Hollow Thing\"\n\"installdir\" \"Hollow Thing\"\n",
        );
        let my_games = t.dir(r"Documents\My Games\Hollow Thing");
        let low = t.dir(r"LocalLow\Team Cherry\Hollow_Thing");
        let remote = t.dir(r"Steam\userdata\555\99\remote");
        t.dir(r"Documents\Unrelated");
        t.dir(r"Roaming\hol"); // too short to match anything
        let l = locate(&exe, &t.roots);
        assert_eq!(l.name, "Hollow Thing");
        assert_eq!(l.steam.as_ref().map(|s| s.app_id), Some(99));
        assert!(l.cloud.synced);
        let p = paths(&l);
        assert_eq!(
            p,
            vec![
                remote.to_string_lossy().into_owned(),
                my_games.to_string_lossy().into_owned(),
                low.to_string_lossy().into_owned(),
            ]
        );
        assert!(l.folders[0].steam_cloud);
        assert!(!l.folders[1].steam_cloud);
    }

    #[test]
    fn never_picks_the_install_folder() {
        let t = Fake::new("loc-install");
        // Installed inside Documents: the install folder itself matches the name.
        let exe = t.file(r"Documents\Retro Quest\retroquest.exe", "");
        let l = locate(&exe, &t.roots);
        assert!(l.folders.is_empty(), "{:?}", l.folders);
        assert_eq!(l.name, "Retro Quest");
    }

    #[test]
    fn names() {
        assert_eq!(norm("My Game: Deluxe!"), "mygamedeluxe");
        assert_eq!(exe_stem(Path::new(r"C:\x\Foo-Win64-Shipping.exe")), "Foo");
        assert!(is_generic("Win64"));
    }
}
