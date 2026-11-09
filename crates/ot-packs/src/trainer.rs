//! Runs a loaded pack against an attached game.
//!
//! Every read and write goes through [`AttachedProcess`], so the anti-cheat
//! guard applies to all of it. Patches change code only through
//! [`AttachedProcess::apply_code`], which checks the original bytes before
//! each write and records the change (crash-safe when the session keeps a
//! journal). They are restored when turned off, when the trainer is unloaded
//! and when it is dropped. Value toggles and held values use a freeze thread.
//!
//! Addresses that start from a register (`capture`) are read with a short
//! hardware breakpoint (see [`ot_core::capture`]) when the player uses the
//! cheat, then remembered; held values re-read it every few seconds so they
//! follow the game (e.g. a new battle).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ot_core::freeze::DEFAULT_FREEZE_INTERVAL;
use ot_core::{AttachedProcess, Freezer, Module, Pattern, Value, ValueType};
use serde::Serialize;
use tracing::{info, warn};

use crate::hotkey::Hotkey;
use crate::numbers::{Num, add, in_bounds, to_value};
use crate::schema::{Address, Capture, Cheat, Impl, Operand, Pack, PatchImpl, Ui, parse_offset};
use crate::validate::{self, Severity};

/// Errors from running a pack.
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] ot_core::Error),

    #[error("this pack is for {expected}, but the attached game is {actual}")]
    WrongGame { expected: String, actual: String },

    #[error("this pack is for {expected} builds, but the game is {actual}")]
    WrongArch {
        expected: ot_core::Arch,
        actual: ot_core::Arch,
    },

    #[error("the pack has errors: {0}")]
    InvalidPack(String),

    #[error("no cheat '{0}' in this pack")]
    UnknownCheat(String),

    #[error("'{id}' is a {ui:?} cheat and can't do that")]
    WrongUi { id: String, ui: Ui },

    #[error("signature not found in {module}: {aob}")]
    SignatureNotFound { module: String, aob: String },

    #[error("signature matches more than once in {module}: {aob}")]
    SignatureAmbiguous { module: String, aob: String },

    /// The bytes at a patch site are neither the original nor the patch:
    /// usually a game update. Needs repair.
    #[error("needs repair: expected {expected} at {address:#X}, found {found}")]
    BytesChanged {
        address: u64,
        expected: String,
        found: String,
    },

    #[error("'{0}' is not available: {1}")]
    Unavailable(String, String),

    #[error("{value} is out of range for '{id}'")]
    OutOfRange { id: String, value: String },

    /// A captured address: the game hasn't run that code yet.
    #[error(
        "not found yet: do something in the game that uses '{0}' (for example start a battle), then try again"
    )]
    NotFoundYet(String),
}

impl Error {
    /// Short machine-readable kind for the UI.
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Core(e) => e.kind(),
            Error::WrongGame { .. } => "wrongGame",
            Error::WrongArch { .. } => "wrongArch",
            Error::InvalidPack(_) => "invalidPack",
            Error::UnknownCheat(_) => "unknownCheat",
            Error::WrongUi { .. } => "wrongUi",
            Error::SignatureNotFound { .. } => "signatureNotFound",
            Error::SignatureAmbiguous { .. } => "signatureAmbiguous",
            Error::BytesChanged { .. } => "needsRepair",
            Error::Unavailable(..) => "unavailable",
            Error::OutOfRange { .. } => "outOfRange",
            Error::NotFoundYet(_) => "notFoundYet",
        }
    }

    /// Errors that mean the session itself is gone.
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            Error::Core(ot_core::Error::Blocked(_) | ot_core::Error::ProcessExited { .. })
        )
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// How the running exe compares with the builds the pack lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum BuildMatch {
    /// Listed and marked verified.
    Verified { label: Option<String> },
    /// Listed but not marked verified.
    Listed { label: Option<String> },
    /// The pack lists builds, and this exe is not one of them.
    Unknown,
    /// The pack lists no builds.
    Untagged,
    /// The exe could not be hashed.
    Unreadable,
}

