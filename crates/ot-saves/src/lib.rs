//! Save manager (PLAN.md §2.3, roadmap v0.6.0): finds a game's save folders,
//! backs them up when the game starts and closes (and on request), compares
//! backups and restores them.
//!
//! Save files are only copied and compared as bytes here; nothing in them is
//! parsed or run. Restoring goes through the anti-cheat guard's install-folder
//! check and needs the game closed (see [`restore`]).

pub mod diff;
pub mod error;
pub mod folders;
pub mod library;
pub mod locate;
pub mod restore;
pub mod steam;
pub mod store;
pub mod watch;

pub use diff::{Diff, Side, diff};
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
