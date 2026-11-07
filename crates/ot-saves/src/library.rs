//! The games whose saves OpenTrainer backs up, kept in `games.json` next to
//! the backups.

use std::path::{Path, PathBuf};

use ot_core::hash::sha256_bytes;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::folders::Roots;
use crate::locate::{CloudInfo, SaveFolder, locate, norm};
use crate::steam::SteamApp;
use crate::store::{Store, now_ms, write_atomic};

const LIBRARY_VERSION: u32 = 1;

/// A game with save backups.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: String,
    pub name: String,
    pub exe: String,
    pub steam: Option<SteamApp>,
    pub cloud: CloudInfo,
    pub folders: Vec<SaveFolder>,
    pub added_ms: u64,
}

impl Game {
    pub fn folder_paths(&self) -> Vec<String> {
        self.folders.iter().map(|f| f.path.clone()).collect()
    }

    /// Steam may sync this game's saves.
    pub fn cloud_synced(&self) -> bool {
        self.cloud.synced || self.folders.iter().any(|f| f.steam_cloud)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryFile {
    version: u32,
    games: Vec<Game>,
}

/// The game list plus where backups are stored.
#[derive(Debug)]
pub struct Library {
    root: PathBuf,
    games: Vec<Game>,
}

/// `%LOCALAPPDATA%\OpenTrainer\saves`.
pub fn default_root() -> Option<PathBuf> {
    Roots::current()
        .local
        .map(|l| l.join("OpenTrainer").join("saves"))
}

/// Stable id for an exe path: readable name plus a short hash of the path.
pub fn game_id(exe: &Path) -> String {
    let stem = exe
        .file_stem()
        .map(|s| norm(&s.to_string_lossy()))
        .unwrap_or_default();
    let stem: String = stem
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(24)
        .collect();
    let stem = if stem.is_empty() { "game".into() } else { stem };
    let hash = sha256_bytes(exe.to_string_lossy().to_lowercase().as_bytes());
    format!("{stem}-{}", &hash[..8])
}

/// Removes `.` and `..` from a path without touching the disk.
fn clean(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

fn same_path(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

fn overlaps(a: &str, b: &str) -> bool {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    a == b || a.starts_with(&format!("{b}\\")) || b.starts_with(&format!("{a}\\"))
}

impl Library {
    /// Opens (or starts) the library in `root`.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let path = root.join("games.json");
        let games = match std::fs::read(&path) {
            Ok(bytes) => {
                let file: LibraryFile =
                    serde_json::from_slice(&bytes).map_err(|e| Error::BadData {
                        path: path.clone(),
                        why: e.to_string(),
                    })?;
                if file.version != LIBRARY_VERSION {
                    return Err(Error::BadData {
                        path,
                        why: format!("unsupported version {}", file.version),
                    });
                }
                file.games
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(Error::io("can't read", path, e)),
        };
        Ok(Self { root, games })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn games(&self) -> &[Game] {
        &self.games
    }

    pub fn get(&self, id: &str) -> Result<&Game> {
        self.games
            .iter()
            .find(|g| g.id == id)
            .ok_or_else(|| Error::UnknownGame(id.to_string()))
    }

    pub fn find_exe(&self, exe: &str) -> Option<&Game> {
        self.games.iter().find(|g| same_path(&g.exe, exe))
    }

    /// The backups of a game.
    pub fn store(&self, id: &str) -> Result<Store> {
        let game = self.get(id)?;
        Ok(Store::new(self.root.join("games").join(&game.id)))
    }

    fn save(&self) -> Result<()> {
        std::fs::create_dir_all(&self.root)
            .map_err(|e| Error::io("can't create folder", &self.root, e))?;
        let file = LibraryFile {
            version: LIBRARY_VERSION,
            games: self.games.clone(),
        };
        let json = serde_json::to_vec_pretty(&file).map_err(|e| Error::BadData {
            path: self.root.clone(),
            why: e.to_string(),
        })?;
        write_atomic(&self.root.join("games.json"), &json)
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut Game> {
        self.games
            .iter_mut()
            .find(|g| g.id == id)
            .ok_or_else(|| Error::UnknownGame(id.to_string()))
    }

    /// Adds a game by its exe and finds its save folders. Returns the existing
    /// entry if the exe is already in the library.
    pub fn add(&mut self, exe: &Path, roots: &Roots) -> Result<Game> {
        // `\\?\C:\...` → `C:\...`, the form running processes report.
        let raw = exe.to_string_lossy().into_owned();
        let exe_str = raw.strip_prefix(r"\\?\").unwrap_or(&raw).to_string();
        let exe = Path::new(&exe_str);
        if let Some(g) = self.find_exe(&exe_str) {
            return Ok(g.clone());
        }
        if !exe.is_file() {
            return Err(Error::BadData {
                path: exe.to_path_buf(),
                why: "not a file".into(),
            });
        }
        let found = locate(exe, roots);
        let game = Game {
            id: game_id(exe),
            name: found.name,
            exe: exe_str,
            steam: found.steam,
            cloud: found.cloud,
            folders: found
                .folders
                .into_iter()
                .filter(|f| !self.inside_root(&f.path))
                .collect(),
            added_ms: now_ms(),
        };
        self.games.push(game.clone());
        self.save()?;
        Ok(game)
    }

    fn inside_root(&self, path: &str) -> bool {
        overlaps(path, &self.root.to_string_lossy())
    }

    /// Looks for save folders again. Folders the player added are kept.
    pub fn relocate(&mut self, id: &str, roots: &Roots) -> Result<Game> {
        let exe = PathBuf::from(&self.get(id)?.exe);
        let found = locate(&exe, roots);
        let root = self.root.clone();
        let game = self.get_mut(id)?;
        let mut folders: Vec<SaveFolder> = game
            .folders
            .iter()
            .filter(|f| f.user_added)
            .cloned()
            .collect();
        for f in found.folders {
            let clash = folders.iter().any(|o| overlaps(&o.path, &f.path));
            if !clash && !overlaps(&f.path, &root.to_string_lossy()) {
                folders.push(f);
            }
        }
        game.folders = folders;
        game.steam = found.steam;
        game.cloud = found.cloud;
        let game = game.clone();
        self.save()?;
        Ok(game)
    }

    /// Adds a save folder chosen by the player.
    pub fn add_folder(&mut self, id: &str, path: &Path) -> Result<Game> {
        let bad = |why: &str| Error::BadFolder {
            path: path.to_path_buf(),
            why: why.to_string(),
        };
        if !path.is_absolute() {
            return Err(bad(
                "use a full path, like C:\\Users\\you\\Documents\\My Games\\Game",
            ));
        }
        if !path.is_dir() {
            return Err(Error::NotAFolder(path.to_path_buf()));
        }
        if path.parent().is_none() {
            return Err(bad("a whole drive is too much"));
        }
        let p = clean(path)
            .to_string_lossy()
            .trim_end_matches('\\')
            .to_string();
        if self.inside_root(&p) {
            return Err(bad("that is where OpenTrainer keeps its backups"));
        }
        let game = self.get(id)?;
        if let Some(other) = game.folders.iter().find(|f| overlaps(&f.path, &p)) {
            return Err(bad(&format!(
                "it overlaps {}, which is already listed",
                other.path
            )));
        }
        if game
            .exe
            .to_lowercase()
            .starts_with(&format!("{}\\", p.to_lowercase()))
        {
            return Err(bad(
                "that is the game's install folder; pick the folder with the saves",
            ));
        }
        let game = self.get_mut(id)?;
        game.folders.push(SaveFolder {
            path: p,
            reason: "Added by you".into(),
            user_added: true,
            steam_cloud: false,
        });
        let game = game.clone();
        self.save()?;
        Ok(game)
    }

    pub fn remove_folder(&mut self, id: &str, path: &str) -> Result<Game> {
        let game = self.get_mut(id)?;
        let before = game.folders.len();
        game.folders.retain(|f| !same_path(&f.path, path));
        if game.folders.len() == before {
            return Err(Error::BadFolder {
                path: path.into(),
                why: "it isn't listed".into(),
            });
        }
        let game = game.clone();
        self.save()?;
        Ok(game)
    }

    /// Removes a game from the list; its backups are deleted only if asked.
    pub fn remove(&mut self, id: &str, delete_backups: bool) -> Result<()> {
        let store = self.store(id)?;
        self.games.retain(|g| g.id != id);
        self.save()?;
        if delete_backups && store.dir().is_dir() {
            let _guard = crate::store::lock();
            std::fs::remove_dir_all(store.dir())
                .map_err(|e| Error::io("can't delete", store.dir(), e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_games_and_folders_and_persists() {
        let base = crate::test_dir("library");
        let roots = Roots {
            local_low: Some(base.join("LocalLow")),
            ..Roots::default()
        };
        let exe = base.join("Game").join("testbed_native.exe");
        std::fs::create_dir_all(base.join("Game").join("testbed_native_Data")).unwrap();
        std::fs::write(&exe, b"").unwrap();
        std::fs::write(
            base.join("Game")
                .join("testbed_native_Data")
                .join("app.info"),
            "TestCo\nTestbed",
        )
        .unwrap();
        let saves = base.join("LocalLow").join("TestCo").join("Testbed");
        std::fs::create_dir_all(&saves).unwrap();
        let extra = base.join("Extra");
        std::fs::create_dir_all(&extra).unwrap();

        let mut lib = Library::open(base.join("store")).unwrap();
        let g = lib.add(&exe, &roots).unwrap();
        assert_eq!(g.name, "Testbed");
        assert!(g.id.starts_with("testbednative-"));
        assert_eq!(g.folders.len(), 1);
        assert_eq!(lib.add(&exe, &roots).unwrap().id, g.id, "added once");

        let g = lib.add_folder(&g.id, &extra).unwrap();
        assert_eq!(g.folders.len(), 2);
        assert!(g.folders[1].user_added);
        assert!(matches!(
            lib.add_folder(&g.id, &extra.join("..").join("Extra")),
            Err(Error::BadFolder { .. })
        ));
        assert!(matches!(
            lib.add_folder(&g.id, Path::new("relative")),
            Err(Error::BadFolder { .. })
        ));
        assert!(matches!(
            lib.add_folder(&g.id, &base.join("Game")),
            Err(Error::BadFolder { .. })
        ));
        assert!(matches!(
            lib.add_folder(&g.id, &base.join("missing")),
            Err(Error::NotAFolder(_))
        ));
        assert!(matches!(
            lib.add_folder(&g.id, &base.join("store")),
            Err(Error::BadFolder { .. }) | Err(Error::NotAFolder(_))
        ));

        // Relocating keeps the folder the player added.
        let g = lib.relocate(&g.id, &roots).unwrap();
        assert_eq!(g.folders.len(), 2);

        let reopened = Library::open(base.join("store")).unwrap();
        assert_eq!(reopened.games(), lib.games());

        let g = lib.remove_folder(&g.id, &extra.to_string_lossy()).unwrap();
        assert_eq!(g.folders.len(), 1);
        lib.remove(&g.id, true).unwrap();
        assert!(lib.games().is_empty());
        assert!(matches!(lib.get(&g.id), Err(Error::UnknownGame(_))));
        std::fs::remove_dir_all(&base).unwrap();
    }
}
