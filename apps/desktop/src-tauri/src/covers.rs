//! A game's own `.exe` icon as a PNG, for the Games tab when no store cover is on
//! this PC (Epic games, games the player added). Windows draws the icon (the
//! same one Explorer shows); it is turned into a PNG in memory and never saved.

use std::ffi::c_void;
use std::io::Write;
use std::path::Path;

use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW, HBITMAP,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};
use windows::core::HSTRING;

/// Icon size asked for (Windows gives the closest it has).
pub const ICON_SIZE: i32 = 256;

/// The exe's icon as PNG bytes, or `None` if Windows can't draw one.
pub fn exe_icon_png(exe: &Path) -> Option<Vec<u8>> {
    if !exe.is_file() {
        return None;
    }
    // SAFETY: COM is set up for this thread only and torn down below; every
    // handle made here is released before returning.
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let out = icon_rgba(exe).map(|(w, h, rgba)| png(w, h, &rgba));
        if init.is_ok() {
            CoUninitialize();
        }
        out
    }
}

/// # Safety
/// COM must be initialised on this thread.
unsafe fn icon_rgba(exe: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let path = HSTRING::from(exe.as_os_str());
    // SAFETY: `path` is a valid wide string for the call; COM is initialised (caller).
    let factory: IShellItemImageFactory =
        unsafe { SHCreateItemFromParsingName(&path, None) }.ok()?;
    let size = SIZE {
        cx: ICON_SIZE,
        cy: ICON_SIZE,
    };
    // SAFETY: `factory` is a live COM object; the returned bitmap is ours to delete.
    let bmp = unsafe { factory.GetImage(size, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK) }.ok()?;
    // SAFETY: `bmp` was just returned by GetImage and is still alive.
    let out = unsafe { bitmap_rgba(bmp) };
    // SAFETY: deletes the bitmap GetImage gave us, once, after its last use.
    let _ = unsafe { DeleteObject(bmp.into()) };
    out
}

/// # Safety
/// `bmp` must be a valid bitmap handle.
unsafe fn bitmap_rgba(bmp: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    let mut bm = BITMAP::default();
    // SAFETY: `bm` is a BITMAP-sized, writable buffer; `bmp` is valid (caller).
    let got = unsafe {
        GetObjectW(
            bmp.into(),
            size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut BITMAP as *mut c_void),
        )
    };
    let (w, h) = (bm.bmWidth, bm.bmHeight.abs());
    if got == 0 || w <= 0 || h <= 0 || w > 1024 || h > 1024 {
        return None;
    }
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down rows
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buf = vec![0u8; (w * h * 4) as usize];
    // SAFETY: a memory DC compatible with the screen; deleted below.
    let dc = unsafe { CreateCompatibleDC(None) };
    // SAFETY: `buf` holds exactly w*h 32-bit pixels, matching `info` (top-down, 32 bpp).
    let lines = unsafe {
        GetDIBits(
            dc,
            bmp,
            0,
            h as u32,
            Some(buf.as_mut_ptr() as *mut c_void),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    // SAFETY: the DC made above, deleted once.
    let _ = unsafe { DeleteDC(dc) };
    if lines == 0 {
        return None;
    }
    bgra_to_rgba(&mut buf);
    Some((w as u32, h as u32, buf))
}

/// Windows gives blue-green-red-alpha, usually with colours already multiplied
/// by alpha. Turns it into plain RGBA. Icons without any alpha are opaque.
fn bgra_to_rgba(buf: &mut [u8]) {
    let (pixels, _) = buf.as_chunks_mut::<4>();
    let no_alpha = pixels.iter().all(|p| p[3] == 0);
    let premultiplied = pixels
        .iter()
        .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
    for p in pixels {
        p.swap(0, 2);
        if no_alpha {
            p[3] = 255;
        } else if premultiplied && p[3] > 0 && p[3] < 255 {
            let a = u32::from(p[3]);
            for c in &mut p[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

/// A minimal RGBA PNG (one IDAT, no filtering).
fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgba.len() + h as usize);
    for row in rgba.chunks_exact(w as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = z.write_all(&raw);
    let idat = z.finish().unwrap_or_default();

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
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
    fn colours_are_unpremultiplied_and_opaque_icons_stay_opaque() {
        // Half-transparent pure red, premultiplied: B G R A = 0 0 128 128.
        let mut px = vec![0, 0, 128, 128, 0, 0, 0, 0];
        bgra_to_rgba(&mut px);
        assert_eq!(px, vec![255, 0, 0, 128, 0, 0, 0, 0]);
        // No alpha at all: an old-style icon, shown opaque.
        let mut old = vec![10, 20, 30, 0];
        bgra_to_rgba(&mut old);
        assert_eq!(old, vec![30, 20, 10, 255]);
    }

    #[test]
    fn writes_a_valid_png() {
        let png = png(2, 1, &[255, 0, 0, 255, 0, 0, 255, 128]);
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[16..20], &2u32.to_be_bytes());
        assert!(png.ends_with(&[0xAE, 0x42, 0x60, 0x82])); // IEND's CRC
    }

    #[test]
    fn draws_a_real_exe_icon() {
        let exe = std::env::var_os("SystemRoot")
            .map(|r| Path::new(&r).join(r"System32\notepad.exe"))
            .unwrap();
        let png = exe_icon_png(&exe).expect("notepad has an icon");
        assert!(png.starts_with(b"\x89PNG"));
        assert!(png.len() > 500);
        assert!(exe_icon_png(Path::new(r"Q:\no\such\game.exe")).is_none());
    }
}
