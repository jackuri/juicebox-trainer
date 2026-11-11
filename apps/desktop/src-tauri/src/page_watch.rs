//! Noticing when a page crashes or freezes (v1.22.0).
//!
//! OpenTrainer's own screen and the web game window are WebView2 pages. When
//! the part of WebView2 that runs a page dies or stops responding, WebView2
//! says so (`ProcessFailed`); without listening, the window just goes blank
//! or stays frozen with no word why.

use tauri::WebviewWindow;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
    COREWEBVIEW2_PROCESS_FAILED_REASON, COREWEBVIEW2_PROCESS_FAILED_REASON_OUT_OF_MEMORY,
    ICoreWebView2ProcessFailedEventArgs2,
};
use webview2_com::ProcessFailedEventHandler;
use windows::core::Interface;

/// What went wrong with a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageFailure {
    /// The page's process ended (it crashed, or Windows ended it).
    Crashed { out_of_memory: bool },
    /// Only a part of the page crashed (a frame, where many web games run).
    PartCrashed,
    /// The page stopped responding (it's frozen; it may still recover).
    Frozen,
    /// WebView2 itself ended: the window can't be used any more.
    EngineGone,
}

/// Maps WebView2's report to a [`PageFailure`]; `None` for ones that fix
/// themselves (the graphics part restarting, helpers ending).
pub fn classify(
    kind: COREWEBVIEW2_PROCESS_FAILED_KIND,
    out_of_memory: bool,
) -> Option<PageFailure> {
    match kind {
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED => {
            Some(PageFailure::Crashed { out_of_memory })
        }
        COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED => {
            Some(PageFailure::PartCrashed)
        }
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE => Some(PageFailure::Frozen),
        COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED => Some(PageFailure::EngineGone),
        _ => None,
    }
}

/// Calls `on_fail` (on a new thread) whenever `window`'s page crashes or freezes.
pub fn watch(window: &WebviewWindow, on_fail: impl Fn(PageFailure) + Send + Sync + 'static) {
    let on_fail = std::sync::Arc::new(on_fail);
    let label = window.label().to_string();
    let result = window.with_webview(move |wv| {
        let on_fail = std::sync::Arc::clone(&on_fail);
        let handler = ProcessFailedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
            // SAFETY: reading the event's own arguments inside its handler.
            unsafe { args.ProcessFailedKind(&raw mut kind)? };
            let out_of_memory = args
                .cast::<ICoreWebView2ProcessFailedEventArgs2>()
                .ok()
                .and_then(|a2| {
                    let mut reason = COREWEBVIEW2_PROCESS_FAILED_REASON::default();
                    // SAFETY: as above.
                    unsafe { a2.Reason(&raw mut reason) }.ok()?;
                    Some(reason == COREWEBVIEW2_PROCESS_FAILED_REASON_OUT_OF_MEMORY)
                })
                .unwrap_or(false);
            if let Some(failure) = classify(kind, out_of_memory) {
                let on_fail = std::sync::Arc::clone(&on_fail);
                // Off WebView2's thread: the reaction may reload the page.
                std::thread::spawn(move || on_fail(failure));
            }
            Ok(())
        }));
        let mut token = 0i64;
        // SAFETY: the controller and its webview belong to this window and are
        // alive inside `with_webview`.
        let added = unsafe {
            wv.controller()
                .CoreWebView2()
                .and_then(|core| core.add_ProcessFailed(&handler, &raw mut token))
        };
        if let Err(e) = added {
            tracing::warn!("can't watch the page for crashes: {e}");
        }
    });
    if let Err(e) = result {
        tracing::warn!(window = %label, "can't watch the page for crashes: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED;

    #[test]
    fn only_real_page_problems_are_reported() {
        assert_eq!(
            classify(COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED, true),
            Some(PageFailure::Crashed {
                out_of_memory: true
            })
        );
        assert_eq!(
            classify(
                COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
                false
            ),
            Some(PageFailure::Frozen)
        );
        assert_eq!(
            classify(
                COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
                false
            ),
            Some(PageFailure::EngineGone)
        );
        // The graphics part restarts by itself.
        assert_eq!(
            classify(COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED, false),
            None
        );
    }
}
