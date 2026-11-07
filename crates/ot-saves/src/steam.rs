//! Steam: the install folder, a game's app id and its Steam Cloud folders.
//!
//! Steam's `.acf` / `.vdf` files are read as text and scanned for
//! `"key" "value"` pairs. Nothing else is interpreted.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A Steam app a game exe belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamApp {
    pub app_id: u32,
    pub name: Option<String>,
}

/// Steam's install folder, from the registry (`HKCU\Software\Valve\Steam`).
pub fn steam_dir() -> Option<PathBuf> {
    crate::folders::registry_string(r"Software\Valve\Steam", "SteamPath")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .or_else(|| {
            let default = PathBuf::from(r"C:\Program Files (x86)\Steam");
            default.is_dir().then_some(default)
        })
}

/// `"key" "value"` pairs, one per line, in file order. Nesting is ignored.
pub(crate) fn kv_pairs(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut tokens = Vec::new();
        let mut rest = line.trim();
        while let Some(start) = rest.find('"') {
            let after = &rest[start + 1..];
            let Some(end) = after.find('"') else { break };
            tokens.push(after[..end].to_string());
            rest = &after[end + 1..];
        }
        if tokens.len() == 2 {
            let value = tokens.pop().unwrap_or_default();
            let key = tokens.pop().unwrap_or_default();
            out.push((key, value));
        }
    }
    out
}

fn value<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

/// Finds the Steam app of a game exe: `steam_appid.txt` next to it (or a few
/// folders up), or the `appmanifest_*.acf` of the Steam library it is in.
pub fn app_for_exe(exe: &Path) -> Option<SteamApp> {
    let from_txt = exe.ancestors().skip(1).take(4).find_map(|dir| {
        let text = std::fs::read_to_string(dir.join("steam_appid.txt")).ok()?;
        text.trim().parse::<u32>().ok()
    });
    let from_acf = manifest_for_exe(exe);
    match (from_txt, from_acf) {
        (_, Some(app)) => Some(app),
        (Some(app_id), None) => Some(SteamApp { app_id, name: None }),
        (None, None) => None,
    }
}

/// `<library>\steamapps\common\<installdir>\...\game.exe` → the matching
/// `<library>\steamapps\appmanifest_<id>.acf`.
fn manifest_for_exe(exe: &Path) -> Option<SteamApp> {
    let mut install_dir = None;
    for dir in exe.ancestors().skip(1) {
        let Some(parent) = dir.parent() else { break };
        let parent_is_common = parent
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("common"));
        let grand_is_steamapps = parent
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|n| n.eq_ignore_ascii_case("steamapps"));
        if parent_is_common && grand_is_steamapps {
            install_dir = Some((
                dir.file_name()?.to_string_lossy().into_owned(),
                parent.parent()?,
            ));
            break;
        }
    }
    let (name, steamapps) = install_dir?;
    for entry in std::fs::read_dir(steamapps).ok()?.flatten() {
        let file = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if !(file.starts_with("appmanifest_") && file.ends_with(".acf")) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let pairs = kv_pairs(&text);
        if value(&pairs, "installdir").is_some_and(|d| d.eq_ignore_ascii_case(&name)) {
            let app_id = value(&pairs, "appid")?.parse().ok()?;
            return Some(SteamApp {
                app_id,
                name: value(&pairs, "name").map(str::to_string),
            });
        }
    }
    None
}

/// What Steam keeps for an app under `userdata`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CloudFolders {
    /// `userdata\<account>\<appid>\remote` folders that exist.
    pub remote: Vec<PathBuf>,
    /// Steam has cloud data for this app (a `remote` folder or `remotecache.vdf`),
    /// so it may sync saves even when they live elsewhere.
    pub synced: bool,
}

pub fn cloud_folders(steam: &Path, app_id: u32) -> CloudFolders {
    let mut out = CloudFolders::default();
    let Ok(accounts) = std::fs::read_dir(steam.join("userdata")) else {
        return out;
    };
    for account in accounts.flatten() {
        let numeric = account
            .file_name()
            .to_str()
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        if !numeric {
            continue;
        }
        let app = account.path().join(app_id.to_string());
        let remote = app.join("remote");
        if remote.is_dir() {
            out.remote.push(remote);
            out.synced = true;
        }
        if app.join("remotecache.vdf").is_file() {
            out.synced = true;
        }
    }
    out.remote.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_key_value_pairs() {
        let acf = "\"AppState\"\n{\n\t\"appid\"\t\t\"620\"\n\t\"name\"\t\t\"Portal 2\"\n\t\"installdir\"\t\t\"Portal 2\"\n}\n";
        let pairs = kv_pairs(acf);
        assert_eq!(value(&pairs, "appid"), Some("620"));
        assert_eq!(value(&pairs, "installdir"), Some("Portal 2"));
        assert_eq!(value(&pairs, "AppState"), None);
    }

    #[test]
    fn finds_app_from_library_manifest_and_appid_txt() {
        let base = crate::test_dir("steam-app");
        let lib = base.join("SteamLibrary").join("steamapps");
        let exe = lib
            .join("common")
            .join("Cool Game")
            .join("bin")
            .join("cool.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"").unwrap();
        std::fs::write(
            lib.join("appmanifest_4242.acf"),
            "\"AppState\"\n{\n\"appid\" \"4242\"\n\"name\" \"Cool Game\"\n\"installdir\" \"cool game\"\n}\n",
        )
        .unwrap();
        assert_eq!(
            app_for_exe(&exe),
            Some(SteamApp {
                app_id: 4242,
                name: Some("Cool Game".into())
            })
        );

        let other = base.join("Other").join("game.exe");
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        std::fs::write(&other, b"").unwrap();
        assert_eq!(app_for_exe(&other), None);
        std::fs::write(base.join("Other").join("steam_appid.txt"), "777\n").unwrap();
        assert_eq!(app_for_exe(&other).map(|a| a.app_id), Some(777));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn finds_cloud_folders() {
        let steam = crate::test_dir("steam-cloud");
        let remote = steam
            .join("userdata")
            .join("1234")
            .join("4242")
            .join("remote");
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::create_dir_all(steam.join("userdata").join("1234").join("99")).unwrap();
        std::fs::write(
            steam
                .join("userdata")
                .join("1234")
                .join("99")
                .join("remotecache.vdf"),
            "",
        )
        .unwrap();
        std::fs::create_dir_all(
            steam
                .join("userdata")
                .join("anonymous")
                .join("4242")
                .join("remote"),
        )
        .unwrap();
        let c = cloud_folders(&steam, 4242);
        assert_eq!(c.remote, vec![remote]);
        assert!(c.synced);
        let c = cloud_folders(&steam, 99);
        assert!(c.remote.is_empty() && c.synced);
        assert_eq!(cloud_folders(&steam, 5), CloudFolders::default());
        std::fs::remove_dir_all(&steam).unwrap();
    }
}
