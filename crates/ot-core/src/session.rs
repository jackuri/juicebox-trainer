//! An attached target process. The only way to touch another process's memory.

use std::ffi::c_void;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tracing::{info, warn};
use windows::Win32::Foundation::WAIT_TIMEOUT;
use windows::Win32::System::Diagnostics::Debug::{
    FlushInstructionCache, ReadProcessMemory, WriteProcessMemory,
};
use windows::Win32::System::Memory::{
    MEM_COMMIT, MEM_IMAGE, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS,
    VirtualProtectEx, VirtualQueryEx,
};
use windows::Win32::System::Threading::{
    PROCESS_QUERY_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_VM_OPERATION, PROCESS_VM_READ,
    PROCESS_VM_WRITE, WaitForSingleObject,
};

use crate::address::{self, AddressExpr};
use crate::aob::Pattern;
use crate::error::{Error, Result};
use crate::guard::{self, Verdict};
use crate::memory::{Region, decode_protect};
use crate::modules::{self, Module};
use crate::process::{self, Arch, ProcessInfo};
use crate::win::OwnedHandle;

/// How often the guard is re-run while attached. Anti-cheat that starts after
/// we attached (e.g. a launcher that spawns it) is caught within this window.
pub const GUARD_RECHECK_INTERVAL: Duration = Duration::from_secs(5);

