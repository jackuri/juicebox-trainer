//! Viewing and editing a game's saves: files in its save folders and
//! registry saves (PlayerPrefs).
//!
//! Saving an edit is guarded like a restore: the anti-cheat guard checks the
//! game's folder (no edits for online games), the game must be closed, and
//! every save folder is backed up first ("Before an edit"). Only files inside
//! the game's listed save folders can be opened or changed.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::codec::{self, Edit, Field};
use crate::error::{Error, Result};
use crate::library::Game;
use crate::registry;
use crate::restore::{CLOUD_WARNING, is_running};
use crate::store::{self, Limits, Reason, Store, safe_rel, scan, write_atomic};

/// Hex reads return at most this many bytes.
pub const MAX_HEX_READ: u64 = 64 * 1024;
/// Files bigger than this aren't opened in the editor.
pub const MAX_EDIT_BYTES: u64 = 64 << 20;

/// A file (or registry key) that can be opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveFile {
    pub folder: String,
    /// Path inside the folder (`/` separators); empty for a registry key.
    pub path: String,
    pub size: u64,
    pub modified_ms: Option<u64>,
    pub registry: bool,
}

/// What the editor shows for one file.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileView {
    pub folder: String,
    pub path: String,
    /// Wrappers, outermost first: "gzip-compressed", "CRC32 checksum at the end".
    pub layers: Vec<String>,
    /// "JSON", "Unreal save (GVAS)", "Unity PlayerPrefs", "Binary (hex)".
    pub format: String,
    /// The name save codes use: json, ini, gvas, playerprefs, hex.
    pub codec: String,
    pub fields: Vec<Field>,
    pub truncated: bool,
    /// Size of the unwrapped contents (what hex offsets refer to).
    pub body_len: u64,
    pub note: Option<String>,
    pub registry: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditReport {
    /// The backup made just before the edit (empty if nothing changed).
    pub backup: String,
    pub changed: bool,
    pub cloud_warning: Option<String>,
}

enum Target {
    File(PathBuf),
    Registry(String),
}

/// Lists the files in the game's save folders.
pub fn files(game: &Game) -> Result<Vec<SaveFile>> {
    let folders = game.folder_paths();
    let mut out = Vec::new();
    let mut registry_sizes = vec![None::<u64>; folders.len()];
    for s in scan(&folders, Limits::default())? {
        if let Some(d) = &s.data {
            *registry_sizes[s.folder].get_or_insert(0) += d.len() as u64;
            continue;
        }
        out.push(SaveFile {
            folder: folders[s.folder].clone(),
            path: s.rel,
            size: std::fs::metadata(&s.abs).map_or(0, |m| m.len()),
            modified_ms: s.modified_ms,
            registry: false,
        });
    }
    for (i, size) in registry_sizes.into_iter().enumerate() {
        if let Some(size) = size {
            out.push(SaveFile {
                folder: folders[i].clone(),
                path: String::new(),
                size,
                modified_ms: None,
                registry: true,
            });
        }
    }
    Ok(out)
}

/// Checks that `folder`/`path` is one of the game's saves.
fn target(game: &Game, folder: &str, path: &str) -> Result<Target> {
    let listed = game
        .folders
        .iter()
        .find(|f| f.path.to_lowercase() == folder.to_lowercase())
        .ok_or_else(|| Error::BadFolder {
            path: folder.into(),
            why: "it isn't one of this game's save folders".into(),
        })?;
    if registry::is_registry(&listed.path) {
        if !path.is_empty() {
            return Err(Error::Edit("a registry save is opened as a whole".into()));
        }
        return Ok(Target::Registry(listed.path.clone()));
    }
    let rel = safe_rel(path).ok_or_else(|| Error::Edit(format!("bad file path {path:?}")))?;
    let full = Path::new(&listed.path).join(rel);
    let meta = std::fs::symlink_metadata(&full).map_err(|e| Error::io("can't open", &full, e))?;
    if !meta.is_file() {
        return Err(Error::Edit(format!("{path} is not a file")));
    }
    if meta.len() > MAX_EDIT_BYTES {
        return Err(Error::Edit(format!(
            "{path} is too big to edit ({} MB; the limit is {} MB)",
            meta.len() >> 20,
            MAX_EDIT_BYTES >> 20
        )));
    }
    Ok(Target::File(full))
}

