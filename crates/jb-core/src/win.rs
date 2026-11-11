//! Small Win32 helpers shared by the other modules.

use windows::Win32::Foundation::{CloseHandle, HANDLE};

/// An owned Win32 handle that is closed on drop.
#[derive(Debug)]
pub(crate) struct OwnedHandle(HANDLE);

// SAFETY: process and snapshot handles are kernel object references that may be
// used from any thread; we never hand out the raw handle beyond this crate.
unsafe impl Send for OwnedHandle {}
// SAFETY: see above; all Win32 calls we make on these handles are thread-safe.
unsafe impl Sync for OwnedHandle {}

impl OwnedHandle {
    pub(crate) fn new(handle: HANDLE) -> Self {
        Self(handle)
    }

    pub(crate) fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: we own this handle and close it exactly once.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

/// Converts a NUL-terminated UTF-16 buffer to a `String`.
pub(crate) fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// True if a `windows` error carries the given Win32 error code.
pub(crate) fn is_code(
    err: &windows::core::Error,
    code: windows::Win32::Foundation::WIN32_ERROR,
) -> bool {
    err.code() == code.to_hresult()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_wide_stops_at_nul() {
        let mut buf = [0u16; 8];
        for (i, c) in "abc".encode_utf16().enumerate() {
            buf[i] = c;
        }
        assert_eq!(from_wide(&buf), "abc");
        assert_eq!(from_wide(&[]), "");
    }
}
