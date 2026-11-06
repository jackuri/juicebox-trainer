//! Freeze loop: rewrites chosen values on a timer so the game can't change them.
//!
//! Writes go through [`AttachedProcess`], so the guard applies. If the guard
//! blocks or the game exits, the freezer stops for good and keeps the error
//! for the UI. Freezing an address is logged once (address and value); the
//! repeated writes are not, to keep logs readable.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tracing::{info, warn};

use crate::error::Error;
use crate::session::AttachedProcess;
use crate::value::Value;

/// How often frozen values are rewritten by default.
pub const DEFAULT_FREEZE_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone)]
struct Entry {
    address: u64,
    bytes: Vec<u8>,
    error: Option<String>,
}

#[derive(Debug, Default)]
struct Shared {
    entries: Mutex<BTreeMap<u64, Entry>>,
    stop: AtomicBool,
    fatal: Mutex<Option<Error>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Background thread that keeps values frozen. Stops when dropped.
#[derive(Debug)]
pub struct Freezer {
    process: Arc<AttachedProcess>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Freezer {
    pub fn start(process: Arc<AttachedProcess>, interval: Duration) -> Self {
        let shared = Arc::new(Shared::default());
        let thread = {
            let process = Arc::clone(&process);
            let shared = Arc::clone(&shared);
            thread::Builder::new()
                .name("ot-freeze".into())
                .spawn(move || run(&process, &shared, interval))
                .expect("spawn freeze thread")
        };
        Self {
            process,
            shared,
            thread: Some(thread),
        }
    }

    /// Freezes `address` at `value` under the caller's `id` (replaces any
    /// previous freeze with that id).
    pub fn set(&self, id: u64, address: u64, value: &Value) {
        info!(
            pid = self.process.pid(),
            id,
            address = format_args!("{address:#X}"),
            value = %value,
            "freeze"
        );
        lock(&self.shared.entries).insert(
            id,
            Entry {
                address,
                bytes: value.to_bytes(self.process.arch()),
                error: None,
            },
        );
        if let Some(t) = &self.thread {
            t.thread().unpark();
        }
    }

    pub fn remove(&self, id: u64) {
        if lock(&self.shared.entries).remove(&id).is_some() {
            info!(pid = self.process.pid(), id, "unfreeze");
        }
    }

    pub fn clear(&self) {
        lock(&self.shared.entries).clear();
    }

    pub fn is_frozen(&self, id: u64) -> bool {
        lock(&self.shared.entries).contains_key(&id)
    }

    /// Last write error for this entry (e.g. memory freed), if any.
    pub fn entry_error(&self, id: u64) -> Option<String> {
        lock(&self.shared.entries)
            .get(&id)
            .and_then(|e| e.error.clone())
    }

    /// Why the freezer stopped (guard blocked or game exited), if it did.
    pub fn fatal(&self) -> Option<Error> {
        lock(&self.shared.fatal).clone()
    }

    pub fn is_running(&self) -> bool {
        !self.shared.stop.load(Ordering::SeqCst)
    }
}

impl Drop for Freezer {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            t.thread().unpark();
            let _ = t.join();
        }
    }
}

fn run(process: &AttachedProcess, shared: &Shared, interval: Duration) {
    while !shared.stop.load(Ordering::SeqCst) {
        let snapshot: Vec<(u64, u64, Vec<u8>)> = lock(&shared.entries)
            .iter()
            .map(|(id, e)| (*id, e.address, e.bytes.clone()))
            .collect();
        for (id, address, bytes) in snapshot {
            match process.write_bytes_quiet(address, &bytes) {
                Ok(()) => {
                    if let Some(e) = lock(&shared.entries).get_mut(&id) {
                        e.error = None;
                    }
                }
                Err(e @ (Error::Blocked(_) | Error::ProcessExited { .. })) => {
                    warn!(pid = process.pid(), "freezer stopped: {e}");
                    lock(&shared.entries).clear();
                    *lock(&shared.fatal) = Some(e);
                    shared.stop.store(true, Ordering::SeqCst);
                    return;
                }
                Err(e) => {
                    if let Some(entry) = lock(&shared.entries).get_mut(&id) {
                        entry.error = Some(e.to_string());
                    }
                }
            }
        }
        thread::park_timeout(interval);
    }
}
