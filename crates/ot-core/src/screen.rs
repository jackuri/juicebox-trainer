//! Pictures of a game's window (v1.21.0), so the AI helper can read the
//! numbers on screen itself instead of asking the player each time.
//!
//! Read-only: one window is copied into a bitmap (`PrintWindow`, or the screen
//! under the window when that comes out black), scaled down and returned as a
//! PNG. Nothing is saved to disk, and only the window asked for is captured.

use std::io::Write;

use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, ClientToScreen, CreateCompatibleBitmap,
    CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC,
    SRCCOPY, SelectObject,
};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClientRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
};
use windows::core::BOOL;

/// A picture of a window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shot {
    pub width: u32,
    pub height: u32,
    /// The picture as a PNG file.
    pub png: Vec<u8>,
}

/// Longest side of the pictures handed out (enough to read numbers).
pub const MAX_SIDE: u32 = 1280;

struct Search {
    pid: u32,
    best: Option<(HWND, i64)>,
}

unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `Search` passed to EnumWindows below, alive for
    // the whole call.
    let search = unsafe { &mut *(lparam.0 as *mut Search) };
    let mut pid = 0u32;
    // SAFETY: plain Win32 calls on a window handle EnumWindows gave us.
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&raw mut pid));
        if pid != search.pid || !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        let mut r = RECT::default();
        if GetClientRect(hwnd, &raw mut r).is_ok() {
            let area = i64::from(r.right - r.left) * i64::from(r.bottom - r.top);
            if area > 100 * 100 && search.best.is_none_or(|(_, a)| area > a) {
                search.best = Some((hwnd, area));
            }
        }
    }
    BOOL(1)
}

/// The process's biggest visible window (its game window), as a raw handle.
pub fn window_of(pid: u32) -> Option<isize> {
    let mut search = Search { pid, best: None };
    // SAFETY: `visit` only touches `search`, which outlives the call.
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&raw mut search as isize)) };
    search.best.map(|(h, _)| h.0 as isize)
}

/// A picture of the process's game window.
pub fn capture_process(pid: u32) -> Result<Shot, String> {
    let hwnd = window_of(pid)
        .ok_or("the game has no window that can be seen (a console game, or one still starting)")?;
    capture_window(hwnd, MAX_SIDE)
}

/// Copies a window's inside (`PrintWindow`, else the screen under it) and
/// returns it as a PNG no bigger than `max_side` on its longest side.
pub fn capture_window(hwnd: isize, max_side: u32) -> Result<Shot, String> {
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    // SAFETY: IsIconic / GetClientRect only read the window's state.
    let (w, h) = unsafe {
        if IsIconic(hwnd).as_bool() {
            return Err("the game window is minimized".into());
        }
        let mut r = RECT::default();
        GetClientRect(hwnd, &raw mut r).map_err(|e| e.to_string())?;
        (r.right - r.left, r.bottom - r.top)
    };
    if w < 16 || h < 16 {
        return Err("the game window is too small to see".into());
    }
    let mut bgra = grab(hwnd, w, h, true)?;
    if looks_black(&bgra) {
        // Some games draw past PrintWindow (DirectX): copy the screen instead.
        bgra = grab(hwnd, w, h, false)?;
        if looks_black(&bgra) {
            return Err("the picture of the game came out black (a full-screen game?); ask the player instead".into());
        }
    }
    let (sw, sh, rgb) = scale(&bgra, w as u32, h as u32, max_side);
    Ok(Shot {
        width: sw,
        height: sh,
        png: png_rgb(sw, sh, &rgb),
    })
}

/// The window's inside as top-down BGRA rows.
fn grab(hwnd: HWND, w: i32, h: i32, print: bool) -> Result<Vec<u8>, String> {
    // SAFETY: standard GDI copy into our own bitmap; every object made here is
    // freed before returning.
    unsafe {
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(dc, bmp.into());
        let ok = if print {
            // PW_CLIENTONLY | PW_RENDERFULLCONTENT
            PrintWindow(hwnd, dc, PRINT_WINDOW_FLAGS(1 | 2)).as_bool()
        } else {
            let mut origin = windows::Win32::Foundation::POINT { x: 0, y: 0 };
            let _ = ClientToScreen(hwnd, &raw mut origin);
            BitBlt(dc, 0, 0, w, h, Some(screen), origin.x, origin.y, SRCCOPY).is_ok()
        };
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; w as usize * h as usize * 4];
        SelectObject(dc, old);
        let lines = GetDIBits(
            dc,
            bmp,
            0,
            h as u32,
            Some(pixels.as_mut_ptr().cast()),
            &raw mut info,
            DIB_RGB_COLORS,
        );
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(dc);
        ReleaseDC(None, screen);
        if !ok || lines == 0 {
            return Err("couldn't copy the game window".into());
        }
        Ok(pixels)
    }
}

/// Nearly every pixel is black.
fn looks_black(bgra: &[u8]) -> bool {
    let lit = bgra
        .as_chunks::<4>()
        .0
        .iter()
        .step_by(7)
        .filter(|p| u16::from(p[0]) + u16::from(p[1]) + u16::from(p[2]) > 24)
        .count();
    lit * 7 * 4 < bgra.len() / 100 // under ~1 % lit
}

/// Box-filters BGRA down so the longest side is at most `max_side`; returns RGB.
fn scale(bgra: &[u8], w: u32, h: u32, max_side: u32) -> (u32, u32, Vec<u8>) {
    let f = w.max(h).div_ceil(max_side.max(1)).max(1);
    let (sw, sh) = (w / f, h / f);
    let mut out = Vec::with_capacity(sw as usize * sh as usize * 3);
    for y in 0..sh {
        for x in 0..sw {
            let mut sum = [0u32; 3];
            for dy in 0..f {
                for dx in 0..f {
                    let i = (((y * f + dy) * w + (x * f + dx)) * 4) as usize;
                    sum[0] += u32::from(bgra[i + 2]);
                    sum[1] += u32::from(bgra[i + 1]);
                    sum[2] += u32::from(bgra[i]);
                }
            }
            let n = f * f;
            out.extend(sum.iter().map(|s| (s / n) as u8));
        }
    }
    (sw, sh, out)
}

/// A minimal RGB PNG (one IDAT, no filtering).
pub fn png_rgb(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgb.len() + h as usize);
    for row in rgb.chunks_exact(w as usize * 3) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    let _ = z.write_all(&raw);
    let idat = z.finish().unwrap_or_default();
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit RGB
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &idat);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = flate2::Crc::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.sum().to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_down_and_writes_a_png() {
        // 4x2 BGRA: left half red, right half blue.
        let mut bgra = Vec::new();
        for _ in 0..2 {
            for x in 0..4 {
                bgra.extend_from_slice(if x < 2 {
                    &[0, 0, 255, 255]
                } else {
                    &[255, 0, 0, 255]
                });
            }
        }
        let (w, h, rgb) = scale(&bgra, 4, 2, 2);
        assert_eq!((w, h), (2, 1));
        assert_eq!(rgb, [255, 0, 0, 0, 0, 255]);
        let png = png_rgb(w, h, &rgb);
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(&png[16..24], &[0, 0, 0, 2, 0, 0, 0, 1]);
        assert!(looks_black(&[0u8; 4000]));
        assert!(!looks_black(&[200u8; 4000]));
    }

    #[test]
    fn a_process_without_windows_has_no_picture() {
        // This test process has no window.
        assert!(window_of(std::process::id()).is_none());
        assert!(capture_process(std::process::id()).is_err());
    }
}