/// A located patch site.
#[derive(Debug, Clone)]
struct Site {
    address: u64,
    module: Module,
    original: Vec<u8>,
    replace: Vec<u8>,
    original_asm: Vec<String>,
    replace_asm: Vec<String>,
}

#[derive(Debug, Default)]
struct Slot {
    /// Toggle on, or a slider/number value being held.
    active: bool,
    /// Last problem (resolve failure, freeze error…).
    error: Option<String>,
    /// For patches: the resolved site, or why it couldn't be found.
    site: Option<Site>,
    /// Cached start point of an AOB address (code doesn't move).
    aob_base: Option<u64>,
    /// Value frozen while active, and where.
    frozen: Option<(u64, Value)>,
}

/// State of one cheat, for the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheatState {
    pub id: String,
    /// `ready`, `active` or `failed`.
    pub status: &'static str,
    pub active: bool,
    pub message: Option<String>,
    /// Current value for value cheats, as text.
    pub value: Option<String>,
    /// Resolved address, if known.
    pub address: Option<u64>,
    /// Canonical hotkey, e.g. `Ctrl+F1`.
    pub hotkey: Option<String>,
    /// What this cheat touches, in plain words (transparency).
    pub touches: Vec<String>,
}

/// A pack bound to an attached game.
#[derive(Debug)]
pub struct Trainer {
    process: Arc<AttachedProcess>,
    pack: Pack,
    build: BuildMatch,
    exe_sha256: Option<String>,
    slots: Vec<Slot>,
    freezer: Freezer,
    /// Captured register values by code address and register: value, when.
    captures: HashMap<(u64, String), (u64, Instant)>,
}

/// How fresh a captured address must be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Need {
    /// The player is using the cheat: read it again unless just read
    /// (waits for the game to run the code).
    Now,
    /// Keeping a held value on the right address: re-read now and then,
    /// briefly; keep the last one if the code didn't run.
    Refresh,
    /// Showing values: only what was read before.
    Cached,
}

/// Waits and ages for each [`Need`].
fn capture_policy(need: Need) -> (Duration, Duration) {
    match need {
        Need::Now => (Duration::from_secs(2), Duration::from_secs(6)),
        Need::Refresh => (Duration::from_secs(5), Duration::from_millis(400)),
        Need::Cached => (Duration::MAX, Duration::ZERO),
    }
}

impl Trainer {
    /// Binds `pack` to the attached game: checks the exe name, architecture
    /// and build hash, and locates every patch site. Nothing is written.
    pub fn load(process: Arc<AttachedProcess>, pack: Pack) -> Result<Self> {
        let errors: Vec<_> = validate::validate(&pack)
            .into_iter()
            .filter(|i| i.severity == Severity::Error)
            .map(|i| i.to_string())
            .collect();
        if !errors.is_empty() {
            return Err(Error::InvalidPack(errors.join("; ")));
        }
        let actual = process.info().name.clone();
        if !actual.eq_ignore_ascii_case(&pack.game.exe) {
            return Err(Error::WrongGame {
                expected: pack.game.exe.clone(),
                actual,
            });
        }
        if let Some(arch) = pack.game.arch
            && arch.core() != process.arch()
        {
            return Err(Error::WrongArch {
                expected: arch.core(),
                actual: process.arch(),
            });
        }
        let exe_sha256 = process.exe_sha256();
        let build = match &exe_sha256 {
            None => BuildMatch::Unreadable,
            Some(_) if pack.game.builds.is_empty() => BuildMatch::Untagged,
            Some(h) => match pack
                .game
                .builds
                .iter()
                .find(|b| b.sha256.eq_ignore_ascii_case(h))
            {
                Some(b) if b.verified => BuildMatch::Verified {
                    label: b.label.clone(),
                },
                Some(b) => BuildMatch::Listed {
                    label: b.label.clone(),
                },
                None => BuildMatch::Unknown,
            },
        };
        let freezer = Freezer::start(Arc::clone(&process), DEFAULT_FREEZE_INTERVAL);
        let mut trainer = Self {
            slots: pack.cheats.iter().map(|_| Slot::default()).collect(),
            process,
            pack,
            build,
            exe_sha256,
            freezer,
            captures: HashMap::new(),
        };
        for i in 0..trainer.slots.len() {
            if let Impl::Patch(p) = &trainer.pack.cheats[i].implementation {
                let p = p.clone();
                match trainer.locate_patch(&p) {
                    Ok((site, already_patched)) => {
                        let slot = &mut trainer.slots[i];
                        slot.site = Some(site);
                        slot.active = already_patched;
                    }
                    Err(e) if e.is_fatal() => return Err(e),
                    Err(e) => trainer.slots[i].error = Some(e.to_string()),
                }
            }
        }
        info!(
            pid = trainer.process.pid(),
            pack = %trainer.pack.game.id,
            build = ?trainer.build,
            "pack loaded"
        );
        Ok(trainer)
    }

