//! Global hotkeys for loaded packs, via Win32 `RegisterHotKey`.
//!
//! One thread owns the registrations and runs a message loop. It never touches
//! app state: key presses go to a second thread that calls the handler, so a
//! command holding the state lock can wait on a registration without
//! deadlocking. A [`Registration`] unregisters its keys when dropped, so they
//! go away with the trainer (detach, game exit, guard block, unload).
//!
//! `RegisterHotKey` is a normal, documented API: no keyboard hooks, no
//! polling. A key registered here is not passed on to other apps while the
//! pack is loaded, and a key another app already holds is reported as a
//! conflict.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use ot_packs::Hotkey;
use tracing::{info, warn};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_APP, WM_HOTKEY,
};

/// Posted to the hotkey thread when a command is waiting.
const WM_WAKE: u32 = WM_APP + 1;

/// Called on the dispatch thread with (registration generation, cheat id).
pub type Handler = Box<dyn Fn(u64, &str) + Send + 'static>;

/// Result of registering one cheat's key: `None` if it worked, or why not.
pub type KeyResult = (String, Option<String>);

enum Cmd {
    Set {
        generation: u64,
        keys: Vec<(String, Hotkey)>,
        reply: Sender<Vec<KeyResult>>,
    },
    Clear {
        generation: u64,
    },
}

/// Handle to the hotkey thread.
#[derive(Debug)]
pub struct HotkeyService {
    tx: Sender<Cmd>,
    thread_id: u32,
    next_generation: AtomicU64,
}

impl HotkeyService {
    /// Starts the hotkey and dispatch threads.
    pub fn start(handler: Handler) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel::<Cmd>();
        let (events_tx, events_rx) = mpsc::channel::<(u64, String)>();
        let (ready_tx, ready_rx) = mpsc::channel::<u32>();
        thread::Builder::new()
            .name("ot-hotkeys".into())
            .spawn(move || run(rx, events_tx, ready_tx))?;
        thread::Builder::new()
            .name("ot-hotkey-dispatch".into())
            .spawn(move || {
                for (generation, id) in events_rx {
                    handler(generation, &id);
                }
            })?;
        let thread_id = ready_rx
            .recv()
            .map_err(|_| std::io::Error::other("hotkey thread did not start"))?;
        Ok(Self {
            tx,
            thread_id,
            next_generation: AtomicU64::new(1),
        })
    }

    fn send(&self, cmd: Cmd) {
        if self.tx.send(cmd).is_ok() {
            // SAFETY: posts a plain message to our own thread's queue.
            let _ = unsafe { PostThreadMessageW(self.thread_id, WM_WAKE, WPARAM(0), LPARAM(0)) };
        }
    }

    /// Replaces all registered keys with `keys`. Returns the registration
    /// (drop it to unregister) and a per-cheat result.
    pub fn register(&self, keys: Vec<(String, Hotkey)>) -> (Registration, Vec<KeyResult>) {
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst);
        let (reply_tx, reply_rx) = mpsc::channel();
        self.send(Cmd::Set {
            generation,
            keys,
            reply: reply_tx,
        });
        let results = reply_rx.recv().unwrap_or_default();
        (
            Registration {
                generation,
                tx: self.tx.clone(),
                thread_id: self.thread_id,
            },
            results,
        )
    }
}

/// Live hotkey registration. Dropping it unregisters the keys (unless a newer
/// registration already replaced them).
#[derive(Debug)]
pub struct Registration {
    generation: u64,
    tx: Sender<Cmd>,
    thread_id: u32,
}

impl Registration {
    pub fn id(&self) -> u64 {
        self.generation
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        if self
            .tx
            .send(Cmd::Clear {
                generation: self.generation,
            })
            .is_ok()
        {
            // SAFETY: posts a plain message to the hotkey thread's queue.
            let _ = unsafe { PostThreadMessageW(self.thread_id, WM_WAKE, WPARAM(0), LPARAM(0)) };
        }
    }
}

struct Current {
    generation: u64,
    /// Registered Win32 ids and the cheat each belongs to.
    ids: Vec<(i32, String)>,
}

fn unregister(current: &mut Option<Current>) {
    if let Some(c) = current.take() {
        for (id, _) in c.ids {
            // SAFETY: unregisters a hotkey this thread registered.
            let _ = unsafe { UnregisterHotKey(None, id) };
        }
    }
}