fn read(full: &Path) -> Result<Vec<u8>> {
    std::fs::read(full).map_err(|e| Error::io("can't read", full, e))
}

/// A SQLite save with changes still in its `-wal` file (the game didn't
/// finish writing them into the database). Editing it would lose them.
fn unsaved_wal(full: &Path, bytes: &[u8]) -> Option<String> {
    if !crate::codec::sqlite::is_sqlite(bytes) {
        return None;
    }
    let mut wal = full.as_os_str().to_owned();
    wal.push("-wal");
    let size = std::fs::metadata(PathBuf::from(&wal)).map_or(0, |m| m.len());
    (size > 0).then(|| {
        let name = full
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!(
            "The game hasn't finished writing this database: {name}-wal still holds changes. \
             Start the game and close it normally, then try again."
        )
    })
}

/// Opens a save for viewing.
pub fn open(game: &Game, folder: &str, path: &str) -> Result<FileView> {
    match target(game, folder, path)? {
        Target::Registry(key) => {
            let values =
                registry::read_values(&key).map_err(|e| Error::io("can't read", &key, e))?;
            Ok(FileView {
                folder: key,
                path: String::new(),
                layers: Vec::new(),
                format: "Unity PlayerPrefs (registry)".into(),
                codec: "playerprefs".into(),
                fields: registry::fields(&values),
                truncated: false,
                body_len: 0,
                note: None,
                registry: true,
            })
        }
        Target::File(full) => {
            let bytes = read(&full)?;
            let o = codec::open(&bytes);
            let note = unsaved_wal(&full, &bytes).or(o.note);
            Ok(FileView {
                folder: folder.to_string(),
                path: path.to_string(),
                layers: o.layers.iter().map(codec::Layer::label).collect(),
                format: o.body.label().into(),
                codec: o.body.codec_name().into(),
                fields: o.fields,
                truncated: o.truncated,
                body_len: o.body_len,
                note,
                registry: false,
            })
        }
    }
}

/// Bytes of the unwrapped contents, for the hex view.
pub fn hex(game: &Game, folder: &str, path: &str, offset: u64, len: u64) -> Result<Vec<u8>> {
    let Target::File(full) = target(game, folder, path)? else {
        return Err(Error::Edit("registry saves have no hex view".into()));
    };
    let o = codec::open(&read(&full)?);
    let start = offset.min(o.body_len) as usize;
    let end = (offset.saturating_add(len.min(MAX_HEX_READ))).min(o.body_len) as usize;
    Ok(o.contents[start..end].to_vec())
}

fn guard(game: &Game) -> Result<()> {
    let verdict = ot_core::guard::check_install(Path::new(&game.exe));
    if !verdict.allowed {
        return Err(Error::Blocked(Box::new(verdict)));
    }
    if is_running(&game.exe)? {
        return Err(Error::GameRunning(game.name.clone()));
    }
    Ok(())
}

/// Saves `edits` to one save, after the guard and running checks and a
/// backup of all the game's save folders.
pub fn save(
    game: &Game,
    store: &Store,
    folder: &str,
    path: &str,
    edits: &[Edit],
) -> Result<EditReport> {
    guard(game)?;
    save_unchecked(game, store, folder, path, edits)
}

