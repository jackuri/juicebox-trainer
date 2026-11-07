//! Restoring a backup into the game's save folders.
//!
//! Before anything is written:
//! - the anti-cheat guard checks the game's install folder (restoring is how
//!   you'd roll back progress, so it is refused for online games);
//! - the game must be closed;
//! - every stored file is checked against its hash;
//! - the current saves are backed up ("before restore"), so a restore can be
//!   undone by restoring that backup.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ot_core::hash::sha256_file;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::library::Game;
use crate::store::{self, Limits, Reason, Store, key, safe_rel, scan, time_of, write_atomic};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RestoreOptions {
    /// Also delete files that weren't in the backup (they are still in the
    /// "before restore" backup).
    pub remove_extra: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub snapshot: String,
    /// The backup of the saves as they were just before restoring.
    pub safety_snapshot: String,
    pub written: usize,
    pub unchanged: usize,
    /// Files deleted because they weren't in the backup.
    pub removed: Vec<String>,
    /// Files that weren't in the backup and were left alone.
    pub extra: Vec<String>,
    /// Shown when Steam may sync this game's saves.
    pub cloud_warning: Option<String>,
}

pub const CLOUD_WARNING: &str = "Steam Cloud syncs this game's saves. If Steam says the cloud \
    copy is different when you start the game, choose the files on this PC (local), or Steam may \
    put the newer cloud save back.";

/// Is the game's exe running? Compares full paths, ignoring case.
pub fn is_running(exe: &str) -> Result<bool> {
    let list = ot_core::list_processes().map_err(|e| Error::Process(e.to_string()))?;
    Ok(list.iter().any(|p| {
        p.exe_path
            .as_deref()
            .is_some_and(|x| x.to_lowercase() == exe.to_lowercase())
    }))
}

struct Planned {
    target: PathBuf,
    sha: String,
    modified_ms: Option<u64>,
    rel: String,
}

/// Restores backup `id` of `game`.
pub fn restore(
    game: &Game,
    store: &Store,
    id: &str,
    options: RestoreOptions,
) -> Result<RestoreReport> {
    let verdict = ot_core::guard::check_install(Path::new(&game.exe));
    if !verdict.allowed {
        return Err(Error::Blocked(Box::new(verdict)));
    }
    if is_running(&game.exe)? {
        return Err(Error::GameRunning(game.name.clone()));
    }
    restore_unchecked(game, store, id, options)
}

