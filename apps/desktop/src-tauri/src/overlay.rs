//! The overlay: a small see-through window that stays on top of the game and
//! lists the cheats, their keys and which are on.
//!
//! It's a separate window of OpenTrainer, not drawn inside the game (nothing
//! is injected). Mouse clicks go through it to the game. It shows over games
//! in windowed or borderless mode; exclusive fullscreen hides it (PLAN §8).

use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
};
use windows::core::BOOL;

use crate::commands::{AppState, CmdResult, CommandError};

pub const LABEL: &str = "overlay";
const WIDTH: f64 = 300.0;
const HEIGHT: f64 = 420.0;
const MARGIN: i32 = 16;

struct Search {
    pid: u32,
    best: Option<(RECT, i64)>,
}

unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `Search` passed to EnumWindows below, alive for
    // the whole enumeration.
    let search = unsafe { &mut *(lparam.0 as *mut Search) };
    let mut pid = 0u32;
    // SAFETY: plain Win32 calls on a window handle EnumWindows gave us.
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid != search.pid || !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_ok() {
            let area = i64::from(r.right - r.left) * i64::from(r.bottom - r.top);
            if search.best.is_none_or(|(_, a)| area > a) {
                search.best = Some((r, area));
            }
        }
    }
    BOOL(1)
}

/// The biggest visible window of process `pid` (the game's own window).
fn game_window(pid: u32) -> Option<RECT> {
    let mut search = Search { pid, best: None };
    // SAFETY: `visit` only uses `search` during this call.
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&raw mut search as isize)) };
    search
        .best
        .map(|(r, _)| r)
        .filter(|r| r.right - r.left > 200)
}

fn place(window: &tauri::WebviewWindow, pid: Option<u32>) {
    if let Some(r) = pid.and_then(game_window) {
        let _ = window.set_position(PhysicalPosition::new(r.left + MARGIN, r.top + MARGIN * 3));
    } else if let Ok(Some(m)) = window.primary_monitor() {
        let p = m.position();
        let _ = window.set_position(PhysicalPosition::new(p.x + MARGIN, p.y + MARGIN));
    }
}

fn err(e: impl std::fmt::Display) -> CommandError {
    CommandError::new("overlay", format!("couldn't show the overlay: {e}"))
}

/// Shows or hides the overlay. It goes to the game window's top-left corner.
#[tauri::command]
pub async fn overlay_set(show: bool, app: AppHandle) -> CmdResult<bool> {
    let pid = app.state::<AppState>().attached_pid();
    if let Some(w) = app.get_webview_window(LABEL) {
        if show {
            place(&w, pid);
            w.show().map_err(err)?;
        } else {
            w.hide().map_err(err)?;
        }
        return Ok(show);
    }
    if !show {
        return Ok(false);
    }
    let w = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::App("index.html#overlay".into()))
        .title("OpenTrainer overlay")
        .inner_size(WIDTH, HEIGHT)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .build()
        .map_err(err)?;
    w.set_ignore_cursor_events(true).map_err(err)?;
    place(&w, pid);
    Ok(true)
}

#[tauri::command]
pub fn overlay_visible(app: AppHandle) -> bool {
    app.get_webview_window(LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}