/// [`save`] without the guard and running checks; for tests of the file
/// handling only.
pub(crate) fn save_unchecked(
    game: &Game,
    store: &Store,
    folder: &str,
    path: &str,
    edits: &[Edit],
) -> Result<EditReport> {
    let _lock = store::lock();
    let what = if path.is_empty() { folder } else { path };
    // Work out the new contents first, so a bad edit changes nothing.
    let (write, changed): (Box<dyn FnOnce() -> Result<()>>, bool) =
        match target(game, folder, path)? {
            Target::File(full) => {
                let old = read(&full)?;
                if let Some(why) = unsaved_wal(&full, &old) {
                    return Err(Error::Edit(why));
                }
                let new = codec::apply(&old, edits).map_err(Error::Edit)?;
                let changed = new != old;
                (Box::new(move || write_atomic(&full, &new)), changed)
            }
            Target::Registry(key) => {
                let values =
                    registry::read_values(&key).map_err(|e| Error::io("can't read", &key, e))?;
                let mut updated = values.clone();
                for e in edits {
                    let Edit::Set { path, value } = e else {
                        return Err(Error::Edit("registry saves are edited by value".into()));
                    };
                    let new = registry::edit(&updated, path, value).map_err(Error::Edit)?;
                    if let Some(v) = updated.iter_mut().find(|v| v.name == new.name) {
                        *v = new;
                    }
                }
                let changes: Vec<_> = updated
                    .into_iter()
                    .filter(|u| !values.contains(u))
                    .collect();
                let changed = !changes.is_empty();
                (
                    Box::new(move || {
                        for v in &changes {
                            registry::set_value(&key, v)
                                .map_err(|e| Error::io("can't write", &key, e))?;
                        }
                        Ok(())
                    }),
                    changed,
                )
            }
        };
    if !changed {
        return Ok(EditReport {
            backup: String::new(),
            changed: false,
            cloud_warning: None,
        });
    }
    let backup = store.create_locked(
        &game.folder_paths(),
        Reason::BeforeEdit,
        &format!("before editing {what}"),
        Limits::default(),
    )?;
    write()?;
    tracing::info!(game = %game.name, %what, "save edited");
    Ok(EditReport {
        backup: backup.snapshot.id,
        changed: true,
        cloud_warning: game.cloud_synced().then(|| CLOUD_WARNING.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::layers::{self, Layer};
    use crate::locate::{CloudInfo, SaveFolder};

    fn game(folders: &[String]) -> Game {
        Game {
            id: "g".into(),
            name: "Game".into(),
            exe: r"C:\nowhere\game.exe".into(),
            steam: None,
            cloud: CloudInfo::default(),
            folders: folders
                .iter()
                .map(|p| SaveFolder {
                    path: p.clone(),
                    reason: String::new(),
                    user_added: true,
                    steam_cloud: false,
                })
                .collect(),
            added_ms: 0,
        }
    }

    #[test]
    fn opens_and_edits_files_with_a_backup_first() {
        let base = crate::test_dir("editor");
        let saves = base.join("saves");
        std::fs::create_dir_all(saves.join("slots")).unwrap();
        let json = b"{\"player\": {\"gold\": 250}}".to_vec();
        let wrapped = layers::wrap(&[Layer::Gzip], json).unwrap();
        std::fs::write(saves.join("slots").join("slot1.sav"), &wrapped).unwrap();
        std::fs::write(saves.join("data.bin"), [0u8; 16]).unwrap();
        let folder = saves.to_string_lossy().into_owned();
        let g = game(std::slice::from_ref(&folder));
        let store = Store::new(base.join("store"));

        let list = files(&g).unwrap();
        assert_eq!(list.len(), 2);
        let v = open(&g, &folder, "slots/slot1.sav").unwrap();
        assert_eq!(v.format, "JSON");
        assert_eq!(v.codec, "json");
        assert_eq!(v.layers, vec!["gzip-compressed".to_string()]);
        assert_eq!(v.fields[0].value, "250");

        let edit = Edit::Set {
            path: "player.gold".into(),
            value: "1000".into(),
        };
        let r = save_unchecked(
            &g,
            &store,
            &folder,
            "slots/slot1.sav",
            std::slice::from_ref(&edit),
        )
        .unwrap();
        assert!(r.changed);
        let backup = store.manifest(&r.backup).unwrap();
        assert_eq!(backup.reason, Reason::BeforeEdit);
        assert_eq!(
            open(&g, &folder, "slots/slot1.sav").unwrap().fields[0].value,
            "1000"
        );
        // The same edit again changes nothing and makes no backup.
        let again = save_unchecked(&g, &store, &folder, "slots/slot1.sav", &[edit]).unwrap();
        assert!(!again.changed);
        assert_eq!(store.list().unwrap().len(), 1);

        // Hex view and byte edits for unknown files.
        assert_eq!(open(&g, &folder, "data.bin").unwrap().codec, "hex");
        let patch = Edit::Patch {
            offset: 4,
            hex: "FF".into(),
        };
        save_unchecked(&g, &store, &folder, "data.bin", &[patch]).unwrap();
        assert_eq!(
            hex(&g, &folder, "data.bin", 3, 3).unwrap(),
            vec![0, 0xFF, 0]
        );

        // A bad edit changes nothing.
        let bad = Edit::Set {
            path: "player.gold".into(),
            value: "lots".into(),
        };
        assert!(matches!(
            save_unchecked(&g, &store, &folder, "slots/slot1.sav", &[bad]),
            Err(Error::Edit(_))
        ));
        assert_eq!(store.list().unwrap().len(), 2);

        // Only files in the game's save folders.
        assert!(open(&g, &folder, "../outside.txt").is_err());
        assert!(open(&g, &base.to_string_lossy(), "saves/data.bin").is_err());
        assert!(open(&g, &folder, "missing.sav").is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn sqlite_saves_with_unsaved_changes_are_not_edited() {
        let base = crate::test_dir("editor-sqlite");
        let saves = base.join("saves");
        std::fs::create_dir_all(&saves).unwrap();
        std::fs::write(
            saves.join("game.db"),
            crate::codec::sqlite::tests::sample(true),
        )
        .unwrap();
        let folder = saves.to_string_lossy().into_owned();
        let g = game(std::slice::from_ref(&folder));
        let store = Store::new(base.join("store"));
        let edit = Edit::Set {
            path: "Player[1].Gold".into(),
            value: "1000".into(),
        };
        let v = open(&g, &folder, "game.db").unwrap();
        assert_eq!(v.codec, "sqlite");
        assert!(v.note.is_none());
        assert!(
            save_unchecked(&g, &store, &folder, "game.db", std::slice::from_ref(&edit))
                .unwrap()
                .changed
        );
        std::fs::write(saves.join("game.db-wal"), b"pending").unwrap();
        assert!(
            open(&g, &folder, "game.db")
                .unwrap()
                .note
                .unwrap()
                .contains("-wal")
        );
        assert!(matches!(
            save_unchecked(&g, &store, &folder, "game.db", &[edit]),
            Err(Error::Edit(_))
        ));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn edits_player_prefs() {
        let base = crate::test_dir("editor-reg");
        let key = registry::TestKey::new("editor");
        key.unity_sample();
        let g = game(std::slice::from_ref(&key.0));
        let store = Store::new(base.join("store"));
        let list = files(&g).unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].registry);
        let v = open(&g, &key.0, "").unwrap();
        assert_eq!(v.codec, "playerprefs");
        let edit = Edit::Set {
            path: "gold".into(),
            value: "777".into(),
        };
        let r = save_unchecked(&g, &store, &key.0, "", &[edit]).unwrap();
        assert!(r.changed);
        let v = open(&g, &key.0, "").unwrap();
        assert!(
            v.fields
                .iter()
                .any(|f| f.path == "gold" && f.value == "777")
        );
        // The backup holds the old value; restoring it puts 250 back.
        crate::restore::restore_unchecked(&g, &store, &r.backup, Default::default()).unwrap();
        let v = open(&g, &key.0, "").unwrap();
        assert!(
            v.fields
                .iter()
                .any(|f| f.path == "gold" && f.value == "250")
        );
        assert!(hex(&g, &key.0, "", 0, 4).is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn saving_needs_the_guard_and_a_closed_game() {
        let base = crate::test_dir("editor-guard");
        let saves = base.join("saves");
        std::fs::create_dir_all(&saves).unwrap();
        std::fs::write(saves.join("a.json"), "{\"a\": 1}").unwrap();
        let folder = saves.to_string_lossy().into_owned();
        let mut g = game(std::slice::from_ref(&folder));
        g.exe = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let store = Store::new(base.join("store"));
        let edit = Edit::Set {
            path: "a".into(),
            value: "2".into(),
        };
        let r = save(&g, &store, &folder, "a.json", &[edit]);
        assert!(
            matches!(r, Err(Error::GameRunning(_)) | Err(Error::Blocked(_))),
            "{r:?}"
        );
        assert_eq!(
            std::fs::read_to_string(saves.join("a.json")).unwrap(),
            "{\"a\": 1}"
        );
        std::fs::remove_dir_all(&base).unwrap();
    }
}
