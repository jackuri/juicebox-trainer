//! Module map of a target process (WOW64-aware).

use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_BAD_LENGTH, ERROR_PARTIAL_COPY};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW, TH32CS_SNAPMODULE,
    TH32CS_SNAPMODULE32,
};

use crate::error::{Error, Result};
use crate::process::Arch;
use crate::win::{OwnedHandle, from_wide, is_code};

/// A loaded module (exe or DLL) in the target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Module {
    pub name: String,
    pub path: String,
    pub base: u64,
    pub size: u32,
}

impl Module {
    /// One past the last byte of the module image.
    pub fn end(&self) -> u64 {
        self.base + u64::from(self.size)
    }

    pub fn contains(&self, address: u64) -> bool {
        (self.base..self.end()).contains(&address)
    }
}

/// Finds a module by file name, ignoring case.
pub fn find<'a>(modules: &'a [Module], name: &str) -> Option<&'a Module> {
    modules.iter().find(|m| m.name.eq_ignore_ascii_case(name))
}

/// Lists the modules loaded in `pid`, sorted by base address.
///
/// For x86 (WOW64) targets only the 32-bit modules are returned; the 64-bit
/// WOW64 layer (ntdll, wow64*.dll) lives above 4 GiB and is filtered out.
pub fn modules_of(pid: u32, arch: Arch) -> Result<Vec<Module>> {
    let snap = snapshot(pid)?;
    let mut entry = MODULEENTRY32W {
        dwSize: size_of::<MODULEENTRY32W>() as u32,
        ..Default::default()
    };
    let mut out = Vec::new();
    // SAFETY: `entry` is a correctly sized MODULEENTRY32W and `snap` is valid.
    let mut more = unsafe { Module32FirstW(snap.raw(), &mut entry) }.is_ok();
    while more {
        out.push(Module {
            name: from_wide(&entry.szModule),
            path: from_wide(&entry.szExePath),
            base: entry.modBaseAddr as u64,
            size: entry.modBaseSize,
        });
        // SAFETY: as above.
        more = unsafe { Module32NextW(snap.raw(), &mut entry) }.is_ok();
    }
    if arch == Arch::X86 {
        out.retain(|m| m.end() <= Arch::X86.address_limit());
    }
    out.sort_by_key(|m| m.base);
    Ok(out)
}

fn snapshot(pid: u32) -> Result<OwnedHandle> {
    // ToolHelp can fail transiently while the target is loading modules.
    const RETRIES: u32 = 20;
    let mut attempt = 0;
    loop {
        // SAFETY: plain Win32 call; the returned handle is owned below.
        match unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid) } {
            Ok(h) => return Ok(OwnedHandle::new(h)),
            Err(e)
                if (is_code(&e, ERROR_BAD_LENGTH) || is_code(&e, ERROR_PARTIAL_COPY))
                    && attempt < RETRIES =>
            {
                attempt += 1;
                thread::sleep(Duration::from_millis(25));
            }
            Err(e) if is_code(&e, ERROR_ACCESS_DENIED) => return Err(Error::AccessDenied { pid }),
            Err(e) => return Err(Error::win32("CreateToolhelp32Snapshot(modules)", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(name: &str, base: u64, size: u32) -> Module {
        Module {
            name: name.into(),
            path: format!(r"C:\x\{name}"),
            base,
            size,
        }
    }

    #[test]
    fn find_is_case_insensitive() {
        let list = [
            m("Game.exe", 0x400000, 0x1000),
            m("KERNEL32.DLL", 0x7000, 0x10),
        ];
        assert_eq!(find(&list, "game.EXE").unwrap().base, 0x400000);
        assert!(find(&list, "nope.dll").is_none());
    }

    #[test]
    fn contains_is_half_open() {
        let module = m("a.dll", 0x1000, 0x100);
        assert!(module.contains(0x1000));
        assert!(module.contains(0x10FF));
        assert!(!module.contains(0x1100));
    }

    #[test]
    fn own_module_map_has_exe_and_ntdll() {
        let mods = modules_of(std::process::id(), Arch::X64).expect("modules");
        assert!(mods.iter().any(|m| m.name.to_lowercase().ends_with(".exe")));
        assert!(find(&mods, "ntdll.dll").is_some());
    }
}