    pub fn pack(&self) -> &Pack {
        &self.pack
    }

    pub fn build(&self) -> &BuildMatch {
        &self.build
    }

    pub fn exe_sha256(&self) -> Option<&str> {
        self.exe_sha256.as_deref()
    }

    pub fn process(&self) -> &Arc<AttachedProcess> {
        &self.process
    }

    fn index(&self, id: &str) -> Result<usize> {
        self.pack
            .cheats
            .iter()
            .position(|c| c.id == id)
            .ok_or_else(|| Error::UnknownCheat(id.to_string()))
    }

    fn module_named(&self, name: Option<&str>) -> Result<Module> {
        Ok(self.process.module(name.unwrap_or(&self.pack.game.exe))?)
    }

    fn find_unique(&self, module: &Module, aob: &str) -> Result<u64> {
        let pattern = Pattern::parse(aob)?;
        let hits = self.process.aob_scan(module, &pattern, 2)?;
        match hits.as_slice() {
            [one] => Ok(*one),
            [] => Err(Error::SignatureNotFound {
                module: module.name.clone(),
                aob: pattern.to_string(),
            }),
            _ => Err(Error::SignatureAmbiguous {
                module: module.name.clone(),
                aob: pattern.to_string(),
            }),
        }
    }

    /// Finds a patch site and checks it holds the original (or patched) bytes.
    fn locate_patch(&self, p: &PatchImpl) -> Result<(Site, bool)> {
        let module = self.module_named(p.module.as_deref())?;
        let hit = self.find_unique(&module, &p.aob)?;
        let address = hit
            .checked_add_signed(p.offset)
            .ok_or_else(|| ot_core::Error::InvalidAddress(format!("{hit:#X}{:+}", p.offset)))?;
        let original = Pattern::parse_exact(&p.original)?;
        let replace = Pattern::parse_exact(&p.replace)?;
        if !module.contains(address) || !module.contains(address + original.len() as u64 - 1) {
            return Err(Error::Unavailable(
                p.aob.clone(),
                "the patch site is outside the module".into(),
            ));
        }
        let arch = self.process.arch();
        let replace_asm =
            ot_core::code::check_patch(&replace, arch, address, module.base..module.end())?
                .into_iter()
                .map(|i| i.text)
                .collect();
        let original_asm = ot_core::code::disassemble(&original, arch, address)
            .into_iter()
            .map(|i| i.text)
            .collect();
        let found = self.process.read_bytes(address, original.len())?;
        let patched = if found == original {
            false
        } else if found == replace {
            // Left patched (e.g. by an earlier run): record it so unloading
            // or detaching restores it. Nothing is written.
            self.process.apply_code(address, &original, &replace)?;
            true
        } else {
            return Err(Error::BytesChanged {
                address,
                expected: ot_core::aob::hex_string(&original),
                found: ot_core::aob::hex_string(&found),
            });
        };
        Ok((
            Site {
                address,
                module,
                original,
                replace,
                original_asm,
                replace_asm,
            },
            patched,
        ))
    }

