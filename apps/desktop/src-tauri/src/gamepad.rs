//! Controller hotkeys: polls XInput (Xbox-style controllers) and fires a
//! cheat when its button chord is pressed.
//!
//! Reading the controller is shared with the game (nothing is blocked or
//! hidden), which is why chords need two or more buttons. Polling only runs
//! while a pack with chords is loaded. Like keyboard hotkeys, presses go to a
//! dispatch thread, and a [`PadRegistration`] removes its chords when dropped.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use jb_packs::PadChord;
use windows::Win32::UI::Input::XboxController::{XINPUT_STATE, XInputGetState};

use crate::hotkeys::Handler;

const POLL: Duration = Duration::from_millis(16);
const IDLE: Duration = Duration::from_millis(250);
/// Checking an empty controller slot is slow; do it now and then.
const RECHECK_EMPTY: Duration = Duration::from_secs(2);
const ERROR_SUCCESS: u32 = 0;

type Chords = Option<(u64, Vec<(String, PadChord)>)>;

#[derive(Debug)]
pub struct PadService {
    chords: Arc<Mutex<Chords>>,
    next_generation: AtomicU64,
}

impl PadService {
    pub fn start(handler: Handler) -> std::io::Result<Self> {
        let chords: Arc<Mutex<Chords>> = Arc::default();
        let (tx, rx) = mpsc::channel::<(u64, String)>();
        let shared = Arc::clone(&chords);
        thread::Builder::new()
            .name("ot-gamepad".into())
            .spawn(move || run(&shared, &tx))?;
        thread::Builder::new()
            .name("ot-gamepad-dispatch".into())
            .spawn(move || {
                for (generation, id) in rx {
                    handler(generation, &id);
                }
            })?;
        Ok(Self {
            chords,
            next_generation: AtomicU64::new(1),
        })
    }

    /// Replaces the watched chords. Drop the registration to stop.
    pub fn register(&self, chords: Vec<(String, PadChord)>) -> PadRegistration {
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst);
        *self.chords.lock().unwrap_or_else(|e| e.into_inner()) = Some((generation, chords));
        PadRegistration {
            generation,
            chords: Arc::clone(&self.chords),
        }
    }
}

#[derive(Debug)]
pub struct PadRegistration {
    generation: u64,
    chords: Arc<Mutex<Chords>>,
}

impl PadRegistration {
    pub fn id(&self) -> u64 {
        self.generation
    }
}

impl Drop for PadRegistration {
    fn drop(&mut self) {
        let mut c = self.chords.lock().unwrap_or_else(|e| e.into_inner());
        if c.as_ref().is_some_and(|(g, _)| *g == self.generation) {
            *c = None;
        }
    }
}

/// Which chords are held now; a chord fires when it goes from not held to
/// held, so holding it doesn't repeat.
fn edges(
    chords: &[(String, PadChord)],
    was: &mut Vec<bool>,
    held: impl Fn(&PadChord) -> bool,
) -> Vec<usize> {
    was.resize(chords.len(), false);
    let mut fired = Vec::new();
    for (i, (_, c)) in chords.iter().enumerate() {
        let now = held(c);
        if now && !was[i] {
            fired.push(i);
        }
        was[i] = now;
    }
    fired
}

fn run(chords: &Mutex<Chords>, events: &Sender<(u64, String)>) {
    let mut connected = [false; 4];
    let mut last_check = [None::<Instant>; 4];
    let mut was: [Vec<bool>; 4] = Default::default();
    let mut generation = 0;
    loop {
        let current = chords.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let Some((g, list)) = current.filter(|(_, l)| !l.is_empty()) else {
            thread::sleep(IDLE);
            continue;
        };
        if g != generation {
            generation = g;
            was = Default::default();
        }
        for user in 0..4u32 {
            let u = user as usize;
            if !connected[u] && last_check[u].is_some_and(|t| t.elapsed() < RECHECK_EMPTY) {
                continue;
            }
            let mut state = XINPUT_STATE::default();
            // SAFETY: plain XInput call with a valid out-pointer.
            let r = unsafe { XInputGetState(user, &mut state) };
            last_check[u] = Some(Instant::now());
            connected[u] = r == ERROR_SUCCESS;
            if !connected[u] {
                was[u].clear();
                continue;
            }
            let pad = state.Gamepad;
            let buttons = pad.wButtons.0;
            for i in edges(&list, &mut was[u], |c| {
                c.held(buttons, pad.bLeftTrigger, pad.bRightTrigger)
            }) {
                if events.send((g, list[i].0.clone())).is_err() {
                    return;
                }
            }
        }
        thread::sleep(POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chord_fires_once_per_press() {
        let chords = vec![
            ("a".to_string(), PadChord::parse("LB+RB").unwrap()),
            ("b".to_string(), PadChord::parse("LB+RB+A").unwrap()),
        ];
        let mut was = Vec::new();
        let press = |b: u16, was: &mut Vec<bool>| edges(&chords, was, |c| c.held(b, 0, 0));
        assert!(press(0x0100, &mut was).is_empty());
        assert_eq!(press(0x0300, &mut was), [0]);
        assert!(press(0x0300, &mut was).is_empty(), "held: no repeat");
        assert_eq!(press(0x1300, &mut was), [1]);
        assert!(press(0, &mut was).is_empty());
        assert_eq!(press(0x0300, &mut was), [0]);
    }

    #[test]
    fn registrations_replace_and_clear() {
        let service = PadService::start(Box::new(|_, _| {})).unwrap();
        let first = service.register(vec![("a".into(), PadChord::parse("LB+RB").unwrap())]);
        let second = service.register(vec![]);
        drop(first);
        assert!(
            service.chords.lock().unwrap().is_some(),
            "old drop leaves the new one"
        );
        drop(second);
        assert!(service.chords.lock().unwrap().is_none());
    }
}
