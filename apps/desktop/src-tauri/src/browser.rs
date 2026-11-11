//! Opening web pages in the player's browser (v1.22.0).
//!
//! Before 1.22 links were handed to `explorer.exe`, which treats some web
//! addresses (ones with `&` in them, like a forum search) as a folder and opens
//! Documents instead. `ShellExecuteW("open")` hands the address to the default
//! browser the way Windows means it to.

use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};

use crate::commands::{CmdResult, CommandError};

/// Only plain web addresses are opened (never files or other programs).
pub fn is_web_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("https://")
        && url.len() <= 2048
        && !url.chars().any(|c| c.is_control() || c == '"' || c == ' ')
}

/// Opens `url` (https only) in the default browser.
pub fn open_url(url: &str) -> CmdResult<()> {
    if !is_web_url(url) {
        return Err(CommandError::new(
            "badRequest",
            "only https web pages can be opened",
        ));
    }
    let target = HSTRING::from(url);
    // SAFETY: plain ShellExecuteW with NUL-terminated wide strings that outlive
    // the call; the target was checked to be an https address above.
    let r = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if r.0 as isize <= 32 {
        return Err(CommandError::new(
            "browser",
            "Windows couldn't open your web browser. Check that a default browser is set (Windows Settings, Apps, Default apps).",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_pages_are_opened() {
        assert!(is_web_url(
            "https://fearlessrevolution.com/search.php?keywords=a+b&terms=all"
        ));
        assert!(!is_web_url("http://example.com/"));
        assert!(!is_web_url("C:\\Windows\\notepad.exe"));
        assert!(!is_web_url("file:///C:/x"));
        assert!(!is_web_url("https://a.com/\" & calc"));
        assert!(open_url("C:\\Windows").is_err());
    }
}