    fn value_impl(&self, i: usize) -> Option<(ValueType, Address)> {
        match &self.pack.cheats[i].implementation {
            Impl::Value(v) => Some((v.ty, v.address.clone())),
            Impl::Patch(_) => None,
        }
    }

    /// Resolves a value cheat's address now (pointer chains can move).
    fn resolve_value(&mut self, i: usize) -> Result<(u64, ValueType)> {
        self.resolve_value_for(i, Need::Now)
    }

    fn resolve_value_for(&mut self, i: usize, need: Need) -> Result<(u64, ValueType)> {
        let (ty, a) = self.value_impl(i).ok_or_else(|| Error::WrongUi {
            id: self.pack.cheats[i].id.clone(),
            ui: self.pack.cheats[i].ui,
        })?;
        let start = match (&a.base, &a.aob, &a.capture) {
            (None, Some(aob), Some(cap)) => self.captured_start(i, &a, aob, cap, need)?,
            (Some(base), _, _) => self.process.resolve(base)?,
            (None, Some(aob), None) => match self.slots[i].aob_base {
                Some(b) => b,
                None => {
                    let b = self.aob_start(&a, aob)?;
                    self.slots[i].aob_base = Some(b);
                    b
                }
            },
            (None, None, _) => unreachable!("validated: base or aob"),
        };
        let offsets: Vec<i64> = a
            .offsets
            .iter()
            .map(|o| parse_offset(o).expect("validated offset"))
            .collect();
        Ok((self.process.resolve_chain(start, &offsets)?, ty))
    }

    /// The start point of a `capture` address: the register's value when the
    /// game runs the code at the found address, plus `add`.
    fn captured_start(
        &mut self,
        i: usize,
        a: &Address,
        aob: &str,
        cap: &Capture,
        need: Need,
    ) -> Result<u64> {
        let site = match self.slots[i].aob_base {
            Some(b) => b,
            None => {
                let b = self.aob_start(a, aob)?;
                self.slots[i].aob_base = Some(b);
                b
            }
        };
        let arch = self.process.arch();
        let bad =
            || Error::InvalidPack(format!("'{}' isn't a register of this game", cap.register));
        let register = ot_core::capture::Register::parse(&cap.register, arch).ok_or_else(bad)?;
        let when = match &cap.when {
            Some(w) => Some((
                ot_core::capture::Register::parse(&w.register, arch).ok_or_else(bad)?,
                w.equals,
            )),
            None => None,
        };
        let add = cap
            .add
            .as_deref()
            .map_or(0, |o| parse_offset(o).expect("validated offset"));
        let key = (
            site,
            format!("{}|{:?}", register.name(), when.map(|(r, v)| (r.name(), v))),
        );
        let (max_age, wait) = capture_policy(need);
        let known = self.captures.get(&key).copied();
        let start = |v: u64| v.wrapping_add_signed(add);
        if let Some((v, at)) = known
            && at.elapsed() <= max_age
        {
            return Ok(start(v));
        }
        let name = self.cheat(i).name.clone();
        if wait.is_zero() {
            return known.map(|(v, _)| start(v)).ok_or(Error::NotFoundYet(name));
        }
        let spec = ot_core::capture::CaptureSpec {
            site,
            register,
            when,
        };
        match ot_core::capture::capture(&self.process, spec, wait)? {
            Some(v) => {
                self.captures.insert(key, (v, Instant::now()));
                Ok(start(v))
            }
            // The code didn't run in time: keep the last one, like a table's
            // hook keeps its pointer until the code runs again.
            None => known.map(|(v, _)| start(v)).ok_or(Error::NotFoundYet(name)),
        }
    }

