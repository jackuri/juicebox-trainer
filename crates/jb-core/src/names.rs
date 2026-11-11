//! The app's name and where it keeps things (v1.23.0).
//!
//! OpenTrainer was renamed JuiceBox Trainer in 1.23.0. Its folders moved with
//! it: [`move_legacy_folders`] moves what an OpenTrainer install left (packs,
//! settings, save backups, notes, the screen's own data) on the first start,
//! so nothing the player made is lost. Nothing is deleted: entries that
//! already exist under the new name are left where they were.

use std::path::{Path, PathBuf};

/// The app's name, as players see it.
pub const APP_NAME: &str = "JuiceBox Trainer";
/// The folder under `%APPDATA%` and `%LOCALAPPDATA%`.
pub const FOLDER: &str = "JuiceBox Trainer";
/// The app's id (also the WebView2 / AI engine folder under `%LOCALAPPDATA%`).
pub const IDENTIFIER: &str = "org.juiceboxtrainer.desktop";

/// OpenTrainer's folder name (before 1.23.0).
pub const LEGACY_FOLDER: &str = "OpenTrainer";
/// OpenTrainer's app id (before 1.23.0).
pub const LEGACY_IDENTIFIER: &str = "org.opentrainer.desktop";
/// OpenTrainer's installed program files in its `%LOCALAPPDATA%` folder: these
/// belong to the old install (its uninstaller removes them), not to the player.
const LEGACY_PROGRAM_FILES: &[&str] = &["opentrainer-desktop.exe", "uninstall.exe", "ai-agent"];

/// `%APPDATA%\JuiceBox Trainer` (packs, settings, the agreement, web games).
pub fn roaming_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join(FOLDER))
}

/// `%LOCALAPPDATA%\JuiceBox Trainer` (save backups, the restore journal,
/// problems, AI notes and logs).
pub fn local_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|a| PathBuf::from(a).join(FOLDER))
}

/// Moves each entry of `from` into `to` (made if needed) unless `to` already
/// has one of that name or it's in `skip`. Removes `from` if that empties it.
/// Returns how many entries moved.
fn merge(from: &Path, to: &Path, skip: &[&str]) -> usize {
    let Ok(entries) = std::fs::read_dir(from) else {
        return 0;
    };
    let mut moved = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if skip
            .iter()
            .any(|s| name.to_string_lossy().eq_ignore_ascii_case(s))
        {
            continue;
        }
        let target = to.join(&name);
        // Both folders: merge inside (e.g. a `saves` folder made before the move).
        if target.is_dir() && entry.path().is_dir() {
            moved += merge(&entry.path(), &target, &[]);
            continue;
        }
        if target.exists() {
            tracing::warn!(
                "not moving {}: {} already exists",
                entry.path().display(),
                target.display()
            );
            continue;
        }
        if std::fs::create_dir_all(to).is_err() {
            return moved;
        }
        match std::fs::rename(entry.path(), &target) {
            Ok(()) => moved += 1,
            // In use (the old app still open): tried again next start.
            Err(e) => tracing::warn!("couldn't move {}: {e}", entry.path().display()),
        }
    }
    // Only removes it when it's empty.
    let _ = std::fs::remove_dir(from);
    moved
}

/// Moves one folder to its new name: a rename when the new one doesn't exist
/// yet, else entry by entry. Returns how many things moved.
fn move_folder(from: &Path, to: &Path, skip: &[&str]) -> usize {
    if !from.is_dir() {
        return 0;
    }
    if skip.is_empty() && !to.exists() {
        if let Some(parent) = to.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::rename(from, to) {
            Ok(()) => return 1,
            Err(e) => tracing::warn!("couldn't rename {}: {e}", from.display()),
        }
    }
    merge(from, to, skip)
}

