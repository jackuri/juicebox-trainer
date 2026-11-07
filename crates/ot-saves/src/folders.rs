//! Windows folders: known folders (Documents, Saved Games, AppData…) and a
//! registry string reader.

use std::path::PathBuf;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
use windows::Win32::UI::Shell::{
    FOLDERID_Documents, FOLDERID_LocalAppData, FOLDERID_LocalAppDataLow, FOLDERID_RoamingAppData,
    FOLDERID_SavedGames, KNOWN_FOLDER_FLAG, SHGetKnownFolderPath,
};
use windows::core::{GUID, HSTRING, PCWSTR};

fn known(id: &GUID) -> Option<PathBuf> {
    // SAFETY: `id` points to a valid GUID; the returned string is freed below.
    let path = unsafe { SHGetKnownFolderPath(id, KNOWN_FOLDER_FLAG(0), None) }.ok()?;
    // SAFETY: on success `path` is a valid NUL-terminated wide string.
    let s = unsafe { path.to_string() }.ok();
    // SAFETY: allocated by SHGetKnownFolderPath with the COM allocator.
    unsafe { CoTaskMemFree(Some(path.0 as *const _)) };
    s.map(PathBuf::from)
}

/// The folders games save into, for the current user.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Roots {
    pub documents: Option<PathBuf>,
    pub saved_games: Option<PathBuf>,
    /// `%APPDATA%` (Roaming).
    pub roaming: Option<PathBuf>,
    /// `%LOCALAPPDATA%`.
    pub local: Option<PathBuf>,
    /// `AppData\LocalLow` (Unity games).
    pub local_low: Option<PathBuf>,
    /// Steam's install folder.
    pub steam: Option<PathBuf>,
}

impl Roots {
    /// This PC's folders.
    pub fn current() -> Self {
        Self {
            documents: known(&FOLDERID_Documents),
            saved_games: known(&FOLDERID_SavedGames),
            roaming: known(&FOLDERID_RoamingAppData),
            local: known(&FOLDERID_LocalAppData),
            local_low: known(&FOLDERID_LocalAppDataLow),
            steam: crate::steam::steam_dir(),
        }
    }
}

/// Reads a `REG_SZ` value under `HKEY_CURRENT_USER`.
pub(crate) fn registry_string(key: &str, value: &str) -> Option<String> {
    let key = HSTRING::from(key);
    let value = HSTRING::from(value);
    let mut buf = vec![0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    // SAFETY: `buf` is writable for `len` bytes; the key and value names are
    // NUL-terminated wide strings that outlive the call.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut len),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let chars = (len as usize / 2).min(buf.len());
    let s = String::from_utf16_lossy(&buf[..chars]);
    let s = s.trim_end_matches('\0').to_string();
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_this_users_folders() {
        let r = Roots::current();
        assert!(r.documents.as_ref().is_some_and(|p| p.is_absolute()));
        assert!(r.local.as_ref().is_some_and(|p| p.is_dir()));
        assert!(r.roaming.as_ref().is_some_and(|p| p.is_dir()));
    }

    #[test]
    fn reads_registry_strings() {
        // Always present for an interactive user.
        assert!(
            registry_string(r"Control Panel\Desktop", "WallPaper").is_some()
                || registry_string(r"Control Panel\International", "LocaleName").is_some()
        );
        assert_eq!(
            registry_string(r"Software\OpenTrainer\NoSuchKey", "x"),
            None
        );
    }
}