    fn aob_start(&self, a: &Address, aob: &str) -> Result<u64> {
        let module = self.module_named(a.module.as_deref())?;
        let hit = self.find_unique(&module, aob)?;
        let offset = a.offset.unwrap_or(0);
        let at = hit
            .checked_add_signed(offset)
            .ok_or_else(|| ot_core::Error::InvalidAddress(format!("{hit:#X}{offset:+}")))?;
        Ok(match a.operand {
            None => at,
            Some(Operand::Abs32) => u64::from(u32::from_le_bytes(self.read4(at)?)),
            Some(Operand::Rip32) => {
                let disp = i64::from(i32::from_le_bytes(self.read4(at)?));
                let next = hit.wrapping_add_signed(a.next.expect("validated: rip32 has next"));
                next.checked_add_signed(disp)
                    .ok_or_else(|| ot_core::Error::InvalidAddress(format!("{next:#X}{disp:+}")))?
            }
        })
    }

    fn read4(&self, at: u64) -> Result<[u8; 4]> {
        let b = self.process.read_bytes(at, 4)?;
        Ok([b[0], b[1], b[2], b[3]])
    }

    fn cheat(&self, i: usize) -> &Cheat {
        &self.pack.cheats[i]
    }

    fn wrong_ui(&self, i: usize) -> Error {
        Error::WrongUi {
            id: self.cheat(i).id.clone(),
            ui: self.cheat(i).ui,
        }
    }

    /// Records a non-fatal error on the slot and passes every error on.
    fn note<T>(&mut self, i: usize, r: Result<T>) -> Result<T> {
        match &r {
            Ok(_) => self.slots[i].error = None,
            Err(e) => {
                warn!(cheat = %self.pack.cheats[i].id, "{e}");
                self.slots[i].error = Some(e.to_string());
            }
        }
        r
    }

    /// Turns a toggle on or off.
    pub fn set_active(&mut self, id: &str, on: bool) -> Result<()> {
        let i = self.index(id)?;
        if self.cheat(i).ui != Ui::Toggle {
            return Err(self.wrong_ui(i));
        }
        let r = match self.cheat(i).implementation {
            Impl::Patch(_) => self.set_patch(i, on),
            Impl::Value(_) => self.set_value_toggle(i, on),
        };
        self.note(i, r)
    }

    fn set_patch(&mut self, i: usize, on: bool) -> Result<()> {
        let site = self.slots[i].site.clone().ok_or_else(|| {
            Error::Unavailable(
                self.cheat(i).id.clone(),
                self.slots[i]
                    .error
                    .clone()
                    .unwrap_or_else(|| "patch site not found".into()),
            )
        })?;
        // Both check the bytes first and refuse on a mismatch.
        let wrote = if on {
            self.process
                .apply_code(site.address, &site.original, &site.replace)?
        } else {
            self.process
                .revert_code(site.address, &site.original, &site.replace)?
        };
        self.slots[i].active = on;
        if wrote {
            info!(cheat = %self.cheat(i).id, on, "patch");
        }
        Ok(())
    }

    fn set_value_toggle(&mut self, i: usize, on: bool) -> Result<()> {
        if !on {
            self.unfreeze(i);
            return Ok(());
        }
        let (address, ty) = self.resolve_value(i)?;
        let value = match self.cheat(i).toggle.as_ref().and_then(|t| t.value.as_ref()) {
            Some(n) => to_value(ty, Num::from_json(n)).expect("validated value"),
            None => self.process.read_value(address, ty)?,
        };
        self.process.write_value(address, &value)?;
        self.freeze(i, address, value);
        Ok(())
    }

    fn freeze(&mut self, i: usize, address: u64, value: Value) {
        self.freezer.set(i as u64, address, &value);
        let slot = &mut self.slots[i];
        slot.frozen = Some((address, value));
        slot.active = true;
    }

    fn unfreeze(&mut self, i: usize) {
        self.freezer.remove(i as u64);
        let slot = &mut self.slots[i];
        slot.frozen = None;
        slot.active = false;
    }

