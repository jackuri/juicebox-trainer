//! Save backups ("snapshots").
//!
//! A backup is a manifest (`snapshots\<id>.json`) that lists every file in the
//! game's save folders with its size and SHA-256. File contents are stored once
//! per content in `blobs\<aa>\<sha256>`, so backing up unchanged saves again
//! costs no space. Automatic backups that match the newest one are not made
//! again, and only the newest [`KEEP_AUTOMATIC`] automatic backups are kept.
//! Manual backups are kept until deleted.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ot_core::hash::sha256_bytes;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::registry;

/// Automatic backups kept per game (manual ones don't count).
pub const KEEP_AUTOMATIC: usize = 30;

const MANIFEST_VERSION: u32 = 1;
pub(crate) const TMP_SUFFIX: &str = ".ot-tmp";

/// Serialises every change to any store in this process, so a cleanup can't
/// remove a blob that a backup running at the same time relies on.
static LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

/// Why a backup was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// The game started.
    Launch,
    /// The game closed.
    Exit,
    /// The player asked for it.
    Manual,
    /// Taken right before restoring another backup.
    BeforeRestore,
    /// Taken right before editing a save.
    BeforeEdit,
    /// Taken right before copying saves to another Steam account.
    BeforeAccountCopy,
}

impl Reason {
    pub fn automatic(self) -> bool {
        self != Self::Manual
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    /// Index into [`Manifest::folders`].
    pub folder: usize,
    /// Path inside the folder, with `/` separators.
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub modified_ms: Option<u64>,
}

/// A file that could not be read (e.g. the game had it locked).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub folder: usize,
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub version: u32,
    pub id: String,
    pub created_ms: u64,
    pub reason: Reason,
    pub note: String,
    pub folders: Vec<String>,
    pub files: Vec<FileEntry>,
    pub skipped: Vec<Skipped>,
    pub total_size: u64,
}

/// A backup as shown in a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub created_ms: u64,
    pub reason: Reason,
    pub note: String,
    pub files: usize,
    pub total_size: u64,
    pub skipped: Vec<Skipped>,
    pub folders: Vec<String>,
}

impl From<&Manifest> for Summary {
    fn from(m: &Manifest) -> Self {
        Self {
            id: m.id.clone(),
            created_ms: m.created_ms,
            reason: m.reason,
            note: m.note.clone(),
            files: m.files.len(),
            total_size: m.total_size,
            skipped: m.skipped.clone(),
            folders: m.folders.clone(),
        }
    }
}

/// Result of [`Store::create`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Created {
    pub snapshot: Summary,
    /// False when an automatic backup matched the newest one, which is
    /// returned instead.
    pub new: bool,
}

/// Refuses to back up more than this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_files: usize,
    pub max_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_files: 20_000,
            max_bytes: 2 << 30,
        }
    }
}

/// A file found in a save folder.
#[derive(Debug, Clone)]
pub(crate) struct Scanned {
    pub folder: usize,
    /// Path inside the folder, or the value name for a registry key.
    pub rel: String,
    pub abs: PathBuf,
    pub modified_ms: Option<u64>,
    /// A registry value's bytes (type + data); `None` for files.
    pub data: Option<Vec<u8>>,
}

