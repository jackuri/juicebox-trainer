//! Reading a register when a line of game code runs (a "capture").
//!
//! Many Cheat Engine tables find the player's data by hooking an instruction
//! and copying a register into a variable. JuiceBox Trainer gets the same value
//! without changing any game code: it attaches as a debugger (it shows up as
//! one; it does not hide), sets a hardware *execute* breakpoint on the
//! instruction in every thread's debug registers, reads the register the
//! first time the game runs it (optionally only when another register holds a
//! given value), then clears the breakpoints and detaches. The game keeps
//! running; nothing in its code or memory is changed.
//!
//! Only for processes that passed the anti-cheat guard (it takes an
//! [`AttachedProcess`]). Single-player games only, like everything else.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::Debug::{
    CONTEXT_CONTROL_AMD64, CONTEXT_DEBUG_REGISTERS_AMD64, CONTEXT_INTEGER_AMD64,
    CREATE_PROCESS_DEBUG_EVENT, CREATE_THREAD_DEBUG_EVENT, ContinueDebugEvent, DEBUG_EVENT,
    DebugActiveProcess, DebugActiveProcessStop, DebugSetProcessKillOnExit, EXCEPTION_DEBUG_EVENT,
    EXIT_PROCESS_DEBUG_EVENT, GetThreadContext, LOAD_DLL_DEBUG_EVENT, SetThreadContext,
    WOW64_CONTEXT, WOW64_CONTEXT_CONTROL, WOW64_CONTEXT_DEBUG_REGISTERS, WOW64_CONTEXT_INTEGER,
    WaitForDebugEvent, Wow64GetThreadContext, Wow64SetThreadContext,
};

use crate::error::{Error, Result};
use crate::process::Arch;
use crate::session::AttachedProcess;
use crate::win::OwnedHandle;
use crate::writes::{
    Ctx64, DBG_CONTINUE, DBG_EXCEPTION_NOT_HANDLED, EXCEPTION_BREAKPOINT, EXCEPTION_SINGLE_STEP,
    STATUS_WX86_BREAKPOINT, STATUS_WX86_SINGLE_STEP, Wow, open_thread, set_slot0, threads_of,
};

/// A capture waits at most this long for the game to run the instruction.
pub const MAX_WAIT: Duration = Duration::from_secs(30);

/// `DR7` for breakpoint 0: enabled (L0), break on execute, length 1.
const DR7_EXECUTE: u64 = 1;
/// EFLAGS resume flag: run the instruction once without breaking again.
const RF: u32 = 1 << 16;

/// A general-purpose register, read whole (`rdx`) or its low 32 bits (`edx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Register {
    /// 0 rax, 1 rcx, 2 rdx, 3 rbx, 4 rsp, 5 rbp, 6 rsi, 7 rdi, 8-15 r8-r15.
    index: u8,
    wide: bool,
}

const NAMES64: [&str; 16] = [
    "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13",
    "r14", "r15",
];
const NAMES32: [&str; 16] = [
    "eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi", "r8d", "r9d", "r10d", "r11d", "r12d",
    "r13d", "r14d", "r15d",
];

impl Register {
    /// A register name for a game of this architecture: 64-bit games can use
    /// `rax`…`r15` and their low halves (`eax`, `r8d`); 32-bit games `eax`…`edi`.
    pub fn parse(name: &str, arch: Arch) -> Option<Self> {
        let n = name.trim().to_ascii_lowercase();
        let (index, wide) = if let Some(i) = NAMES64.iter().position(|x| *x == n) {
            (i, true)
        } else {
            (NAMES32.iter().position(|x| *x == n)?, false)
        };
        match arch {
            Arch::X64 => {}
            Arch::X86 if !wide && index < 8 => {}
            _ => return None,
        }
        Some(Self {
            index: index as u8,
            wide,
        })
    }

    pub fn name(self) -> &'static str {
        if self.wide {
            NAMES64[self.index as usize]
        } else {
            NAMES32[self.index as usize]
        }
    }

    fn read64(self, c: &windows::Win32::System::Diagnostics::Debug::CONTEXT) -> u64 {
        let v = match self.index {
            0 => c.Rax,
            1 => c.Rcx,
            2 => c.Rdx,
            3 => c.Rbx,
            4 => c.Rsp,
            5 => c.Rbp,
            6 => c.Rsi,
            7 => c.Rdi,
            8 => c.R8,
            9 => c.R9,
            10 => c.R10,
            11 => c.R11,
            12 => c.R12,
            13 => c.R13,
            14 => c.R14,
            _ => c.R15,
        };
        if self.wide { v } else { v & 0xFFFF_FFFF }
    }

    fn read32(self, c: &WOW64_CONTEXT) -> u64 {
        u64::from(match self.index {
            0 => c.Eax,
            1 => c.Ecx,
            2 => c.Edx,
            3 => c.Ebx,
            4 => c.Esp,
            5 => c.Ebp,
            6 => c.Esi,
            _ => c.Edi,
        })
    }

    fn mask(self) -> u64 {
        if self.wide { u64::MAX } else { 0xFFFF_FFFF }
    }
}