    /// Sets a slider or number cheat from text (parsed for its value type).
    pub fn set_value(&mut self, id: &str, text: &str) -> Result<()> {
        let i = self.index(id)?;
        let c = self.cheat(i);
        let (min, max, hold) = match (c.ui, &c.slider, &c.number) {
            (Ui::Slider, Some(s), _) => (
                Some(Num::from_json(&s.min)),
                Some(Num::from_json(&s.max)),
                s.hold,
            ),
            (Ui::Number, _, n) => (
                n.as_ref().and_then(|n| n.min.as_ref()).map(Num::from_json),
                n.as_ref().and_then(|n| n.max.as_ref()).map(Num::from_json),
                n.as_ref().is_some_and(|n| n.hold),
            ),
            _ => return Err(self.wrong_ui(i)),
        };
        let r = (|| {
            let (address, ty) = self.resolve_value(i)?;
            let value = ty.parse(text, self.process.arch())?;
            if !in_bounds(Num::from_value(&value), min, max) {
                return Err(Error::OutOfRange {
                    id: id.to_string(),
                    value: text.trim().to_string(),
                });
            }
            self.process.write_value(address, &value)?;
            if hold {
                self.freeze(i, address, value);
            }
            Ok(())
        })();
        self.note(i, r)
    }

    /// Stops holding a slider/number value (the game can change it again).
    pub fn release(&mut self, id: &str) -> Result<()> {
        let i = self.index(id)?;
        self.unfreeze(i);
        Ok(())
    }

    /// Runs a button: adds to or sets its value once.
    pub fn press(&mut self, id: &str) -> Result<()> {
        let i = self.index(id)?;
        let Some(b) = self.cheat(i).button.clone() else {
            return Err(self.wrong_ui(i));
        };
        let r = (|| {
            let (address, ty) = self.resolve_value(i)?;
            let value = match (&b.set, &b.add) {
                (Some(n), _) => to_value(ty, Num::from_json(n)).expect("validated value"),
                (None, Some(n)) => {
                    let current = self.process.read_value(address, ty)?;
                    add(&current, Num::from_json(n)).ok_or_else(|| Error::OutOfRange {
                        id: id.to_string(),
                        value: n.to_string(),
                    })?
                }
                (None, None) => unreachable!("validated: add or set"),
            };
            self.process.write_value(address, &value)?;
            // A frozen value would undo the button; follow it.
            if let Some((a, _)) = self.slots[i].frozen {
                self.freeze(i, a, value);
            }
            Ok(())
        })();
        self.note(i, r)
    }

    /// What a hotkey does: flips a toggle or presses a button.
    pub fn hotkey(&mut self, id: &str) -> Result<()> {
        let i = self.index(id)?;
        match self.cheat(i).ui {
            Ui::Toggle => {
                let on = !self.slots[i].active;
                self.set_active(id, on)
            }
            Ui::Button => self.press(id),
            _ => Err(self.wrong_ui(i)),
        }
    }

    /// Hotkeys to register: (cheat id, parsed hotkey).
    pub fn hotkeys(&self) -> Vec<(String, Hotkey)> {
        self.pack
            .cheats
            .iter()
            .filter_map(|c| {
                let hk = Hotkey::parse(c.hotkey.as_deref()?).ok()?;
                Some((c.id.clone(), hk))
            })
            .collect()
    }

    /// Re-resolves frozen pointer chains (objects can move, e.g. on a new
    /// level) and picks up freeze errors. Call this regularly. Fails only if
    /// the session is gone.
    pub fn refresh(&mut self) -> Result<()> {
        if let Some(fatal) = self.freezer.fatal() {
            return Err(fatal.into());
        }
        self.sync_patches()?;
        for i in 0..self.slots.len() {
            let Some((old, value)) = self.slots[i].frozen else {
                continue;
            };
            match self.resolve_value_for(i, Need::Refresh) {
                Ok((address, _)) => {
                    if address != old {
                        info!(cheat = %self.cheat(i).id, "frozen value moved to {address:#X}");
                        self.freeze(i, address, value);
                    }
                    self.slots[i].error = self.freezer.entry_error(i as u64);
                }
                Err(e) if e.is_fatal() => return Err(e),
                Err(e) => self.slots[i].error = Some(e.to_string()),
            }
        }
        Ok(())
    }