fn run(rx: Receiver<Cmd>, events: Sender<(u64, String)>, ready: Sender<u32>) {
    let mut msg = MSG::default();
    // SAFETY: creates this thread's message queue before anyone posts to it.
    unsafe {
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
    }
    // SAFETY: plain Win32 call.
    let _ = ready.send(unsafe { GetCurrentThreadId() });
    let mut current: Option<Current> = None;
    loop {
        // SAFETY: `msg` is a valid out-buffer; thread messages only (no hwnd).
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if r.0 <= 0 {
            break;
        }
        match msg.message {
            WM_HOTKEY => {
                if let Some(c) = &current {
                    let id = msg.wParam.0 as i32;
                    if let Some((_, cheat)) = c.ids.iter().find(|(i, _)| *i == id) {
                        let _ = events.send((c.generation, cheat.clone()));
                    }
                }
            }
            WM_WAKE => {
                while let Ok(cmd) = rx.try_recv() {
                    match cmd {
                        Cmd::Set {
                            generation,
                            keys,
                            reply,
                        } => {
                            unregister(&mut current);
                            let mut ids = Vec::new();
                            let mut results = Vec::new();
                            for (n, (cheat, hk)) in keys.into_iter().enumerate() {
                                let id = n as i32 + 1;
                                let mods = HOT_KEY_MODIFIERS(hk.modifiers()) | MOD_NOREPEAT;
                                // SAFETY: thread-bound hotkey (no hwnd); id is unique here.
                                match unsafe { RegisterHotKey(None, id, mods, u32::from(hk.vk)) } {
                                    Ok(()) => {
                                        ids.push((id, cheat.clone()));
                                        results.push((cheat, None));
                                    }
                                    Err(e) => {
                                        warn!(%cheat, key = %hk, "hotkey not registered: {e}");
                                        results.push((
                                            cheat,
                                            Some(format!("{hk} is already used by another app")),
                                        ));
                                    }
                                }
                            }
                            info!(registered = ids.len(), "hotkeys set");
                            current = Some(Current { generation, ids });
                            let _ = reply.send(results);
                        }
                        Cmd::Clear { generation } => {
                            if current.as_ref().is_some_and(|c| c.generation == generation) {
                                unregister(&mut current);
                                info!("hotkeys cleared");
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    unregister(&mut current);
}

/// These need an interactive desktop session (RegisterHotKey/SendInput), so
/// they're ignored in CI. Run locally with
/// `cargo test -p opentrainer-desktop -- --ignored hotkeys`.
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::Duration;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
        VIRTUAL_KEY,
    };

    /// A combo nobody uses: Ctrl+Alt+Shift+F24.
    fn combo() -> Hotkey {
        Hotkey::parse("Ctrl+Alt+Shift+F24").unwrap()
    }

    fn key(vk: u16, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    #[test]
    #[ignore = "needs an interactive desktop"]
    fn hotkeys_fire_conflict_and_clear() {
        let (tx, rx) = mpsc::channel::<(u64, String)>();
        let tx = std::sync::Mutex::new(tx);
        let service = HotkeyService::start(Box::new(move |g, id| {
            let _ = tx.lock().unwrap().send((g, id.to_string()));
        }))
        .unwrap();
        let (reg, results) = service.register(vec![("cheat".into(), combo())]);
        assert_eq!(results, vec![("cheat".to_string(), None)]);

        // Another app (here: a second service) can't take the same key.
        let other = HotkeyService::start(Box::new(|_, _| {})).unwrap();
        let (_other_reg, clash) = other.register(vec![("x".into(), combo())]);
        assert!(
            clash[0].1.as_deref().unwrap().contains("already used"),
            "{clash:?}"
        );

        // Press it for real.
        let (ctrl, alt, shift, f24) = (0x11, 0x12, 0x10, 0x87);
        let inputs = [
            key(ctrl, false),
            key(alt, false),
            key(shift, false),
            key(f24, false),
            key(f24, true),
            key(shift, true),
            key(alt, true),
            key(ctrl, true),
        ];
        // SAFETY: valid INPUT array of the size we pass.
        let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
        assert_eq!(sent as usize, inputs.len());
        let (g, id) = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("hotkey event");
        assert_eq!((g, id.as_str()), (reg.id(), "cheat"));

        // After the registration drops, the key no longer fires.
        drop(reg);
        std::thread::sleep(Duration::from_millis(100));
        // SAFETY: as above.
        unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
        assert_eq!(
            rx.recv_timeout(Duration::from_millis(500)),
            Err(RecvTimeoutError::Timeout)
        );
    }
}
