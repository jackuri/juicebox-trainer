//! "Find what writes to this address" (PLAN.md §5.2 `find_what_writes`).
//!
//! OpenTrainer attaches to the game as a debugger (it shows up as one; it
//! does not hide) and sets a hardware write breakpoint on the address in
//! every thread's debug registers. Each time an instruction writes there the
//! CPU stops the thread; the instruction is noted and the thread continues.
//! Nothing in the game's code or memory is changed. When the watch stops, the
//! breakpoints are cleared and the debugger detaches without killing the game.
//!
//! Only for processes that passed the anti-cheat guard (it takes an
//! [`AttachedProcess`]). Single-player games only, like everything else.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use iced_x86::{
    Decoder, DecoderOptions, Formatter, InstructionInfoFactory, IntelFormatter, OpAccess,
};
use serde::Serialize;
use windows::Win32::Foundation::{CloseHandle, HANDLE, NTSTATUS};
use windows::Win32::System::Diagnostics::Debug::{
    CONTEXT, CONTEXT_CONTROL_AMD64, CONTEXT_DEBUG_REGISTERS_AMD64, CREATE_PROCESS_DEBUG_EVENT,
    CREATE_THREAD_DEBUG_EVENT, ContinueDebugEvent, DEBUG_EVENT, DebugActiveProcess,
    DebugActiveProcessStop, DebugSetProcessKillOnExit, EXCEPTION_DEBUG_EVENT,
    EXIT_PROCESS_DEBUG_EVENT, GetThreadContext, LOAD_DLL_DEBUG_EVENT, SetThreadContext,
    WOW64_CONTEXT, WOW64_CONTEXT_CONTROL, WOW64_CONTEXT_DEBUG_REGISTERS, WaitForDebugEvent,
    Wow64GetThreadContext, Wow64SetThreadContext,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows::Win32::System::Threading::{
    OpenThread, ResumeThread, SuspendThread, THREAD_GET_CONTEXT, THREAD_QUERY_INFORMATION,
    THREAD_SET_CONTEXT, THREAD_SUSPEND_RESUME,
};

use crate::error::{Error, Result};
use crate::process::Arch;
use crate::session::AttachedProcess;
use crate::win::OwnedHandle;

/// A watch stops by itself after this long.
pub const MAX_WATCH: Duration = Duration::from_secs(120);

const DBG_CONTINUE: NTSTATUS = NTSTATUS(0x0001_0002);
const DBG_EXCEPTION_NOT_HANDLED: NTSTATUS = NTSTATUS(0x8001_0001_u32 as i32);
const EXCEPTION_SINGLE_STEP: i32 = 0x8000_0004_u32 as i32;
const STATUS_WX86_SINGLE_STEP: i32 = 0x4000_001E;
const EXCEPTION_BREAKPOINT: i32 = 0x8000_0003_u32 as i32;
const STATUS_WX86_BREAKPOINT: i32 = 0x4000_001F;

/// One instruction that wrote to the watched address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Writer {
    /// Start of the writing instruction.
    pub address: u64,
    /// `game.exe+0x1234`, if inside a module.
    pub location: Option<String>,
    /// e.g. `mov [rcx],eax`.
    pub text: String,
    /// The instruction's bytes, hex.
    pub bytes: String,
    pub count: u64,
}

/// `DR7` bits for breakpoint 0: enable (L0), break on write, `len` bytes.
pub(crate) fn dr7_for(len: usize) -> Result<u64> {
    let len_bits = match len {
        1 => 0b00,
        2 => 0b01,
        4 => 0b11,
        8 => 0b10,
        _ => {
            return Err(Error::Debugger(format!(
                "can't watch {len} bytes (use 1, 2, 4 or 8)"
            )));
        }
    };
    Ok(1 | (0b01 << 16) | (len_bits << 18))
}

/// Bits of DR7 that belong to breakpoint 0.
const DR7_SLOT0: u64 = 0b11 | (0b1111 << 16);

#[repr(C, align(16))]
struct Ctx64(CONTEXT);

enum Wow {
    No,
    Yes,
}

fn threads_of(pid: u32) -> Result<Vec<u32>> {
    // SAFETY: plain Win32 call; the handle is owned below.
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }
        .map_err(|e| Error::win32("CreateToolhelp32Snapshot(threads)", e))?;
    let snap = OwnedHandle::new(snap);
    let mut entry = THREADENTRY32 {
        dwSize: size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut out = Vec::new();
    // SAFETY: `entry` is correctly sized and `snap` is valid.
    let mut more = unsafe { Thread32First(snap.raw(), &mut entry) }.is_ok();
    while more {
        if entry.th32OwnerProcessID == pid {
            out.push(entry.th32ThreadID);
        }
        // SAFETY: as above.
        more = unsafe { Thread32Next(snap.raw(), &mut entry) }.is_ok();
    }
    Ok(out)
}