/// [`restore`] without the guard and running checks; for tests of the file
/// handling only.
pub(crate) fn restore_unchecked(
    game: &Game,
    store: &Store,
    id: &str,
    options: RestoreOptions,
) -> Result<RestoreReport> {
    let _guard = store::lock();
    let manifest = store.manifest(id)?;
    let damaged = |why: String| Error::Damaged {
        id: id.to_string(),
        why,
    };

    // Every folder with files must still be one of the game's save folders.
    let current_folders = game.folder_paths();
    let mut used: Vec<usize> = manifest.files.iter().map(|f| f.folder).collect();
    used.sort_unstable();
    used.dedup();
    for &i in &used {
        let folder = &manifest.folders[i];
        if !current_folders
            .iter()
            .any(|c| c.to_lowercase() == folder.to_lowercase())
        {
            return Err(Error::FolderNotListed {
                id: id.to_string(),
                folder: folder.clone(),
            });
        }
    }

    let mut plan = Vec::with_capacity(manifest.files.len());
    for f in &manifest.files {
        let rel =
            safe_rel(&f.path).ok_or_else(|| damaged(format!("bad file path {:?}", f.path)))?;
        if !store.has_blob(&f.sha256) {
            return Err(damaged(format!("the stored copy of {} is missing", f.path)));
        }
        plan.push(Planned {
            target: Path::new(&manifest.folders[f.folder]).join(rel),
            sha: f.sha256.clone(),
            modified_ms: f.modified_ms,
            rel: f.path.clone(),
        });
    }
    // Check every stored file before writing any.
    let mut contents = BTreeMap::new();
    for p in &plan {
        if !contents.contains_key(&p.sha) {
            contents.insert(p.sha.clone(), store.read_blob(&p.sha)?);
        }
    }

    let restored_folders: Vec<String> = manifest.folders.clone();
    let safety = store.create_locked(
        &restored_folders,
        Reason::BeforeRestore,
        "",
        Limits::default(),
    )?;

    let mut written = 0;
    let mut unchanged = 0;
    for p in &plan {
        if p.target.is_file() && sha256_file(&p.target).is_ok_and(|s| s == p.sha) {
            unchanged += 1;
            continue;
        }
        if let Some(parent) = p.target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::io("can't create folder", parent, e))?;
        }
        write_atomic(&p.target, &contents[&p.sha])?;
        if let Some(ms) = p.modified_ms
            && let Ok(file) = std::fs::OpenOptions::new().write(true).open(&p.target)
        {
            let _ = file.set_modified(time_of(ms));
        }
        written += 1;
        tracing::info!("restored {}", p.rel);
    }

    let wanted: std::collections::BTreeSet<String> = manifest
        .files
        .iter()
        .map(|f| key(&manifest.folders[f.folder], &f.path))
        .collect();
    let mut removed = Vec::new();
    let mut extra = Vec::new();
    for s in scan(&restored_folders, Limits::default())? {
        if wanted.contains(&key(&restored_folders[s.folder], &s.rel)) {
            continue;
        }
        if options.remove_extra {
            std::fs::remove_file(&s.abs).map_err(|e| Error::io("can't delete", &s.abs, e))?;
            removed.push(s.rel);
        } else {
            extra.push(s.rel);
        }
    }

    Ok(RestoreReport {
        snapshot: id.to_string(),
        safety_snapshot: safety.snapshot.id,
        written,
        unchanged,
        removed,
        extra,
        cloud_warning: game.cloud_synced().then(|| CLOUD_WARNING.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locate::{CloudInfo, SaveFolder};

    fn game(folder: &Path, cloud: bool) -> Game {
        Game {
            id: "g".into(),
            name: "Game".into(),
            exe: r"C:\nowhere\game.exe".into(),
            steam: None,
            cloud: CloudInfo {
                synced: cloud,
                reason: None,
            },
            folders: vec![SaveFolder {
                path: folder.to_string_lossy().into_owned(),
                reason: String::new(),
                user_added: true,
                steam_cloud: false,
            }],
            added_ms: 0,
        }
    }

    #[test]
    fn restores_files_and_backs_up_first() {
        let base = crate::test_dir("restore");
        let saves = base.join("saves");
        std::fs::create_dir_all(saves.join("sub")).unwrap();
        std::fs::write(saves.join("save1.json"), "{\"gold\":100}").unwrap();
        std::fs::write(saves.join("sub").join("same.dat"), "same").unwrap();
        let store = Store::new(base.join("store"));
        let g = game(&saves, false);
        let old = store
            .create(&g.folder_paths(), Reason::Manual, "", Limits::default())
            .unwrap()
            .snapshot;

        std::fs::write(saves.join("save1.json"), "{\"gold\":9999}").unwrap();
        std::fs::write(saves.join("new.json"), "later").unwrap();
        let r = restore_unchecked(&g, &store, &old.id, RestoreOptions::default()).unwrap();
        assert_eq!(
            std::fs::read_to_string(saves.join("save1.json")).unwrap(),
            "{\"gold\":100}"
        );
        assert_eq!((r.written, r.unchanged), (1, 1));
        assert_eq!(r.extra, vec!["new.json".to_string()]);
        assert!(r.removed.is_empty());
        assert!(r.cloud_warning.is_none());
        // The saves from just before are backed up and can be restored back.
        let safety = store.manifest(&r.safety_snapshot).unwrap();
        assert_eq!(safety.reason, Reason::BeforeRestore);
        assert_eq!(safety.files.len(), 3);
        let back =
            restore_unchecked(&g, &store, &r.safety_snapshot, RestoreOptions::default()).unwrap();
        assert_eq!(back.written, 1);
        assert_eq!(
            std::fs::read_to_string(saves.join("save1.json")).unwrap(),
            "{\"gold\":9999}"
        );

        let r =
            restore_unchecked(&g, &store, &old.id, RestoreOptions { remove_extra: true }).unwrap();
        assert_eq!(r.removed, vec!["new.json".to_string()]);
        assert!(!saves.join("new.json").exists());

        // A deleted file comes back.
        std::fs::remove_file(saves.join("sub").join("same.dat")).unwrap();
        restore_unchecked(&g, &store, &old.id, RestoreOptions::default()).unwrap();
        assert_eq!(
            std::fs::read_to_string(saves.join("sub").join("same.dat")).unwrap(),
            "same"
        );
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn refuses_damaged_backups_and_unlisted_folders() {
        let base = crate::test_dir("restore-bad");
        let saves = base.join("saves");
        std::fs::create_dir_all(&saves).unwrap();
        std::fs::write(saves.join("a.sav"), "aaaa").unwrap();
        let store = Store::new(base.join("store"));
        let g = game(&saves, true);
        let snap = store
            .create(&g.folder_paths(), Reason::Manual, "", Limits::default())
            .unwrap()
            .snapshot;
        let m = store.manifest(&snap.id).unwrap();

        // Folder no longer listed for the game.
        let other = game(&base.join("elsewhere"), false);
        assert!(matches!(
            restore_unchecked(&other, &store, &snap.id, RestoreOptions::default()),
            Err(Error::FolderNotListed { .. })
        ));

        // A stored file was tampered with: nothing is written.
        let blob = store
            .dir()
            .join("blobs")
            .join(&m.files[0].sha256[..2])
            .join(&m.files[0].sha256);
        std::fs::write(&blob, "evil").unwrap();
        std::fs::write(saves.join("a.sav"), "current").unwrap();
        assert!(restore_unchecked(&g, &store, &snap.id, RestoreOptions::default()).is_err());
        assert_eq!(
            std::fs::read_to_string(saves.join("a.sav")).unwrap(),
            "current"
        );

        // A manifest with a path that leaves the folder.
        let mut evil = m.clone();
        evil.id = "1".into();
        evil.files[0].path = "../escape.txt".into();
        std::fs::write(
            store.dir().join("snapshots").join("1.json"),
            serde_json::to_vec(&evil).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            restore_unchecked(&g, &store, "1", RestoreOptions::default()),
            Err(Error::Damaged { .. })
        ));
        assert!(!base.join("escape.txt").exists());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn restore_runs_the_guard_and_needs_the_game_closed() {
        let base = crate::test_dir("restore-guard");
        let game_dir = base.join("Game");
        std::fs::create_dir_all(game_dir.join("EasyAntiCheat")).unwrap();
        std::fs::write(game_dir.join("game.exe"), b"").unwrap();
        let saves = base.join("saves");
        std::fs::create_dir_all(&saves).unwrap();
        let mut g = game(&saves, false);
        g.exe = game_dir.join("game.exe").to_string_lossy().into_owned();
        let store = Store::new(base.join("store"));
        let snap = store
            .create(&g.folder_paths(), Reason::Manual, "", Limits::default())
            .unwrap()
            .snapshot;
        assert!(matches!(
            restore(&g, &store, &snap.id, RestoreOptions::default()),
            Err(Error::Blocked(_))
        ));

        // This test process is "the game" and it is running.
        let me = std::env::current_exe().unwrap();
        g.exe = me.to_string_lossy().into_owned();
        assert!(is_running(&g.exe).unwrap());
        let r = restore(&g, &store, &snap.id, RestoreOptions::default());
        assert!(
            matches!(r, Err(Error::GameRunning(_)) | Err(Error::Blocked(_))),
            "{r:?}"
        );
        std::fs::remove_dir_all(&base).unwrap();
    }
}
