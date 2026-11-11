//! Crash-safe restore for code changes.
//!
//! Every change JuiceBox Trainer makes to a game's code (a patch) is kept in a
//! ledger by the [`AttachedProcess`] that made it, together with the original
//! bytes. Detaching restores every entry.
//!
//! With a journal folder set ([`AttachedProcess::enable_journal`]) the ledger
//! is also written to disk *before* each change, so a crash or a killed
//! JuiceBox Trainer can't leave a game patched for good: [`recover`] (run at the
//! next start, and by the desktop app's panic handler) attaches again —
//! through the anti-cheat guard like any attach — and puts the original bytes
//! back. Each file is named after the game's pid and start time, so a reused
//! pid is never mistaken for the same run of the game.
//!
//! Recovery only ever writes the recorded original bytes over the recorded
//! patched bytes. If it finds anything else there, it leaves the bytes alone.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::aob::{Pattern, hex_string};
use crate::error::{Error, Result};
use crate::session::AttachedProcess;

/// Format version of journal files.
pub const JOURNAL_VERSION: u32 = 1;

/// One changed run of code bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeChange {
    pub address: u64,
    #[serde(with = "hex_bytes")]
    pub original: Vec<u8>,
    #[serde(with = "hex_bytes")]
    pub written: Vec<u8>,
}

impl CodeChange {
    fn end(&self) -> u64 {
        self.address + self.written.len() as u64
    }
}

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::hex_string(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        super::Pattern::parse_exact(&text).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JournalFile {
    version: u32,
    pid: u32,
    /// Process start time (FILETIME ticks).
    started: u64,
    exe: String,
    changes: Vec<CodeChange>,
}

/// Where a session's journal lives.
#[derive(Debug, Clone)]
pub(crate) struct JournalTarget {
    pub path: PathBuf,
    pub pid: u32,
    pub started: u64,
    pub exe: String,
}

/// The code changes a session has made, by address.
#[derive(Debug, Default)]
pub(crate) struct Ledger {
    changes: BTreeMap<u64, CodeChange>,
    target: Option<JournalTarget>,
}

impl Ledger {
    pub fn get(&self, address: u64) -> Option<&CodeChange> {
        self.changes.get(&address)
    }

    /// The start of an entry that overlaps `address..address+len` without
    /// being exactly that range.
    pub fn overlap(&self, address: u64, len: usize) -> Option<u64> {
        let end = address + len as u64;
        self.changes
            .values()
            .find(|c| {
                c.address < end
                    && address < c.end()
                    && (c.address != address || c.written.len() != len)
            })
            .map(|c| c.address)
    }

    pub fn insert(&mut self, change: CodeChange) -> Option<CodeChange> {
        self.changes.insert(change.address, change)
    }

    pub fn remove(&mut self, address: u64) -> Option<CodeChange> {
        self.changes.remove(&address)
    }

    pub fn all(&self) -> Vec<CodeChange> {
        self.changes.values().cloned().collect()
    }

    /// Starts writing the ledger to `target`, taking over any changes a
    /// previous run left in that file. Returns how many it took over.
    pub fn attach_file(&mut self, target: JournalTarget) -> Result<usize> {
        let mut adopted = 0;
        if let Some(file) = read_file(&target.path)?
            && file.pid == target.pid
            && file.started == target.started
        {
            for c in file.changes {
                if self.changes.contains_key(&c.address) {
                    continue;
                }
                self.changes.insert(c.address, c);
                adopted += 1;
            }
        }
        self.target = Some(target);
        self.persist()?;
        Ok(adopted)
    }

    /// Writes the ledger to its file (atomically), or deletes the file when
    /// nothing is changed. A no-op without a journal.
    pub fn persist(&self) -> Result<()> {
        let Some(t) = &self.target else {
            return Ok(());
        };
        if self.changes.is_empty() {
            return match fs::remove_file(&t.path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(io_error("remove", &t.path, &e)),
            };
        }
        let file = JournalFile {
            version: JOURNAL_VERSION,
            pid: t.pid,
            started: t.started,
            exe: t.exe.clone(),
            changes: self.all(),
        };
        let text = serde_json::to_string_pretty(&file)
            .map_err(|e| Error::Journal(format!("could not encode: {e}")))?;
        let tmp = t.path.with_extension("json.tmp");
        fs::write(&tmp, text).map_err(|e| io_error("write", &tmp, &e))?;
        fs::rename(&tmp, &t.path).map_err(|e| io_error("rename", &t.path, &e))
    }

    /// Forgets the file without touching it (the process is gone).
    pub fn delete_file(&mut self) {
        if let Some(t) = self.target.take() {
            let _ = fs::remove_file(&t.path);
        }
    }
}

fn io_error(op: &str, path: &Path, e: &std::io::Error) -> Error {
    Error::Journal(format!("could not {op} {}: {e}", path.display()))
}

fn read_file(path: &Path) -> Result<Option<JournalFile>> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error("read", path, &e)),
    };
    match serde_json::from_str::<JournalFile>(&text) {
        Ok(f) if f.version == JOURNAL_VERSION => Ok(Some(f)),
        Ok(f) => Err(Error::Journal(format!(
            "{} has unknown version {}",
            path.display(),
            f.version
        ))),
        Err(e) => Err(Error::Journal(format!(
            "{} is damaged: {e}",
            path.display()
        ))),
    }
}

