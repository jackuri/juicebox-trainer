//! Process enumeration, opening and architecture detection.

use std::fmt;

use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::SystemInformation::{
    IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64,
    IMAGE_FILE_MACHINE_I386, IMAGE_FILE_MACHINE_UNKNOWN,
};
use windows::Win32::System::Threading::{
    IsWow64Process2, OpenProcess, PROCESS_ACCESS_RIGHTS, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::core::PWSTR;

use crate::error::{Error, Result};
use crate::win::{OwnedHandle, from_wide, is_code};

/// CPU architecture of a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    /// 32-bit x86, running under WOW64 on an x64 host.
    X86,
    X64,
    Arm64,
    Unknown,
}

impl Arch {
    /// Size of a pointer in the target, in bytes.
    pub fn pointer_size(self) -> usize {
        match self {
            Arch::X86 => 4,
            _ => 8,
        }
    }

    /// One past the highest address the target can use.
    pub fn address_limit(self) -> u64 {
        match self {
            Arch::X86 => 1 << 32,
            _ => 1 << 56,
        }
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Arch::X86 => "x86",
            Arch::X64 => "x64",
            Arch::Arm64 => "arm64",
            Arch::Unknown => "unknown",
        })
    }
}

/// A running process as shown in the process picker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub parent_pid: u32,
    pub name: String,
    /// Full image path; `None` when the process can't be queried (system or protected).
    pub exe_path: Option<String>,
    pub arch: Arch,
    pub threads: u32,
}

/// Minimal entry from a ToolHelp process snapshot.
#[derive(Debug, Clone)]
pub(crate) struct RawProcess {
    pub pid: u32,
    pub parent_pid: u32,
    pub name: String,
    pub threads: u32,
}

/// Every running process (names only). Cheap; used by the guard.
pub(crate) fn snapshot_processes() -> Result<Vec<RawProcess>> {
    // SAFETY: plain Win32 call; the returned handle is owned below.
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map_err(|e| Error::win32("CreateToolhelp32Snapshot(processes)", e))?;
    let snap = OwnedHandle::new(snap);

    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut out = Vec::new();
    // SAFETY: `entry` is a correctly sized PROCESSENTRY32W and `snap` is valid.
    let mut more = unsafe { Process32FirstW(snap.raw(), &mut entry) }.is_ok();
    while more {
        out.push(RawProcess {
            pid: entry.th32ProcessID,
            parent_pid: entry.th32ParentProcessID,
            name: from_wide(&entry.szExeFile),
            threads: entry.cntThreads,
        });
        // SAFETY: as above.
        more = unsafe { Process32NextW(snap.raw(), &mut entry) }.is_ok();
    }
    Ok(out)
}

/// Lists running processes with their image path and architecture, sorted by name.
pub fn list_processes() -> Result<Vec<ProcessInfo>> {
    let mut list: Vec<ProcessInfo> = snapshot_processes()?
        .into_iter()
        .filter(|p| p.pid != 0)
        .map(|p| {
            let (exe_path, arch) = match open_query(p.pid) {
                Ok(h) => (exe_path_of(&h), arch_of(&h)),
                Err(_) => (None, Arch::Unknown),
            };
            ProcessInfo {
                pid: p.pid,
                parent_pid: p.parent_pid,
                name: p.name,
                exe_path,
                arch,
                threads: p.threads,
            }
        })
        .collect();
    list.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.pid.cmp(&b.pid))
    });
    Ok(list)
}

/// Opens a process with the given rights.
pub(crate) fn open(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> Result<OwnedHandle> {
    // SAFETY: plain Win32 call; the handle is owned by the returned value.
    match unsafe { OpenProcess(access, false, pid) } {
        Ok(h) => Ok(OwnedHandle::new(h)),
        Err(e) if is_code(&e, ERROR_ACCESS_DENIED) => Err(Error::AccessDenied { pid }),
        Err(e) if is_code(&e, ERROR_INVALID_PARAMETER) => Err(Error::ProcessNotFound { pid }),
        Err(e) => Err(Error::win32("OpenProcess", e)),
    }
}

/// Opens a process for querying its path/architecture only.
pub(crate) fn open_query(pid: u32) -> Result<OwnedHandle> {
    open(pid, PROCESS_QUERY_LIMITED_INFORMATION)
}

/// Full Win32 image path of an open process.
pub(crate) fn exe_path_of(handle: &OwnedHandle) -> Option<String> {
    let mut buf = vec![0u16; 32_768];
    let mut len = buf.len() as u32;
    // SAFETY: `buf` is valid for `len` UTF-16 units and `len` is updated in place.
    unsafe {
        QueryFullProcessImageNameW(
            handle.raw(),
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    }
    .ok()?;
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

/// Architecture of an open process (WOW64-aware).
pub(crate) fn arch_of(handle: &OwnedHandle) -> Arch {
    let mut process = IMAGE_FILE_MACHINE::default();
    let mut native = IMAGE_FILE_MACHINE::default();
    // SAFETY: both out-pointers are valid for writes.
    if unsafe { IsWow64Process2(handle.raw(), &mut process, Some(&mut native)) }.is_err() {
        return Arch::Unknown;
    }
    machine_to_arch(process, native)
}

fn machine_to_arch(process: IMAGE_FILE_MACHINE, native: IMAGE_FILE_MACHINE) -> Arch {
    // Not WOW64: the process runs as the native machine type.
    let machine = if process == IMAGE_FILE_MACHINE_UNKNOWN {
        native
    } else {
        process
    };
    match machine {
        IMAGE_FILE_MACHINE_I386 => Arch::X86,
        IMAGE_FILE_MACHINE_AMD64 => Arch::X64,
        IMAGE_FILE_MACHINE_ARM64 => Arch::Arm64,
        _ => Arch::Unknown,
    }
}

/// File name part of a Windows path.
pub(crate) fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wow64_maps_to_x86() {
        assert_eq!(
            machine_to_arch(IMAGE_FILE_MACHINE_I386, IMAGE_FILE_MACHINE_AMD64),
            Arch::X86
        );
        assert_eq!(
            machine_to_arch(IMAGE_FILE_MACHINE_UNKNOWN, IMAGE_FILE_MACHINE_AMD64),
            Arch::X64
        );
    }

    #[test]
    fn pointer_sizes() {
        assert_eq!(Arch::X86.pointer_size(), 4);
        assert_eq!(Arch::X64.pointer_size(), 8);
        assert_eq!(Arch::X86.address_limit(), 0x1_0000_0000);
    }

    #[test]
    fn lists_this_test_process() {
        let me = std::process::id();
        let list = list_processes().expect("process list");
        let entry = list.iter().find(|p| p.pid == me).expect("own pid listed");
        assert_eq!(entry.arch, Arch::X64);
        assert!(entry.exe_path.is_some());
    }

    #[test]
    fn file_name_handles_both_separators() {
        assert_eq!(file_name(r"C:\Games\a\game.exe"), "game.exe");
        assert_eq!(file_name("C:/x/y.exe"), "y.exe");
        assert_eq!(file_name("plain.exe"), "plain.exe");
    }
}