impl Scanned {
    /// The bytes as stored in a backup.
    pub fn read(&self) -> std::io::Result<Vec<u8>> {
        match &self.data {
            Some(d) => Ok(d.clone()),
            None => std::fs::read(&self.abs),
        }
    }
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn ms_of(t: std::io::Result<SystemTime>) -> Option<u64> {
    t.ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

pub(crate) fn time_of(ms: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms)
}

/// Lowercased `folder|path`, the identity of a file across backups.
pub(crate) fn key(folder: &str, rel: &str) -> String {
    format!("{}|{}", folder.to_lowercase(), rel.to_lowercase())
}

/// Lists the files in `folders`. A folder that doesn't exist (yet) is empty.
/// Symbolic links and junctions are not followed.
pub(crate) fn scan(folders: &[String], limits: Limits) -> Result<Vec<Scanned>> {
    let mut out = Vec::new();
    let mut bytes = 0u64;
    for (index, folder) in folders.iter().enumerate() {
        if registry::is_registry(folder) {
            // A key that doesn't exist (yet) is empty, like a missing folder.
            for v in registry::read_values(folder).unwrap_or_default() {
                let data = v.to_bytes();
                bytes += data.len() as u64;
                out.push(Scanned {
                    folder: index,
                    rel: v.name,
                    abs: PathBuf::new(),
                    modified_ms: None,
                    data: Some(data),
                });
                if out.len() > limits.max_files || bytes > limits.max_bytes {
                    return Err(Error::TooLarge {
                        files: out.len(),
                        bytes,
                        max_files: limits.max_files,
                        max_bytes: limits.max_bytes,
                    });
                }
            }
            continue;
        }
        let root = PathBuf::from(folder);
        if !root.is_dir() {
            continue;
        }
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let entries =
                std::fs::read_dir(&dir).map_err(|e| Error::io("can't read folder", &dir, e))?;
            for entry in entries {
                let entry = entry.map_err(|e| Error::io("can't read folder", &dir, e))?;
                let path = entry.path();
                let Ok(meta) = std::fs::symlink_metadata(&path) else {
                    continue;
                };
                let ty = meta.file_type();
                if ty.is_symlink() {
                    continue;
                }
                if ty.is_dir() {
                    stack.push(path);
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.ends_with(TMP_SUFFIX) {
                    continue;
                }
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                bytes += meta.len();
                out.push(Scanned {
                    folder: index,
                    rel,
                    abs: path,
                    modified_ms: ms_of(meta.modified()),
                    data: None,
                });
                if out.len() > limits.max_files || bytes > limits.max_bytes {
                    return Err(Error::TooLarge {
                        files: out.len(),
                        bytes,
                        max_files: limits.max_files,
                        max_bytes: limits.max_bytes,
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| (a.folder, &a.rel).cmp(&(b.folder, &b.rel)));
    Ok(out)
}

/// A relative path from a manifest, checked so it can't leave its folder.
pub(crate) fn safe_rel(rel: &str) -> Option<PathBuf> {
    let p = PathBuf::from(rel.replace('/', "\\"));
    let ok = !rel.is_empty() && p.components().all(|c| matches!(c, Component::Normal(_)));
    ok.then_some(p)
}

/// Writes `bytes` to `path` via a temporary file and a rename.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(TMP_SUFFIX);
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes).map_err(|e| Error::io("can't write", &tmp, e))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        Error::io("can't replace", path, e)
    })
}

/// The backups of one game.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn manifests(&self) -> PathBuf {
        self.dir.join("snapshots")
    }