    /// Reads every patch site so the on/off state shown is what is really in
    /// the game (e.g. after a crash handler restored it).
    fn sync_patches(&mut self) -> Result<()> {
        for i in 0..self.slots.len() {
            let Some(site) = self.slots[i].site.clone() else {
                continue;
            };
            let found = match self.process.read_bytes(site.address, site.original.len()) {
                Ok(b) => b,
                Err(e @ (ot_core::Error::Blocked(_) | ot_core::Error::ProcessExited { .. })) => {
                    return Err(e.into());
                }
                Err(e) => {
                    self.slots[i].error = Some(e.to_string());
                    continue;
                }
            };
            if found == site.replace {
                self.slots[i].active = true;
            } else if found == site.original {
                self.slots[i].active = false;
            } else {
                self.slots[i].active = false;
                self.slots[i].error = Some(
                    Error::BytesChanged {
                        address: site.address,
                        expected: ot_core::aob::hex_string(&site.original),
                        found: ot_core::aob::hex_string(&found),
                    }
                    .to_string(),
                );
            }
        }
        Ok(())
    }

    /// Current state of every cheat, in pack order.
    pub fn states(&mut self) -> Result<Vec<CheatState>> {
        let mut out = Vec::with_capacity(self.slots.len());
        for i in 0..self.slots.len() {
            out.push(self.state(i)?);
        }
        Ok(out)
    }

    fn state(&mut self, i: usize) -> Result<CheatState> {
        let c = self.cheat(i).clone();
        let mut touches = Vec::new();
        let (mut value, mut address, mut resolve_error) = (None, None, None);
        let mut waiting = false;
        match &c.implementation {
            Impl::Patch(p) => match &self.slots[i].site {
                Some(s) => {
                    touches.push(format!(
                        "{}+{:#X}: {} → {}",
                        s.module.name,
                        s.address - s.module.base,
                        ot_core::aob::hex_string(&s.original),
                        ot_core::aob::hex_string(&s.replace)
                    ));
                    touches.push(format!(
                        "{} → {}",
                        s.original_asm.join("; "),
                        s.replace_asm.join("; ")
                    ));
                    address = Some(s.address);
                }
                None => touches.push(format!("patch at signature {}", p.aob)),
            },
            Impl::Value(v) => {
                touches.push(format!("{} at {}", v.ty, describe_address(&v.address)));
                match self.resolve_value_for(i, Need::Cached) {
                    Ok((a, ty)) => {
                        address = Some(a);
                        match self.process.read_value(a, ty) {
                            Ok(v) => value = Some(v.to_string()),
                            Err(
                                e @ (ot_core::Error::Blocked(_)
                                | ot_core::Error::ProcessExited { .. }),
                            ) => {
                                return Err(e.into());
                            }
                            Err(e) => resolve_error = Some(e.to_string()),
                        }
                    }
                    Err(e) if e.is_fatal() => return Err(e),
                    Err(e @ Error::NotFoundYet(_)) => {
                        waiting = true;
                        resolve_error = Some(e.to_string());
                    }
                    Err(e) => resolve_error = Some(e.to_string()),
                }
            }
        }
        let slot = &self.slots[i];
        let message = slot.error.clone().or(resolve_error);
        let failed = match &c.implementation {
            Impl::Patch(_) => slot.site.is_none(),
            // Waiting for the game to run captured code isn't a failure.
            Impl::Value(_) => address.is_none() && !slot.active && !waiting,
        };
        let status = if slot.active {
            "active"
        } else if failed {
            "failed"
        } else {
            "ready"
        };
        Ok(CheatState {
            id: c.id.clone(),
            status,
            active: slot.active,
            message,
            value,
            address,
            hotkey: c
                .hotkey
                .as_deref()
                .and_then(|h| Hotkey::parse(h).ok())
                .map(|h| h.to_string()),
            touches,
        })
    }