fn open_thread(tid: u32) -> Option<OwnedHandle> {
    let access =
        THREAD_GET_CONTEXT | THREAD_SET_CONTEXT | THREAD_SUSPEND_RESUME | THREAD_QUERY_INFORMATION;
    // SAFETY: plain Win32 call; the handle is owned by OwnedHandle.
    unsafe { OpenThread(access, false, tid) }
        .ok()
        .map(OwnedHandle::new)
}

/// Sets (`Some(address)`) or clears (`None`) breakpoint 0 in a thread. The
/// thread is suspended while its registers change.
fn set_slot0(thread: HANDLE, wow: &Wow, target: Option<(u64, u64)>) -> bool {
    // SAFETY: `thread` is a valid thread handle opened with suspend rights.
    if unsafe { SuspendThread(thread) } == u32::MAX {
        return false;
    }
    let ok = match wow {
        Wow::No => {
            // SAFETY: zeroed is a valid CONTEXT; it is 16-byte aligned.
            let mut c = Ctx64(unsafe { std::mem::zeroed() });
            c.0.ContextFlags = CONTEXT_DEBUG_REGISTERS_AMD64;
            // SAFETY: `c` is aligned and flags request only debug registers.
            let got = unsafe { GetThreadContext(thread, &mut c.0) }.is_ok();
            got && {
                match target {
                    Some((address, dr7)) => {
                        c.0.Dr0 = address;
                        c.0.Dr7 = (c.0.Dr7 & !DR7_SLOT0) | dr7;
                    }
                    None => {
                        c.0.Dr0 = 0;
                        c.0.Dr7 &= !DR7_SLOT0;
                    }
                }
                c.0.Dr6 = 0;
                // SAFETY: as above.
                unsafe { SetThreadContext(thread, &c.0) }.is_ok()
            }
        }
        Wow::Yes => {
            // SAFETY: zeroed is a valid WOW64_CONTEXT.
            let mut c: WOW64_CONTEXT = unsafe { std::mem::zeroed() };
            c.ContextFlags = WOW64_CONTEXT_DEBUG_REGISTERS;
            // SAFETY: flags request only debug registers.
            let got = unsafe { Wow64GetThreadContext(thread, &mut c) }.is_ok();
            got && {
                match target {
                    Some((address, dr7)) => {
                        c.Dr0 = address as u32;
                        c.Dr7 = (c.Dr7 & !(DR7_SLOT0 as u32)) | dr7 as u32;
                    }
                    None => {
                        c.Dr0 = 0;
                        c.Dr7 &= !(DR7_SLOT0 as u32);
                    }
                }
                c.Dr6 = 0;
                // SAFETY: as above.
                unsafe { Wow64SetThreadContext(thread, &c) }.is_ok()
            }
        }
    };
    // SAFETY: balanced with the SuspendThread above.
    unsafe { ResumeThread(thread) };
    ok
}

/// The instruction pointer and whether breakpoint 0 fired (DR6 bit 0); also
/// clears DR6.
fn hit_of(thread: HANDLE, wow: &Wow) -> Option<(u64, bool)> {
    match wow {
        Wow::No => {
            // SAFETY: zeroed is a valid CONTEXT; it is 16-byte aligned.
            let mut c = Ctx64(unsafe { std::mem::zeroed() });
            c.0.ContextFlags = CONTEXT_DEBUG_REGISTERS_AMD64 | CONTEXT_CONTROL_AMD64;
            // SAFETY: the thread is stopped at a debug event.
            unsafe { GetThreadContext(thread, &mut c.0) }.ok()?;
            let fired = c.0.Dr6 & 1 != 0;
            c.0.Dr6 = 0;
            // SAFETY: as above.
            let _ = unsafe { SetThreadContext(thread, &c.0) };
            Some((c.0.Rip, fired))
        }
        Wow::Yes => {
            // SAFETY: zeroed is a valid WOW64_CONTEXT.
            let mut c: WOW64_CONTEXT = unsafe { std::mem::zeroed() };
            c.ContextFlags = WOW64_CONTEXT_DEBUG_REGISTERS | WOW64_CONTEXT_CONTROL;
            // SAFETY: the thread is stopped at a debug event.
            unsafe { Wow64GetThreadContext(thread, &mut c) }.ok()?;
            let fired = c.Dr6 & 1 != 0;
            c.Dr6 = 0;
            // SAFETY: as above.
            let _ = unsafe { Wow64SetThreadContext(thread, &c) };
            Some((u64::from(c.Eip), fired))
        }
    }
}

