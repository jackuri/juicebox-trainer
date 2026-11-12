//! Windows "Browse…" pickers for fields that used to need a pasted path
//! (save folders, a game's exe). They only return a path; the command the
//! path is then given to checks it as before.

use std::path::Path;

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::commands::{CmdResult, CommandError};

/// Longest title the UI may set on a picker.
const MAX_TITLE: usize = 120;

fn title(t: &str) -> String {
    let t = t.trim();
    if t.is_empty() {
        "Choose".into()
    } else {
        t.chars().take(MAX_TITLE).collect()
    }
}

fn path_of(p: tauri_plugin_dialog::FilePath) -> CmdResult<String> {
    p.into_path()
        .map(|p| p.display().to_string())
        .map_err(|e| CommandError::new("io", e.to_string()))
}

/// The folder to open the picker in: `start` if it is an existing folder, or
/// the folder of an existing file.
fn start_dir(start: Option<&str>) -> Option<&Path> {
    let p = Path::new(start?.trim());
    if p.as_os_str().is_empty() {
        return None;
    }
    if p.is_dir() {
        Some(p)
    } else {
        p.parent().filter(|d| d.is_dir())
    }
}

/// Lets the player pick a folder. `None` if they cancelled.
#[tauri::command]
pub async fn pick_folder(
    app: AppHandle,
    title_text: String,
    start: Option<String>,
) -> CmdResult<Option<String>> {
    let mut d = app.dialog().file().set_title(title(&title_text));
    if let Some(dir) = start_dir(start.as_deref()) {
        d = d.set_directory(dir);
    }
    d.blocking_pick_folder().map(path_of).transpose()
}

/// Lets the player pick a program (`.exe`). `None` if they cancelled.
#[tauri::command]
pub async fn pick_exe(
    app: AppHandle,
    title_text: String,
    start: Option<String>,
) -> CmdResult<Option<String>> {
    let mut d = app
        .dialog()
        .file()
        .set_title(title(&title_text))
        .add_filter("Programs", &["exe"]);
    if let Some(dir) = start_dir(start.as_deref()) {
        d = d.set_directory(dir);
    }
    d.blocking_pick_file().map(path_of).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_short_and_never_empty() {
        assert_eq!(title("  "), "Choose");
        assert_eq!(title(" Pick a folder "), "Pick a folder");
        assert_eq!(title(&"x".repeat(500)).chars().count(), MAX_TITLE);
    }

    #[test]
    fn starts_in_an_existing_folder_only() {
        let dir = std::env::temp_dir();
        let d = dir.display().to_string();
        assert_eq!(start_dir(Some(&d)), Some(dir.as_path()));
        let file = dir.join("ot-no-such-file.exe").display().to_string();
        assert_eq!(start_dir(Some(&file)), Some(dir.as_path()));
        assert_eq!(start_dir(Some(r"Q:\ot\no\such\folder\game.exe")), None);
        assert_eq!(start_dir(Some("")), None);
        assert_eq!(start_dir(None), None);
    }
}

/// Lets the player pick a Cheat Engine table (`.CT`), to share it (v1.27.0).
/// `None` if they cancelled.
#[tauri::command]
pub async fn pick_table(app: AppHandle) -> CmdResult<Option<String>> {
    app.dialog()
        .file()
        .set_title("Choose the Cheat Engine table (.CT)")
        .add_filter("Cheat Engine table", &[jb_packs::ct::EXTENSION, "CT"])
        .blocking_pick_file()
        .map(path_of)
        .transpose()
}