    fn manifest_path(&self, id: &str) -> Result<PathBuf> {
        let valid = !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_digit());
        if !valid {
            return Err(Error::UnknownSnapshot(id.to_string()));
        }
        Ok(self.manifests().join(format!("{id}.json")))
    }

    fn blob_path(&self, sha: &str) -> Result<PathBuf> {
        let valid = sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit());
        if !valid {
            return Err(Error::BadData {
                path: self.dir.clone(),
                why: format!("bad file hash {sha:?}"),
            });
        }
        Ok(self.dir.join("blobs").join(&sha[..2]).join(sha))
    }

    /// Every manifest, newest first. Unreadable ones are skipped (and logged).
    fn all(&self) -> Result<Vec<Manifest>> {
        let dir = self.manifests();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(Error::io("can't read backups in", dir, e)),
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            match read_manifest(&path) {
                Ok(m) => out.push(m),
                Err(e) => tracing::warn!("skipping backup {}: {e}", path.display()),
            }
        }
        out.sort_by(|a, b| (b.created_ms, &b.id).cmp(&(a.created_ms, &a.id)));
        Ok(out)
    }

    /// Backups, newest first.
    pub fn list(&self) -> Result<Vec<Summary>> {
        Ok(self.all()?.iter().map(Summary::from).collect())
    }

    pub fn manifest(&self, id: &str) -> Result<Manifest> {
        let path = self.manifest_path(id)?;
        if !path.is_file() {
            return Err(Error::UnknownSnapshot(id.to_string()));
        }
        let m = read_manifest(&path)?;
        if m.id != id {
            return Err(Error::Damaged {
                id: id.to_string(),
                why: "its id doesn't match its file name".into(),
            });
        }
        Ok(m)
    }

    /// The stored contents of a file, checked against its hash.
    pub fn read_blob(&self, sha: &str) -> Result<Vec<u8>> {
        let path = self.blob_path(sha)?;
        let bytes =
            std::fs::read(&path).map_err(|e| Error::io("can't read backup file", &path, e))?;
        if sha256_bytes(&bytes) != sha.to_ascii_lowercase() {
            return Err(Error::BadData {
                path,
                why: "the stored file doesn't match its hash".into(),
            });
        }
        Ok(bytes)
    }

    pub(crate) fn has_blob(&self, sha: &str) -> bool {
        self.blob_path(sha).is_ok_and(|p| p.is_file())
    }

    /// Backs up `folders` now.
    pub fn create(
        &self,
        folders: &[String],
        reason: Reason,
        note: &str,
        limits: Limits,
    ) -> Result<Created> {
        let _guard = lock();
        self.create_locked(folders, reason, note, limits)
    }

    pub(crate) fn create_locked(
        &self,
        folders: &[String],
        reason: Reason,
        note: &str,
        limits: Limits,
    ) -> Result<Created> {
        if folders.is_empty() {
            return Err(Error::NoFolders);
        }
        let scanned = scan(folders, limits)?;
        let mut files = Vec::with_capacity(scanned.len());
        let mut skipped = Vec::new();
        let mut total = 0;
        for s in scanned {
            let bytes = match s.read() {
                Ok(b) => b,
                Err(e) => {
                    skipped.push(Skipped {
                        folder: s.folder,
                        path: s.rel,
                        error: e.to_string(),
                    });
                    continue;
                }
            };
            let sha = sha256_bytes(&bytes);
            let blob = self.blob_path(&sha)?;
            if !blob.is_file() {
                let parent = blob.parent().unwrap_or(&self.dir);
                std::fs::create_dir_all(parent)
                    .map_err(|e| Error::io("can't create folder", parent, e))?;
                write_atomic(&blob, &bytes)?;
            }
            total += bytes.len() as u64;
            files.push(FileEntry {
                folder: s.folder,
                path: s.rel,
                size: bytes.len() as u64,
                sha256: sha,
                modified_ms: s.modified_ms,
            });
        }

        let existing = self.all()?;
        if reason.automatic()
            && skipped.is_empty()
            && let Some(latest) = existing.first()
            && same_content(latest, folders, &files)
        {
            return Ok(Created {
                snapshot: Summary::from(latest),
                new: false,
            });
        }

        let mut created = now_ms();
        let dir = self.manifests();
        std::fs::create_dir_all(&dir).map_err(|e| Error::io("can't create folder", &dir, e))?;
        while self.manifest_path(&created.to_string())?.exists() {
            created += 1;
        }
        let manifest = Manifest {
            version: MANIFEST_VERSION,
            id: created.to_string(),
            created_ms: created,
            reason,
            note: note.trim().to_string(),
            folders: folders.to_vec(),
            files,
            skipped,
            total_size: total,
        };
        let json = serde_json::to_vec_pretty(&manifest).map_err(|e| Error::BadData {
            path: dir.clone(),
            why: e.to_string(),
        })?;
        write_atomic(&self.manifest_path(&manifest.id)?, &json)?;
        if reason.automatic() {
            self.prune_locked(KEEP_AUTOMATIC)?;
        }
        Ok(Created {
            snapshot: Summary::from(&manifest),
            new: true,
        })
    }

    /// Deletes a backup and the stored files no other backup uses.
    pub fn delete(&self, id: &str) -> Result<()> {
        let _guard = lock();
        let path = self.manifest_path(id)?;
        if !path.is_file() {
            return Err(Error::UnknownSnapshot(id.to_string()));
        }
        std::fs::remove_file(&path).map_err(|e| Error::io("can't delete", &path, e))?;
        self.gc_locked()
    }

    /// Changes the note of a backup.
    pub fn set_note(&self, id: &str, note: &str) -> Result<Summary> {
        let _guard = lock();
        let mut m = self.manifest(id)?;
        m.note = note.trim().to_string();
        let json = serde_json::to_vec_pretty(&m).map_err(|e| Error::BadData {
            path: self.dir.clone(),
            why: e.to_string(),
        })?;
        write_atomic(&self.manifest_path(id)?, &json)?;
        Ok(Summary::from(&m))
    }

    /// Keeps the newest `keep` automatic backups, then cleans up.
    fn prune_locked(&self, keep: usize) -> Result<()> {
        let all = self.all()?;
        for old in all.iter().filter(|m| m.reason.automatic()).skip(keep) {
            let path = self.manifest_path(&old.id)?;
            std::fs::remove_file(&path).map_err(|e| Error::io("can't delete", &path, e))?;
        }
        self.gc_locked()
    }

    /// Deletes stored files that no backup lists, and leftover temp files.
    /// Does nothing if any backup can't be read, so nothing in use is lost.
    fn gc_locked(&self) -> Result<()> {
        let dir = self.manifests();
        let mut used = BTreeSet::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|e| e != "json") {
                    continue;
                }
                match read_manifest(&path) {
                    Ok(m) => used.extend(m.files.into_iter().map(|f| f.sha256)),
                    Err(_) => return Ok(()),
                }
            }
        }
        let Ok(prefixes) = std::fs::read_dir(self.dir.join("blobs")) else {
            return Ok(());
        };
        for prefix in prefixes.flatten() {
            let Ok(blobs) = std::fs::read_dir(prefix.path()) else {
                continue;
            };
            for blob in blobs.flatten() {
                let name = blob.file_name().to_string_lossy().into_owned();
                if !used.contains(&name) {
                    let _ = std::fs::remove_file(blob.path());
                }
            }
            let _ = std::fs::remove_dir(prefix.path());
        }
        Ok(())
    }
}