/// File name for one run of a game.
pub fn file_name(pid: u32, started: u64) -> String {
    format!("{pid}-{started}.json")
}

/// The app's journal folder: `%LOCALAPPDATA%\JuiceBox Trainer\restore`.
pub fn default_dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(base)
            .join(crate::names::FOLDER)
            .join("restore"),
    )
}

/// What [`recover`] did for one leftover journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recovery {
    pub exe: String,
    pub pid: u32,
    /// The game had already closed, so nothing needed undoing.
    pub game_closed: bool,
    /// Changes put back to the original bytes.
    pub restored: usize,
    /// Changes that could not be undone, and why.
    pub problems: Vec<String>,
}

/// Undoes code changes left behind by a previous run that ended without
/// cleaning up. Safe to call at any time; journals of games that have closed
/// are deleted. Journals of the *current* app's live sessions are restored too,
/// so only call this at startup or when the app is going down.
pub fn recover(dir: &Path) -> Vec<Recovery> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let file = match read_file(&path) {
            Ok(Some(f)) => f,
            Ok(None) => continue,
            Err(e) => {
                warn!("{e}; removing it");
                let _ = fs::remove_file(&path);
                continue;
            }
        };
        out.push(recover_one(dir, &path, file));
    }
    out
}

fn recover_one(dir: &Path, path: &Path, file: JournalFile) -> Recovery {
    let mut report = Recovery {
        exe: file.exe.clone(),
        pid: file.pid,
        game_closed: false,
        restored: 0,
        problems: Vec::new(),
    };
    if crate::process::start_time(file.pid) != Some(file.started) {
        // The game has closed (its code changes went with it), or the pid
        // now belongs to another program.
        report.game_closed = true;
        let _ = fs::remove_file(path);
        return report;
    }
    let process = match AttachedProcess::attach(file.pid) {
        Ok(p) => p,
        Err(e) => {
            // Blocked by the guard or not accessible: leave the file for the
            // next try. It is deleted once the game closes.
            report.problems.push(e.to_string());
            return report;
        }
    };
    if let Err(e) = process.enable_journal(dir) {
        report.problems.push(e.to_string());
        return report;
    }
    let pending = process.code_changes().len();
    let problems = process.restore_code();
    report.restored = pending - problems.len();
    report.problems = problems.iter().map(ToString::to_string).collect();
    info!(
        pid = file.pid,
        exe = %file.exe,
        restored = report.restored,
        problems = report.problems.len(),
        "recovered code changes left by an earlier run"
    );
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(address: u64, len: usize) -> CodeChange {
        CodeChange {
            address,
            original: vec![0x29; len],
            written: vec![0x90; len],
        }
    }

    #[test]
    fn overlap_ignores_exact_matches() {
        let mut l = Ledger::default();
        l.insert(change(0x1000, 6));
        assert_eq!(l.overlap(0x1000, 6), None);
        assert_eq!(l.overlap(0x1002, 2), Some(0x1000));
        assert_eq!(l.overlap(0x0FFE, 4), Some(0x1000));
        assert_eq!(l.overlap(0x1000, 5), Some(0x1000));
        assert_eq!(l.overlap(0x1006, 4), None);
        assert_eq!(l.overlap(0x0FF0, 0x10), None);
    }

    #[test]
    fn journal_round_trips_and_deletes_when_empty() {
        let dir = std::env::temp_dir().join(format!("ot-journal-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join(file_name(1, 2));
        let target = JournalTarget {
            path: path.clone(),
            pid: 1,
            started: 2,
            exe: "game.exe".into(),
        };
        let mut l = Ledger::default();
        l.attach_file(target.clone()).unwrap();
        assert!(!path.exists(), "an empty ledger has no file");
        l.insert(change(0x1000, 2));
        l.persist().unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"original\": \"29 29\""), "{text}");

        // A new session for the same run adopts the change.
        let mut again = Ledger::default();
        assert_eq!(again.attach_file(target.clone()).unwrap(), 1);
        assert_eq!(again.get(0x1000), Some(&change(0x1000, 2)));

        // Another run (different start time) does not.
        let mut other = Ledger::default();
        let t2 = JournalTarget {
            started: 3,
            ..target
        };
        assert_eq!(other.attach_file(t2).unwrap(), 0);

        again.remove(0x1000);
        again.persist().unwrap();
        assert!(!path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recover_drops_journals_of_closed_games() {
        let dir = std::env::temp_dir().join(format!("ot-recover-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let file = JournalFile {
            version: JOURNAL_VERSION,
            // pid 0 is the idle process; its start time never matches.
            pid: 0,
            started: 12345,
            exe: "gone.exe".into(),
            changes: vec![change(0x1000, 2)],
        };
        let path = dir.join(file_name(0, 12345));
        fs::write(&path, serde_json::to_string(&file).unwrap()).unwrap();
        fs::write(dir.join("junk.json"), "{not json").unwrap();
        let out = recover(&dir);
        assert_eq!(out.len(), 1);
        assert!(out[0].game_closed);
        assert!(!path.exists());
        assert!(!dir.join("junk.json").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
