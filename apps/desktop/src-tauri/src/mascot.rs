//! The juice box on the taskbar changes with the season (v1.24.0): Halloween
//! in October, Thanksgiving in November, Christmas in December, Valentine's in
//! February, the everyday one otherwise (the screen picks; Themes can turn it
//! off). Only the open window's icon changes: Windows takes the desktop and
//! Start menu shortcut icon from the installed program.

use tauri::image::Image;
use tauri::{AppHandle, Manager};

use crate::commands::{CmdResult, CommandError};

/// The built-in pictures, by name. Nothing else can be set.
fn picture(name: &str) -> Option<&'static [u8]> {
    Some(match name {
        "default" => include_bytes!("../icons/moods/default.png"),
        "halloween" => include_bytes!("../icons/moods/halloween.png"),
        "thanksgiving" => include_bytes!("../icons/moods/thanksgiving.png"),
        "christmas" => include_bytes!("../icons/moods/christmas.png"),
        "valentine" => include_bytes!("../icons/moods/valentine.png"),
        _ => return None,
    })
}

/// Sets the main window's (and so the taskbar's) icon to a built-in picture.
#[tauri::command]
pub fn app_icon_set(name: String, app: AppHandle) -> CmdResult<()> {
    let bytes = picture(&name)
        .ok_or_else(|| CommandError::new("badRequest", format!("no icon called {name}")))?;
    let image =
        Image::from_bytes(bytes).map_err(|e| CommandError::new("io", format!("icon: {e}")))?;
    if let Some(w) = app.get_webview_window("main") {
        w.set_icon(image)
            .map_err(|e| CommandError::new("io", format!("couldn't set the icon: {e}")))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_picture_is_a_png_and_others_are_refused() {
        for n in [
            "default",
            "halloween",
            "thanksgiving",
            "christmas",
            "valentine",
        ] {
            let b = picture(n).unwrap();
            assert!(b.starts_with(b"\x89PNG"), "{n}");
            assert!(Image::from_bytes(b).is_ok(), "{n}");
        }
        assert!(picture("../../secret").is_none());
        assert!(picture("wink").is_none());
    }
}
