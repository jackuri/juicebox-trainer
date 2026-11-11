//! The juice box on the taskbar changes with the season (v1.24.0): Halloween
//! in October, Thanksgiving in November, Christmas in December, Valentine's in
//! February, the everyday one otherwise (the screen picks; Themes can turn it
//! off). Only the open window's icons change: Windows takes the desktop and
//! Start menu shortcut icon from the installed program.
//!
//! A window has two icons: the small one in its title bar and the big one on
//! the taskbar and in Alt+Tab. Tauri sets only the small one, so the big one
//! is set here directly (v1.24.1).

use std::sync::Mutex;

use tauri::image::Image;
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResourceEx, DestroyIcon, HICON, ICON_BIG, LR_DEFAULTCOLOR, SendMessageW,
    WM_SETICON,
};

use crate::commands::{CmdResult, CommandError};

/// The built-in pictures (64×64 PNG), by name. Nothing else can be set.
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

/// The big icon in use (a window doesn't copy it: kept until replaced).
static BIG: Mutex<Option<isize>> = Mutex::new(None);

/// An icon from PNG bytes (Windows reads PNG icon images directly).
fn icon_from_png(png: &[u8]) -> Option<HICON> {
    // SAFETY: `png` is a whole PNG image that outlives the call; 0x00030000
    // is the icon format version Windows expects.
    unsafe { CreateIconFromResourceEx(png, true, 0x0003_0000, 0, 0, LR_DEFAULTCOLOR) }.ok()
}

/// Sets the window's big icon (taskbar, Alt+Tab), freeing the previous one.
fn set_big_icon(hwnd: HWND, png: &[u8]) -> Result<(), String> {
    let icon = icon_from_png(png).ok_or("Windows couldn't read the icon")?;
    // SAFETY: a valid window and icon; the window only borrows the icon, which
    // stays alive (in BIG) until the next one replaces it.
    unsafe {
        SendMessageW(
            hwnd,
            WM_SETICON,
            Some(WPARAM(ICON_BIG as usize)),
            Some(LPARAM(icon.0 as isize)),
        );
    }
    let old = BIG
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .replace(icon.0 as isize);
    if let Some(old) = old {
        // SAFETY: made by CreateIconFromResourceEx above, no longer used.
        let _ = unsafe { DestroyIcon(HICON(old as *mut core::ffi::c_void)) };
    }
    Ok(())
}

/// Sets the main window's icons (title bar and taskbar) to a built-in picture.
#[tauri::command]
pub fn app_icon_set(name: String, app: AppHandle) -> CmdResult<()> {
    let bytes = picture(&name)
        .ok_or_else(|| CommandError::new("badRequest", format!("no icon called {name}")))?;
    let image =
        Image::from_bytes(bytes).map_err(|e| CommandError::new("io", format!("icon: {e}")))?;
    if let Some(w) = app.get_webview_window("main") {
        w.set_icon(image)
            .map_err(|e| CommandError::new("io", format!("couldn't set the icon: {e}")))?;
        let hwnd = w
            .hwnd()
            .map_err(|e| CommandError::new("io", format!("no window: {e}")))?;
        set_big_icon(hwnd, bytes).map_err(|e| CommandError::new("io", e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_picture_is_a_png_windows_can_use_and_others_are_refused() {
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
            let icon = icon_from_png(b).unwrap_or_else(|| panic!("{n}: no icon"));
            // SAFETY: made just above.
            let _ = unsafe { DestroyIcon(icon) };
        }
        assert!(picture("../../secret").is_none());
        assert!(picture("wink").is_none());
    }
}