/// [`move_legacy_folders`] with explicit `%APPDATA%` and `%LOCALAPPDATA%`.
pub fn move_legacy_folders_in(roaming: &Path, local: &Path) -> usize {
    let mut moved = 0;
    moved += move_folder(&roaming.join(LEGACY_FOLDER), &roaming.join(FOLDER), &[]);
    // OpenTrainer was installed into this folder too: leave its program files.
    moved += move_folder(
        &local.join(LEGACY_FOLDER),
        &local.join(FOLDER),
        LEGACY_PROGRAM_FILES,
    );
    for base in [roaming, local] {
        moved += move_folder(&base.join(LEGACY_IDENTIFIER), &base.join(IDENTIFIER), &[]);
    }
    if moved > 0 {
        tracing::info!(moved, "moved OpenTrainer's folders to JuiceBox Trainer's");
    }
    moved
}

/// Moves OpenTrainer's folders to JuiceBox Trainer's (see the module docs).
/// Call before anything reads or makes the new folders. Returns how many
/// things moved (0 when there was nothing left to move).
pub fn move_legacy_folders() -> usize {
    match (
        std::env::var_os("APPDATA"),
        std::env::var_os("LOCALAPPDATA"),
    ) {
        (Some(roaming), Some(local)) => {
            move_legacy_folders_in(Path::new(&roaming), Path::new(&local))
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn moves_opentrainers_folders_without_losing_anything() {
        let base = std::env::temp_dir().join(format!("jb-names-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (roaming, local) = (base.join("Roaming"), base.join("Local"));
        // What OpenTrainer left.
        write(&roaming.join("OpenTrainer/packs/coin/pack.json"), "pack");
        write(&roaming.join("OpenTrainer/agreement.json"), "signed");
        write(&local.join("OpenTrainer/saves/games.json"), "saves");
        write(
            &local.join("OpenTrainer/opentrainer-desktop.exe"),
            "old program",
        );
        write(&local.join("OpenTrainer/ai-agent/agent.js"), "old program");
        write(
            &local.join("org.opentrainer.desktop/EBWebView/Local State"),
            "screen",
        );
        // The new install is already in place (its folder exists, with a file of the same name).
        write(
            &local.join("JuiceBox Trainer/juicebox-trainer.exe"),
            "new program",
        );
        write(&local.join("JuiceBox Trainer/problems.json"), "new");
        write(&local.join("OpenTrainer/problems.json"), "old");
        // A folder already made under the new name (by a test run, say) is merged into.
        write(&local.join("OpenTrainer/saves/blobs/ab/abcd"), "backup");
        write(&local.join("JuiceBox Trainer/saves/blobs/cd/cdef"), "other");

        assert!(move_legacy_folders_in(&roaming, &local) > 0);
        let read = |p: PathBuf| std::fs::read_to_string(p).unwrap();
        assert_eq!(
            read(roaming.join("JuiceBox Trainer/packs/coin/pack.json")),
            "pack"
        );
        assert_eq!(
            read(roaming.join("JuiceBox Trainer/agreement.json")),
            "signed"
        );
        assert!(!roaming.join("OpenTrainer").exists());
        assert_eq!(
            read(local.join("JuiceBox Trainer/saves/games.json")),
            "saves"
        );
        assert_eq!(
            read(local.join("JuiceBox Trainer/saves/blobs/ab/abcd")),
            "backup"
        );
        assert_eq!(
            read(local.join("JuiceBox Trainer/saves/blobs/cd/cdef")),
            "other"
        );
        assert_eq!(
            read(local.join("org.juiceboxtrainer.desktop/EBWebView/Local State")),
            "screen"
        );
        // The old program files stay for the old uninstaller; nothing is overwritten.
        assert!(local.join("OpenTrainer/opentrainer-desktop.exe").exists());
        assert!(
            !local
                .join("JuiceBox Trainer/opentrainer-desktop.exe")
                .exists()
        );
        assert_eq!(read(local.join("JuiceBox Trainer/problems.json")), "new");
        assert_eq!(read(local.join("OpenTrainer/problems.json")), "old");
        // A second start has nothing left to move.
        assert_eq!(move_legacy_folders_in(&roaming, &local), 0);
        let _ = std::fs::remove_dir_all(&base);
    }
}