/// What to read, and where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureSpec {
    /// The instruction: the register is read just before it runs.
    pub site: u64,
    pub register: Register,
    /// Only when this register holds this value (e.g. `ebx` is 0).
    pub when: Option<(Register, i64)>,
}

/// Reads `spec.register` the first time the game runs the instruction at
/// `spec.site` (with `when` true), waiting at most `wait`. `Ok(None)` if the
/// game didn't run it in time (e.g. the player isn't in a battle).
pub fn capture(
    process: &AttachedProcess,
    spec: CaptureSpec,
    wait: Duration,
) -> Result<Option<u64>> {
    process.recheck_guard()?;
    let wow = match process.arch() {
        Arch::X64 => false,
        Arch::X86 => true,
        other => {
            return Err(Error::Debugger(format!(
                "can't read registers of a {other} process"
            )));
        }
    };
    let pid = process.pid();
    let wait = wait.min(MAX_WAIT);
    // The Win32 debug API ties a debuggee to the thread that attached.
    std::thread::Builder::new()
        .name("capture".into())
        .spawn(move || debug_loop(pid, wow, spec, wait))
        .map_err(|e| Error::Debugger(e.to_string()))?
        .join()
        .map_err(|_| Error::Debugger("the debugger thread failed".into()))?
}

/// At a breakpoint stop: is it ours (instruction pointer at the site)? Then
/// read the registers, set the resume flag so the instruction runs once
/// without stopping again, and clear DR6. Returns the register values asked
/// for, or `None` if this stop isn't ours.
fn at_site(thread: HANDLE, wow: bool, spec: &CaptureSpec) -> Option<(u64, Option<u64>)> {
    if wow {
        // SAFETY: zeroed is a valid WOW64_CONTEXT.
        let mut c: WOW64_CONTEXT = unsafe { std::mem::zeroed() };
        c.ContextFlags =
            WOW64_CONTEXT_INTEGER | WOW64_CONTEXT_CONTROL | WOW64_CONTEXT_DEBUG_REGISTERS;
        // SAFETY: the thread is stopped at a debug event.
        unsafe { Wow64GetThreadContext(thread, &mut c) }.ok()?;
        if u64::from(c.Eip) != spec.site || c.Dr6 & 1 == 0 {
            return None;
        }
        let value = spec.register.read32(&c);
        let cond = spec.when.map(|(r, _)| r.read32(&c));
        c.EFlags |= RF;
        c.Dr6 = 0;
        // SAFETY: as above.
        let _ = unsafe { Wow64SetThreadContext(thread, &c) };
        Some((value, cond))
    } else {
        // SAFETY: zeroed is a valid CONTEXT; it is 16-byte aligned.
        let mut c = Ctx64(unsafe { std::mem::zeroed() });
        c.0.ContextFlags =
            CONTEXT_INTEGER_AMD64 | CONTEXT_CONTROL_AMD64 | CONTEXT_DEBUG_REGISTERS_AMD64;
        // SAFETY: the thread is stopped at a debug event; `c` is aligned.
        unsafe { GetThreadContext(thread, &mut c.0) }.ok()?;
        if c.0.Rip != spec.site || c.0.Dr6 & 1 == 0 {
            return None;
        }
        let value = spec.register.read64(&c.0);
        let cond = spec.when.map(|(r, _)| r.read64(&c.0));
        c.0.EFlags |= RF;
        c.0.Dr6 = 0;
        // SAFETY: as above.
        let _ = unsafe { SetThreadContext(thread, &c.0) };
        Some((value, cond))
    }
}