    /// Turns everything off: restores patches, stops freezes. Returns the
    /// first error but keeps going.
    pub fn restore_all(&mut self) -> Result<()> {
        self.freezer.clear();
        let mut first = None;
        for i in 0..self.slots.len() {
            self.slots[i].frozen = None;
            if matches!(self.cheat(i).implementation, Impl::Patch(_)) && self.slots[i].active {
                if let Err(e) = self.set_patch(i, false) {
                    warn!(cheat = %self.cheat(i).id, "restore failed: {e}");
                    first.get_or_insert(e);
                }
            } else {
                self.slots[i].active = false;
            }
        }
        first.map_or(Ok(()), Err)
    }
}

impl Drop for Trainer {
    fn drop(&mut self) {
        if self.process.is_alive() {
            let _ = self.restore_all();
        }
        info!(pack = %self.pack.game.id, "pack unloaded");
    }
}

/// Plain description of an address, e.g. `[[aob 0F 1F …]+0x10]+0xC`.
pub fn describe_address(a: &Address) -> String {
    let mut s = match (&a.base, &a.aob) {
        (Some(b), _) => b.clone(),
        (None, Some(aob)) => {
            let module = a.module.as_deref().unwrap_or("exe");
            let off = match a.offset.unwrap_or(0) {
                0 => String::new(),
                o if o < 0 => format!("-{:#X}", o.unsigned_abs()),
                o => format!("+{o:#X}"),
            };
            match (a.operand, &a.capture) {
                (_, Some(c)) => {
                    let when = c
                        .when
                        .as_ref()
                        .map(|w| format!(" when {} = {}", w.register, w.equals))
                        .unwrap_or_default();
                    let add = c
                        .add
                        .as_deref()
                        .map(|o| format!("+{o}"))
                        .unwrap_or_default();
                    format!("{}{when} at aob({module}: {aob}){off}{add}", c.register)
                }
                (None, None) => format!("aob({module}: {aob}){off}"),
                (Some(Operand::Abs32), None) => format!("abs32 at aob({module}: {aob}){off}"),
                (Some(Operand::Rip32), None) => format!("rip32 at aob({module}: {aob}){off}"),
            }
        }
        (None, None) => "?".into(),
    };
    for o in &a.offsets {
        s = format!("[{s}]+{o}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_addresses() {
        let a = Address {
            base: Some("game.exe+0x10".into()),
            aob: None,
            module: None,
            offset: None,
            operand: None,
            next: None,
            offsets: vec!["0x18".into(), "0x40".into()],
            capture: None,
        };
        assert_eq!(describe_address(&a), "[[game.exe+0x10]+0x18]+0x40");
        let b = Address {
            base: None,
            aob: Some("48 8B 0D ?? ?? ?? ??".into()),
            offset: Some(10),
            operand: Some(Operand::Rip32),
            next: Some(14),
            offsets: vec![],
            module: None,
            capture: None,
        };
        assert_eq!(
            describe_address(&b),
            "rip32 at aob(exe: 48 8B 0D ?? ?? ?? ??)+0xA"
        );
        let c = Address {
            operand: None,
            next: None,
            offset: Some(7),
            aob: Some("0F 1F 80 54 4F 54 01 8B 01".into()),
            offsets: vec!["0xC".into()],
            capture: Some(Capture {
                register: "rcx".into(),
                add: Some("0x10".into()),
                when: Some(crate::schema::When {
                    register: "edx".into(),
                    equals: 7,
                }),
            }),
            ..b
        };
        assert_eq!(
            describe_address(&c),
            "[rcx when edx = 7 at aob(exe: 0F 1F 80 54 4F 54 01 8B 01)+0x7+0x10]+0xC"
        );
    }

    #[test]
    fn fatal_errors() {
        assert!(Error::Core(ot_core::Error::ProcessExited { pid: 1 }).is_fatal());
        assert!(!Error::UnknownCheat("x".into()).is_fatal());
        assert_eq!(
            Error::BytesChanged {
                address: 0,
                expected: String::new(),
                found: String::new()
            }
            .kind(),
            "needsRepair"
        );
    }
}