/// The instruction that ends right before `ip` and writes memory. A data
/// breakpoint stops *after* the writing instruction.
pub(crate) fn writer_before(
    bytes_before: &[u8],
    ip: u64,
    arch: Arch,
) -> Option<(u64, usize, String)> {
    let bits = if arch == Arch::X86 { 32 } else { 64 };
    let mut info = InstructionInfoFactory::new();
    let mut f = IntelFormatter::new();
    let mut best = None;
    for back in 1..=bytes_before.len().min(15) {
        let start = bytes_before.len() - back;
        let start_ip = ip - back as u64;
        let mut d = Decoder::with_ip(bits, &bytes_before[start..], start_ip, DecoderOptions::NONE);
        let instr = d.decode();
        if instr.is_invalid() || instr.len() != back {
            continue;
        }
        let writes = info.info(&instr).used_memory().iter().any(|m| {
            matches!(
                m.access(),
                OpAccess::Write
                    | OpAccess::ReadWrite
                    | OpAccess::CondWrite
                    | OpAccess::ReadCondWrite
            )
        });
        if writes {
            let mut text = String::new();
            f.format(&instr, &mut text);
            // The shortest match is the most likely real instruction.
            best = Some((start_ip, back, text));
            break;
        }
    }
    best
}

struct Shared {
    stop: AtomicBool,
}

/// A running watch. Stop it to get the results; dropping it stops it too.
pub struct WriteWatch {
    process: Arc<AttachedProcess>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<Result<BTreeMap<u64, u64>>>>,
    started: Instant,
}

impl std::fmt::Debug for WriteWatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WriteWatch")
            .field("pid", &self.process.pid())
            .finish()
    }
}

impl WriteWatch {
    /// Starts watching `len` bytes at `address` (aligned to `len`).
    pub fn start(process: Arc<AttachedProcess>, address: u64, len: usize) -> Result<Self> {
        let dr7 = dr7_for(len)?;
        if !address.is_multiple_of(len as u64) {
            return Err(Error::Debugger(format!(
                "the address must be a multiple of {len} to watch {len} bytes"
            )));
        }
        process.recheck_guard()?;
        let wow = match process.arch() {
            Arch::X64 => false,
            Arch::X86 => true,
            other => return Err(Error::Debugger(format!("can't watch a {other} process"))),
        };
        let pid = process.pid();
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
        });
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let s = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("write-watch".into())
            .spawn(move || debug_loop(pid, wow, address, dr7, &s, &ready_tx))
            .map_err(|e| Error::Debugger(e.to_string()))?;
        match ready_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = thread.join();
                return Err(e);
            }
            Err(_) => {
                shared.stop.store(true, Ordering::SeqCst);
                let _ = thread.join();
                return Err(Error::Debugger("the debugger didn't start".into()));
            }
        }
        Ok(Self {
            process,
            shared,
            thread: Some(thread),
            started: Instant::now(),
        })
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Stops watching, detaches the debugger and lists the writers, most
    /// frequent first.
    pub fn stop(mut self) -> Result<Vec<Writer>> {
        self.finish()
    }

    fn finish(&mut self) -> Result<Vec<Writer>> {
        self.shared.stop.store(true, Ordering::SeqCst);
        let Some(thread) = self.thread.take() else {
            return Ok(Vec::new());
        };
        let hits = thread
            .join()
            .map_err(|_| Error::Debugger("the debugger thread failed".into()))??;
        let arch = self.process.arch();
        let modules = self.process.modules();
        let mut out = Vec::new();
        for (ip, count) in hits {
            let from = ip.saturating_sub(15);
            let Ok(before) = self.process.read_bytes(from, (ip - from) as usize) else {
                continue;
            };
            let Some((address, len, text)) = writer_before(&before, ip, arch) else {
                continue;
            };
            let bytes = before[before.len() - len..]
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" ");
            let location = modules
                .iter()
                .find(|m| m.contains(address))
                .map(|m| format!("{}+{:#X}", m.name, address - m.base));
            out.push(Writer {
                address,
                location,
                text,
                bytes,
                count,
            });
        }
        out.sort_by(|a, b| b.count.cmp(&a.count).then(a.address.cmp(&b.address)));
        Ok(out)
    }
}

