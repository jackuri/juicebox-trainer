//! Save manager (PLAN.md §2.3): finds a game's save folders (and Unity
//! registry saves), backs them up when the game starts and closes (and on
//! request), compares and restores backups (v0.6.0), and opens and edits saves
//! (v0.7.0, more formats in v1.10.0): JSON, XML, INI, YAML and Unreal saves with
//! gzip / zlib / Base64 / lz-string wrappers and checksums, PlayerPrefs, a hex
//! fallback, and quick codes from cheat packs. Saves can be copied between the
//! player's Steam accounts (v1.11.0, see [`accounts`]).
//!
//! Saves are parsed structurally from their bytes; nothing in them is ever
//! deserialised into objects or run. Restoring and editing go through the
//! anti-cheat guard's install-folder check, need the game closed, and back up
//! first (see [`restore`] and [`editor`]).

pub mod accounts;
pub mod codec;
pub mod covers;
pub mod diff;
pub mod editor;
pub mod error;
pub mod folders;
pub mod installs;
pub mod library;
pub mod locate;
pub mod quick;
pub mod registry;
pub mod restore;
pub mod steam;
pub mod store;
pub mod watch;

pub use codec::{Edit, Field, FieldKind};
pub use diff::{Diff, Side, diff};
pub use editor::{EditReport, FileView, SaveFile};
pub use error::{Error, Result};
pub use folders::Roots;
pub use library::{Game, Library, default_root};
pub use locate::{CloudInfo, Located, SaveFolder, locate};
pub use restore::{RestoreOptions, RestoreReport, is_running, restore};
pub use store::{Created, Limits, Reason, Store, Summary};
pub use watch::{WatchEvent, WatchKind, Watcher};

/// A scratch folder under `target/tmp` (not `%TEMP%`, see the guard tests).
#[cfg(test)]
pub(crate) fn test_dir(name: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("tmp")
        .join(format!("ot-saves-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    // Without `..`, so paths compare cleanly in tests.
    std::fs::canonicalize(&p)
        .ok()
        .and_then(|c| {
            let s = c.to_string_lossy().into_owned();
            s.strip_prefix(r"\\?\").map(std::path::PathBuf::from)
        })
        .unwrap_or(p)
}