/// A process OpenTrainer is attached to.
///
/// Created only by [`AttachedProcess::attach`], which runs the anti-cheat guard
/// first. While attached, every read and write re-runs the guard at most every
/// [`GUARD_RECHECK_INTERVAL`]; once it blocks, the session stays blocked.
#[derive(Debug)]
pub struct AttachedProcess {
    handle: OwnedHandle,
    info: ProcessInfo,
    modules: Mutex<Vec<Module>>,
    last_guard_check: Mutex<Instant>,
    blocked: Mutex<Option<Verdict>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AttachedProcess {
    /// Attaches to `pid` if, and only if, the anti-cheat guard allows it.
    pub fn attach(pid: u32) -> Result<Self> {
        let verdict = guard::check(pid);
        if !verdict.allowed {
            warn!(pid, target = %verdict.target, "attach blocked by guard: {verdict}");
            return Err(Error::Blocked(verdict));
        }
        let handle = process::open(
            pid,
            PROCESS_VM_READ
                | PROCESS_VM_WRITE
                | PROCESS_VM_OPERATION
                | PROCESS_QUERY_INFORMATION
                | PROCESS_SYNCHRONIZE,
        )?;
        let arch = process::arch_of(&handle);
        let exe_path = process::exe_path_of(&handle);
        let name = exe_path
            .as_deref()
            .map(|p| process::file_name(p).to_string())
            .unwrap_or_else(|| verdict.target.clone());
        let parent_pid = process::snapshot_processes()
            .ok()
            .and_then(|l| l.into_iter().find(|p| p.pid == pid))
            .map(|p| p.parent_pid)
            .unwrap_or(0);
        let modules = modules::modules_of(pid, arch)?;
        info!(pid, %name, %arch, modules = modules.len(), "attached");
        Ok(Self {
            handle,
            info: ProcessInfo {
                pid,
                parent_pid,
                name,
                exe_path,
                arch,
                threads: 0,
            },
            modules: Mutex::new(modules),
            last_guard_check: Mutex::new(Instant::now()),
            blocked: Mutex::new(None),
        })
    }

    /// Detaches. (Dropping the value does the same.)
    pub fn detach(self) {}

    pub fn pid(&self) -> u32 {
        self.info.pid
    }

    pub fn arch(&self) -> Arch {
        self.info.arch
    }

    pub fn info(&self) -> &ProcessInfo {
        &self.info
    }

    /// True while the target process is still running.
    pub fn is_alive(&self) -> bool {
        // SAFETY: the handle was opened with SYNCHRONIZE and is valid.
        unsafe { WaitForSingleObject(self.handle.raw(), 0) == WAIT_TIMEOUT }
    }

    /// The module map captured at attach (or the last refresh).
    pub fn modules(&self) -> Vec<Module> {
        lock(&self.modules).clone()
    }

    /// Re-reads the module map from the target.
    pub fn refresh_modules(&self) -> Result<Vec<Module>> {
        self.gate()?;
        let fresh = modules::modules_of(self.pid(), self.arch())?;
        *lock(&self.modules) = fresh.clone();
        Ok(fresh)
    }

    /// Runs the anti-cheat guard now. If it blocks, this session is blocked
    /// for good: every later read or write fails with [`Error::Blocked`].
    pub fn recheck_guard(&self) -> Result<()> {
        let verdict = guard::check(self.pid());
        *lock(&self.last_guard_check) = Instant::now();
        if verdict.allowed {
            Ok(())
        } else {
            warn!(
                pid = self.pid(),
                "guard blocked an attached session: {verdict}"
            );
            *lock(&self.blocked) = Some(verdict.clone());
            Err(Error::Blocked(verdict))
        }
    }

    /// Gate in front of every memory operation.
    fn gate(&self) -> Result<()> {
        if let Some(v) = lock(&self.blocked).clone() {
            return Err(Error::Blocked(v));
        }
        if !self.is_alive() {
            return Err(Error::ProcessExited { pid: self.pid() });
        }
        // Claim the recheck before running it so parallel scan threads don't
        // all re-run the guard at once.
        let due = {
            let mut last = lock(&self.last_guard_check);
            let due = last.elapsed() >= GUARD_RECHECK_INTERVAL;
            if due {
                *last = Instant::now();
            }
            due
        };
        if due {
            self.recheck_guard()?;
        }
        Ok(())
    }

    fn check_range(&self, address: u64, len: usize) -> Result<()> {
        let limit = self.arch().address_limit();
        let end = address.checked_add(len as u64);
        match end {
            Some(end) if address != 0 && end <= limit => Ok(()),
            _ => Err(Error::AddressOutOfRange {
                address,
                len,
                arch: self.arch(),
            }),
        }
    }

    /// Resolves an address expression (`0x…` or `module.exe+0x…`).
    pub fn resolve(&self, expr: &str) -> Result<u64> {
        match address::parse(expr)? {
            AddressExpr::Absolute(a) => Ok(a),
            AddressExpr::ModuleOffset { module, offset } => {
                let cached = modules::find(&lock(&self.modules), &module).map(|m| m.base);
                let base = match cached {
                    Some(b) => b,
                    // The module may have loaded after attach.
                    None => modules::find(&self.refresh_modules()?, &module)
                        .map(|m| m.base)
                        .ok_or_else(|| Error::ModuleNotFound(module.clone()))?,
                };
                base.checked_add(offset)
                    .ok_or_else(|| Error::InvalidAddress(expr.to_string()))
            }
        }
    }

    /// Reads exactly `buf.len()` bytes at `address`.
    pub fn read_into(&self, address: u64, buf: &mut [u8]) -> Result<()> {
        self.gate()?;
        self.check_range(address, buf.len())?;
        let mut read = 0usize;
        // SAFETY: `buf` is valid for `buf.len()` bytes; the handle has VM_READ.
        let ok = unsafe {
            ReadProcessMemory(
                self.handle.raw(),
                address as usize as *const c_void,
                buf.as_mut_ptr().cast(),
                buf.len(),
                Some(&mut read),
            )
        };
        if ok.is_err() || read != buf.len() {
            return Err(Error::Read {
                address,
                len: buf.len(),
            });
        }
        Ok(())
    }

    pub fn read_bytes(&self, address: u64, len: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.read_into(address, &mut buf)?;
        Ok(buf)
    }

    /// Writes `data` at `address`. Every write is logged.
    pub fn write_bytes(&self, address: u64, data: &[u8]) -> Result<()> {
        self.write_raw(address, data)?;
        info!(
            pid = self.pid(),
            address = format_args!("{address:#X}"),
            len = data.len(),
            "memory write"
        );
        Ok(())
    }

    /// Like [`write_bytes`](Self::write_bytes) without the per-write log line.
    /// Only the freezer uses this; it logs each freeze once instead.
    pub(crate) fn write_bytes_quiet(&self, address: u64, data: &[u8]) -> Result<()> {
        self.write_raw(address, data)
    }

    fn write_raw(&self, address: u64, data: &[u8]) -> Result<()> {
        self.gate()?;
        self.check_range(address, data.len())?;
        let mut written = 0usize;
        // SAFETY: `data` is valid for `data.len()` bytes; the handle has VM_WRITE|VM_OPERATION.
        let ok = unsafe {
            WriteProcessMemory(
                self.handle.raw(),
                address as usize as *const c_void,
                data.as_ptr().cast(),
                data.len(),
                Some(&mut written),
            )
        };
        if ok.is_err() || written != data.len() {
            warn!(
                pid = self.pid(),
                address = format_args!("{address:#X}"),
                len = data.len(),
                "write failed"
            );
            return Err(Error::Write {
                address,
                len: data.len(),
            });
        }
        Ok(())
    }

    pub fn read_value(&self, address: u64, ty: crate::ValueType) -> Result<crate::Value> {
        let bytes = self.read_bytes(address, ty.size(self.arch()))?;
        crate::Value::from_bytes(ty, &bytes, self.arch()).ok_or(Error::Read {
            address,
            len: bytes.len(),
        })
    }

    pub fn write_value(&self, address: u64, value: &crate::Value) -> Result<()> {
        self.write_bytes(address, &value.to_bytes(self.arch()))
    }

    /// Reads a pointer-sized value (4 bytes on x86, 8 on x64).
    pub fn read_pointer(&self, address: u64) -> Result<u64> {
        match self.read_value(address, crate::ValueType::Ptr)? {
            crate::Value::Ptr(p) => Ok(p),
            _ => unreachable!("read_value(Ptr) returns Value::Ptr"),
        }
    }

    /// Writes code bytes (e.g. a patch) at `address`: makes the page writable
    /// for the write, restores its protection and flushes the instruction
    /// cache. Logged like every other write. Callers check the bytes first
    /// (see [`crate::code::check_patch`]).
    pub fn write_code(&self, address: u64, data: &[u8]) -> Result<()> {
        self.gate()?;
        self.check_range(address, data.len())?;
        let ptr = address as usize as *const c_void;
        let mut old = PAGE_PROTECTION_FLAGS(0);
        // SAFETY: plain Win32 call on our handle (opened with VM_OPERATION).
        unsafe {
            VirtualProtectEx(
                self.handle.raw(),
                ptr,
                data.len(),
                PAGE_EXECUTE_READWRITE,
                &mut old,
            )
        }
        .map_err(|e| Error::win32("VirtualProtectEx", e))?;
        let written = self.write_raw(address, data);
        let mut ignored = PAGE_PROTECTION_FLAGS(0);
        // SAFETY: restores the protection we just changed on the same range.
        let _ = unsafe { VirtualProtectEx(self.handle.raw(), ptr, data.len(), old, &mut ignored) };
        // SAFETY: flushes the written range in the target; harmless if it fails.
        let _ = unsafe { FlushInstructionCache(self.handle.raw(), Some(ptr), data.len()) };
        written?;
        info!(
            pid = self.pid(),
            address = format_args!("{address:#X}"),
            len = data.len(),
            "code write"
        );
        Ok(())
    }

    /// Follows a pointer chain the way Cheat Engine does: for each offset,
    /// read the pointer at the current address and add the offset.
    /// `[[base]+o1]+o2`. With no offsets the result is `base`.
    pub fn resolve_chain(&self, base: u64, offsets: &[i64]) -> Result<u64> {
        let mut address = base;
        for &offset in offsets {
            let p = self.read_pointer(address)?;
            if p == 0 {
                return Err(Error::NullPointer { at: address });
            }
            address = p
                .checked_add_signed(offset)
                .ok_or_else(|| Error::InvalidAddress(format!("{p:#X}{offset:+#X}")))?;
        }
        Ok(address)
    }

    /// Finds a loaded module by name (refreshing the map once if needed).
    pub fn module(&self, name: &str) -> Result<Module> {
        if let Some(m) = modules::find(&lock(&self.modules), name) {
            return Ok(m.clone());
        }
        modules::find(&self.refresh_modules()?, name)
            .cloned()
            .ok_or_else(|| Error::ModuleNotFound(name.to_string()))
    }

    /// The main executable's module (the first module whose name matches the
    /// process name).
    pub fn main_module(&self) -> Result<Module> {
        self.module(&self.info.name)
    }

    /// Searches the readable parts of a module's image for `pattern`.
    /// Returns at most `limit` match addresses, lowest first.
    pub fn aob_scan(&self, module: &Module, pattern: &Pattern, limit: usize) -> Result<Vec<u64>> {
        let mut out = Vec::new();
        // Contiguous readable runs inside the module.
        let mut runs: Vec<(u64, u64)> = Vec::new();
        for r in self.regions()? {
            let start = r.base.max(module.base);
            let end = (r.base + r.size).min(module.end());
            if start >= end || !r.readable {
                continue;
            }
            match runs.last_mut() {
                Some(last) if last.1 == start => last.1 = end,
                _ => runs.push((start, end)),
            }
        }
        for (start, end) in runs {
            let len = usize::try_from(end - start).map_err(|_| Error::Read {
                address: start,
                len: usize::MAX,
            })?;
            let mut buf = vec![0u8; len];
            if self.read_into(start, &mut buf).is_err() {
                // A page may have been decommitted since the region walk.
                continue;
            }
            for at in pattern.find_all(&buf, limit - out.len()) {
                out.push(start + at as u64);
            }
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    /// SHA-256 of the target's executable file, if its path is known.
    pub fn exe_sha256(&self) -> Option<String> {
        let path = self.info.exe_path.as_deref()?;
        crate::hash::sha256_file(path).ok()
    }

    /// Committed memory regions of the target.
    pub fn regions(&self) -> Result<Vec<Region>> {
        self.gate()?;
        let limit = self.arch().address_limit();
        let mut out = Vec::new();
        let mut address: u64 = 0;
        while address < limit {
            let mut mbi = MEMORY_BASIC_INFORMATION::default();
            // SAFETY: `mbi` is a valid out-buffer of the size we pass.
            let n = unsafe {
                VirtualQueryEx(
                    self.handle.raw(),
                    Some(address as usize as *const c_void),
                    &mut mbi,
                    size_of::<MEMORY_BASIC_INFORMATION>(),
                )
            };
            if n == 0 {
                break;
            }
            let base = mbi.BaseAddress as u64;
            let size = mbi.RegionSize as u64;
            if size == 0 {
                break;
            }
            if mbi.State == MEM_COMMIT {
                let (readable, writable, executable) = decode_protect(mbi.Protect.0);
                out.push(Region {
                    base,
                    size,
                    protect: mbi.Protect.0,
                    readable,
                    writable,
                    executable,
                    image: mbi.Type == MEM_IMAGE,
                });
            }
            match base.checked_add(size) {
                Some(next) if next > address => address = next,
                _ => break,
            }
        }
        Ok(out)
    }
}

impl Drop for AttachedProcess {
    fn drop(&mut self) {
        info!(pid = self.info.pid, name = %self.info.name, "detached");
    }
}