impl Drop for WriteWatch {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

/// Runs on its own thread: the Win32 debug API ties a debuggee to the thread
/// that attached.
fn debug_loop(
    pid: u32,
    wow: bool,
    address: u64,
    dr7: u64,
    shared: &Shared,
    ready: &std::sync::mpsc::Sender<Result<()>>,
) -> Result<BTreeMap<u64, u64>> {
    let mode = if wow { Wow::Yes } else { Wow::No };
    // SAFETY: plain Win32 calls on a pid we are allowed to open.
    if let Err(e) = unsafe { DebugActiveProcess(pid) } {
        let err = Error::Debugger(format!(
            "could not attach as a debugger ({e}); another debugger may already be attached"
        ));
        let _ = ready.send(Err(Error::Debugger(err.to_string())));
        return Err(err);
    }
    // Detaching must never kill the game.
    // SAFETY: affects only this thread's debuggees.
    let _ = unsafe { DebugSetProcessKillOnExit(false) };

    let mut threads: BTreeMap<u32, OwnedHandle> = BTreeMap::new();
    let arm = |threads: &mut BTreeMap<u32, OwnedHandle>| {
        for tid in threads_of(pid).unwrap_or_default() {
            if let std::collections::btree_map::Entry::Vacant(slot) = threads.entry(tid)
                && let Some(h) = open_thread(tid)
            {
                set_slot0(h.raw(), &mode, Some((address, dr7)));
                slot.insert(h);
            }
        }
    };

    let mut hits: BTreeMap<u64, u64> = BTreeMap::new();
    let mut armed = false;
    let deadline = Instant::now() + MAX_WATCH;
    let mut exited = false;
    let mut event = DEBUG_EVENT::default();
    loop {
        let stopping = shared.stop.load(Ordering::SeqCst) || Instant::now() > deadline;
        if stopping && armed {
            // Clear the breakpoints, then let any in-flight events through.
            for h in threads.values() {
                set_slot0(h.raw(), &mode, None);
            }
            armed = false;
        }
        // SAFETY: `event` is a valid out-buffer.
        let got = unsafe { WaitForDebugEvent(&mut event, if stopping { 50 } else { 100 }) }.is_ok();
        if !got {
            if stopping {
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
                    set_slot0(h.raw(), &mode, Some((address, dr7)));
                    threads.insert(event.dwThreadId, h);
                }
            }
            EXIT_PROCESS_DEBUG_EVENT => exited = true,
            EXCEPTION_DEBUG_EVENT => {
                // SAFETY: the union member matches the event code.
                let code = unsafe { event.u.Exception.ExceptionRecord.ExceptionCode }.0;
                if code == EXCEPTION_BREAKPOINT || code == STATUS_WX86_BREAKPOINT {
                    // The attach breakpoint: arm now that the game is stopped.
                    if !armed && !stopping {
                        arm(&mut threads);
                        armed = true;
                        let _ = ready.send(Ok(()));
                    }
                } else if code == EXCEPTION_SINGLE_STEP || code == STATUS_WX86_SINGLE_STEP {
                    let ours = threads
                        .get(&event.dwThreadId)
                        .and_then(|h| hit_of(h.raw(), &mode));
                    match ours {
                        Some((ip, true)) => *hits.entry(ip).or_default() += 1,
                        // Not our breakpoint (e.g. the game single-steps itself).
                        _ => status = DBG_EXCEPTION_NOT_HANDLED,
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
    if !armed && hits.is_empty() && !exited {
        // Never got the attach breakpoint: tell start() if it's still waiting.
        let _ = ready.send(Err(Error::Debugger(
            "the game didn't stop for the debugger".into(),
        )));
    }
    // SAFETY: detaches this thread's debugger from the game; the game keeps running.
    let _ = unsafe { DebugActiveProcessStop(pid) };
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dr7_bits() {
        assert_eq!(dr7_for(4).unwrap(), 1 | (1 << 16) | (0b11 << 18));
        assert_eq!(dr7_for(1).unwrap(), 1 | (1 << 16));
        assert_eq!(dr7_for(8).unwrap(), 1 | (1 << 16) | (0b10 << 18));
        assert!(dr7_for(3).is_err());
    }

    #[test]
    fn finds_the_writing_instruction_before_ip() {
        // mov eax,[rcx]; add eax,edx; mov [rcx],eax  — the hit is after the store.
        let code = [0x8B, 0x01, 0x01, 0xD0, 0x89, 0x01];
        let ip = 0x40_1006;
        let (at, len, text) = writer_before(&code, ip, Arch::X64).unwrap();
        assert_eq!((at, len), (0x40_1004, 2));
        assert_eq!(text, "mov [rcx],eax");
        // Nothing that writes memory.
        assert!(writer_before(&[0x90, 0x90, 0x01, 0xD0], 0x1004, Arch::X64).is_none());
    }
}