fn read_manifest(path: &Path) -> Result<Manifest> {
    let text = std::fs::read(path).map_err(|e| Error::io("can't read", path, e))?;
    let m: Manifest = serde_json::from_slice(&text).map_err(|e| Error::BadData {
        path: path.to_path_buf(),
        why: e.to_string(),
    })?;
    if m.version != MANIFEST_VERSION {
        return Err(Error::BadData {
            path: path.to_path_buf(),
            why: format!("unsupported backup version {}", m.version),
        });
    }
    if m.files.iter().any(|f| f.folder >= m.folders.len()) {
        return Err(Error::BadData {
            path: path.to_path_buf(),
            why: "a file refers to a missing folder".into(),
        });
    }
    Ok(m)
}

fn same_content(m: &Manifest, folders: &[String], files: &[FileEntry]) -> bool {
    let by_key = |folders: &[String], files: &[FileEntry]| -> BTreeMap<String, String> {
        files
            .iter()
            .map(|f| (key(&folders[f.folder], &f.path), f.sha256.clone()))
            .collect()
    };
    let lower = |v: &[String]| v.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>();
    m.skipped.is_empty()
        && lower(&m.folders) == lower(folders)
        && by_key(&m.folders, &m.files) == by_key(folders, files)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct T {
        base: PathBuf,
        saves: PathBuf,
        store: Store,
    }

    impl T {
        fn new(name: &str) -> Self {
            let base = crate::test_dir(name);
            let saves = base.join("saves");
            std::fs::create_dir_all(saves.join("slots")).unwrap();
            Self {
                store: Store::new(base.join("store")),
                saves,
                base,
            }
        }
        fn folders(&self) -> Vec<String> {
            vec![self.saves.to_string_lossy().into_owned()]
        }
        fn write(&self, rel: &str, text: &str) {
            std::fs::write(self.saves.join(rel), text).unwrap();
        }
        fn blobs(&self) -> usize {
            walk_count(&self.store.dir().join("blobs"))
        }
    }

    impl Drop for T {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    fn walk_count(dir: &Path) -> usize {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        entries
            .flatten()
            .map(|e| {
                if e.path().is_dir() {
                    walk_count(&e.path())
                } else {
                    1
                }
            })
            .sum()
    }

    #[test]
    fn backs_up_dedupes_and_skips_unchanged() {
        let t = T::new("store-basic");
        t.write("save1.json", "{\"gold\":100}");
        t.write("slots/slot2.dat", "abc");
        let c = t
            .store
            .create(&t.folders(), Reason::Launch, "", Limits::default())
            .unwrap();
        assert!(c.new);
        assert_eq!(c.snapshot.files, 2);
        assert_eq!(c.snapshot.total_size, 15);
        let m = t.store.manifest(&c.snapshot.id).unwrap();
        assert_eq!(m.files[0].path, "save1.json");
        assert_eq!(m.files[1].path, "slots/slot2.dat");
        assert_eq!(
            t.store.read_blob(&m.files[0].sha256).unwrap(),
            b"{\"gold\":100}"
        );

        // Nothing changed: an automatic backup isn't made again, a manual one is.
        let again = t
            .store
            .create(&t.folders(), Reason::Exit, "", Limits::default())
            .unwrap();
        assert!(!again.new);
        assert_eq!(again.snapshot.id, c.snapshot.id);
        let manual = t
            .store
            .create(
                &t.folders(),
                Reason::Manual,
                "  before boss ",
                Limits::default(),
            )
            .unwrap();
        assert!(manual.new);
        assert_eq!(manual.snapshot.note, "before boss");
        // Same contents are stored once.
        assert_eq!(t.blobs(), 2);

        t.write("save1.json", "{\"gold\":1100}");
        let changed = t
            .store
            .create(&t.folders(), Reason::Exit, "", Limits::default())
            .unwrap();
        assert!(changed.new);
        assert_eq!(t.blobs(), 3);
        let list = t.store.list().unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].id, changed.snapshot.id);

        t.store.delete(&changed.snapshot.id).unwrap();
        assert_eq!(t.blobs(), 2, "the file only that backup used is gone");
        assert!(matches!(
            t.store.delete(&changed.snapshot.id),
            Err(Error::UnknownSnapshot(_))
        ));
        let noted = t.store.set_note(&manual.snapshot.id, "x").unwrap();
        assert_eq!(noted.note, "x");
    }

    #[test]
    fn keeps_only_the_newest_automatic_backups() {
        let t = T::new("store-prune");
        let manual = t
            .store
            .create(&t.folders(), Reason::Manual, "keep me", Limits::default())
            .unwrap();
        for i in 0..KEEP_AUTOMATIC + 3 {
            t.write("save1.json", &format!("{i}"));
            t.store
                .create(&t.folders(), Reason::Exit, "", Limits::default())
                .unwrap();
        }
        let list = t.store.list().unwrap();
        assert_eq!(list.len(), KEEP_AUTOMATIC + 1);
        assert!(list.iter().any(|s| s.id == manual.snapshot.id));
        assert_eq!(t.blobs(), KEEP_AUTOMATIC);
    }

    #[test]
    fn refuses_huge_folders_and_missing_folders_are_empty() {
        let t = T::new("store-limits");
        t.write("a", "1234");
        t.write("b", "5678");
        let tiny = Limits {
            max_files: 1,
            max_bytes: 1 << 20,
        };
        assert!(matches!(
            t.store.create(&t.folders(), Reason::Manual, "", tiny),
            Err(Error::TooLarge { .. })
        ));
        let small = Limits {
            max_files: 10,
            max_bytes: 6,
        };
        assert!(matches!(
            t.store.create(&t.folders(), Reason::Manual, "", small),
            Err(Error::TooLarge { .. })
        ));
        assert!(matches!(
            t.store.create(&[], Reason::Manual, "", Limits::default()),
            Err(Error::NoFolders)
        ));
        let missing = vec![t.base.join("nope").to_string_lossy().into_owned()];
        let c = t
            .store
            .create(&missing, Reason::Manual, "", Limits::default())
            .unwrap();
        assert_eq!(c.snapshot.files, 0);
    }

    #[test]
    fn rejects_bad_ids_and_paths() {
        let t = T::new("store-ids");
        assert!(matches!(
            t.store.manifest("..\\..\\x"),
            Err(Error::UnknownSnapshot(_))
        ));
        assert!(t.store.read_blob("../x").is_err());
        assert!(safe_rel("a/b.sav").is_some());
        assert!(safe_rel("../evil").is_none());
        assert!(safe_rel("C:/Windows/x").is_none());
        assert!(safe_rel("/abs").is_none());
        assert!(safe_rel("").is_none());
    }
}