fn debug_loop(pid: u32, wow: bool, spec: CaptureSpec, wait: Duration) -> Result<Option<u64>> {
    let mode = if wow { Wow::Yes } else { Wow::No };
    // SAFETY: plain Win32 call on a pid we are allowed to open.
    if let Err(e) = unsafe { DebugActiveProcess(pid) } {
        return Err(Error::Debugger(format!(
            "could not attach as a debugger ({e}); another debugger may already be attached"
        )));
    }
    // Detaching must never kill the game.
    // SAFETY: affects only this thread's debuggees.
    let _ = unsafe { DebugSetProcessKillOnExit(false) };

    let mut threads: BTreeMap<u32, OwnedHandle> = BTreeMap::new();
    let target = Some((spec.site, DR7_EXECUTE));
    let mut armed = false;
    let mut found = None;
    let deadline = Instant::now() + wait;
    let mut stop_at: Option<Instant> = None;
    let mut exited = false;
    let mut event = DEBUG_EVENT::default();
    loop {
        let stopping = found.is_some() || Instant::now() > deadline;
        if stopping && armed {
            for h in threads.values() {
                set_slot0(h.raw(), &mode, None);
            }
            armed = false;
            stop_at = Some(Instant::now());
        }
        // SAFETY: `event` is a valid out-buffer.
        let got = unsafe { WaitForDebugEvent(&mut event, if stopping { 20 } else { 50 }) }.is_ok();
        if !got {
            // Done once breakpoints are cleared and no stops are in flight.
            if stopping && stop_at.is_none_or(|t| t.elapsed() > Duration::from_millis(100)) {
                break;
            }
            continue;
        }
        let mut status = DBG_CONTINUE;
        match event.dwDebugEventCode {
            CREATE_PROCESS_DEBUG_EVENT => {
                // SAFETY: the union member matches the event code.
                let info = unsafe { event.u.CreateProcessInfo };
                if !info.hFile.is_invalid() {
                    // SAFETY: we own this file handle per the debug API.
                    let _ = unsafe { CloseHandle(info.hFile) };
                }
            }
            LOAD_DLL_DEBUG_EVENT => {
                // SAFETY: the union member matches the event code.
                let info = unsafe { event.u.LoadDll };
                if !info.hFile.is_invalid() {
                    // SAFETY: as above.
                    let _ = unsafe { CloseHandle(info.hFile) };
                }
            }
            CREATE_THREAD_DEBUG_EVENT => {
                if armed && let Some(h) = open_thread(event.dwThreadId) {
                    set_slot0(h.raw(), &mode, target);
                    threads.insert(event.dwThreadId, h);
                }
            }
            EXIT_PROCESS_DEBUG_EVENT => exited = true,
            EXCEPTION_DEBUG_EVENT => {
                // SAFETY: the union member matches the event code.
                let code = unsafe { event.u.Exception.ExceptionRecord.ExceptionCode }.0;
                if code == EXCEPTION_BREAKPOINT || code == STATUS_WX86_BREAKPOINT {
                    // The attach breakpoint: arm now that the game is stopped.
                    if !armed && !stopping && stop_at.is_none() {
                        for tid in threads_of(pid).unwrap_or_default() {
                            if let Some(h) = open_thread(tid) {
                                set_slot0(h.raw(), &mode, target);
                                threads.insert(tid, h);
                            }
                        }
                        armed = true;
                    }
                } else if code == EXCEPTION_SINGLE_STEP || code == STATUS_WX86_SINGLE_STEP {
                    let handle = threads.get(&event.dwThreadId).map(|h| h.raw()).or_else(|| {
                        open_thread(event.dwThreadId).map(|h| {
                            let raw = h.raw();
                            threads.insert(event.dwThreadId, h);
                            raw
                        })
                    });
                    match handle.and_then(|h| at_site(h, wow, &spec)) {
                        // Ours: the resume flag is set, the game goes on.
                        Some((value, cond)) => {
                            let wanted = match (spec.when, cond) {
                                (Some((r, want)), Some(have)) => have == (want as u64) & r.mask(),
                                _ => true,
                            };
                            if wanted && found.is_none() {
                                found = Some(value);
                            }
                        }
                        // Not our breakpoint (e.g. the game single-steps itself).
                        None => status = DBG_EXCEPTION_NOT_HANDLED,
                    }
                } else {
                    status = DBG_EXCEPTION_NOT_HANDLED;
                }
            }
            _ => {}
        }
        // SAFETY: continues the event we just received.
        let _ = unsafe { ContinueDebugEvent(event.dwProcessId, event.dwThreadId, status) };
        if exited {
            break;
        }
    }
    if armed {
        for h in threads.values() {
            set_slot0(h.raw(), &mode, None);
        }
    }
    // SAFETY: detaches this thread's debugger from the game; the game keeps running.
    let _ = unsafe { DebugActiveProcessStop(pid) };
    if exited {
        return Err(Error::Debugger("the game closed".into()));
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_names_depend_on_the_game() {
        let rdx = Register::parse("RDX", Arch::X64).unwrap();
        assert_eq!(rdx.name(), "rdx");
        assert_eq!(Register::parse("r9d", Arch::X64).unwrap().name(), "r9d");
        assert_eq!(
            Register::parse("ebx", Arch::X64).unwrap().mask(),
            0xFFFF_FFFF
        );
        assert!(Register::parse("rdx", Arch::X86).is_none());
        assert!(Register::parse("r8d", Arch::X86).is_none());
        assert_eq!(Register::parse("esi", Arch::X86).unwrap().name(), "esi");
        assert!(Register::parse("rip", Arch::X64).is_none());
        assert!(Register::parse("xmm0", Arch::X64).is_none());
    }
}
