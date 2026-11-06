//! Gathers the facts the guard decides on. Any check that fails to run is
//! recorded in `Snapshot::errors`, which blocks (fail closed).

use std::ffi::c_void;
use std::path::Path;

use windows::Win32::Foundation::ERROR_MORE_DATA;
use windows::Win32::System::ProcessStatus::{EnumDeviceDrivers, GetDeviceDriverBaseNameW};
use windows::Win32::System::Services::{
    CloseServiceHandle, ENUM_SERVICE_STATUS_PROCESSW, EnumServicesStatusExW, OpenSCManagerW,
    SC_ENUM_PROCESS_INFO, SC_MANAGER_ENUMERATE_SERVICE, SERVICE_ACTIVE, SERVICE_DRIVER,
    SERVICE_WIN32,
};
use windows::core::PCWSTR;

use super::Snapshot;
use crate::modules;
use crate::process::{self, Arch};
use crate::win::{from_wide, is_code};

/// How many folders above the exe folder to scan for install markers. Unreal
/// games keep the exe in `<Game>/Binaries/Win64/` with anti-cheat at the root.
const MARKER_PARENT_LEVELS: usize = 3;

pub(crate) fn collect(pid: u32) -> Snapshot {
    let mut s = Snapshot::default();

    match process::snapshot_processes() {
        Ok(list) => {
            if let Some(p) = list.iter().find(|p| p.pid == pid) {
                s.target_name = p.name.clone();
            } else {
                s.errors.push(format!("process {pid} is not running"));
            }
            s.processes = list.into_iter().map(|p| p.name).collect();
        }
        Err(e) => s.errors.push(format!("could not list processes: {e}")),
    }

    match running_services() {
        Ok(list) => s.services = list,
        Err(e) => s.errors.push(format!("could not list services: {e}")),
    }

    // Kernel drivers are also covered by the service list above (driver
    // services). Driver file names are an extra signal and may be unavailable
    // without admin rights, so a failure here does not block on its own.
    s.drivers = loaded_drivers();

    let (exe_path, arch) = match process::open_query(pid) {
        Ok(h) => (process::exe_path_of(&h), process::arch_of(&h)),
        Err(e) => {
            s.errors.push(format!("could not open process {pid}: {e}"));
            (None, Arch::Unknown)
        }
    };

    match modules::modules_of(pid, arch) {
        Ok(list) => s.modules = list.into_iter().map(|m| m.name).collect(),
        Err(e) => s.errors.push(format!("could not list modules: {e}")),
    }

    match exe_path {
        Some(path) => {
            if s.target_name.is_empty() {
                s.target_name = process::file_name(&path).to_string();
            }
            match install_entries(Path::new(&path)) {
                Ok(entries) => s.install_entries = entries,
                Err(e) => s.errors.push(format!("could not read game folder: {e}")),
            }
        }
        None => s.errors.push("could not get the game's exe path".into()),
    }

    if s.target_name.is_empty() {
        s.target_name = format!("pid {pid}");
    }
    s
}

/// Entry names in the exe's folder (required) and up to
/// [`MARKER_PARENT_LEVELS`] parent folders (best effort).
fn install_entries(exe: &Path) -> std::io::Result<Vec<String>> {
    let dir = exe
        .parent()
        .ok_or_else(|| std::io::Error::other("exe has no parent folder"))?;
    let mut out = read_names(dir)?;
    let mut current = dir;
    for _ in 0..MARKER_PARENT_LEVELS {
        let Some(parent) = current.parent() else {
            break;
        };
        // A drive root (C:\) holds unrelated folders; stop above the game.
        if parent.parent().is_none() {
            break;
        }
        if let Ok(names) = read_names(parent) {
            out.extend(names);
        }
        current = parent;
    }
    Ok(out)
}

fn read_names(dir: &Path) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    Ok(names)
}

/// Names of active services (Win32 and kernel-driver services).
fn running_services() -> windows::core::Result<Vec<String>> {
    // SAFETY: plain Win32 call; the handle is closed below.
    let scm =
        unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_ENUMERATE_SERVICE) }?;
    let result = enum_services(scm);
    // SAFETY: `scm` was opened above and is closed exactly once.
    let _ = unsafe { CloseServiceHandle(scm) };
    result
}

fn enum_services(
    scm: windows::Win32::System::Services::SC_HANDLE,
) -> windows::core::Result<Vec<String>> {
    let mut names = Vec::new();
    let mut resume = 0u32;
    // u64 storage keeps the buffer aligned for ENUM_SERVICE_STATUS_PROCESSW.
    let mut storage: Vec<u64> = vec![0; 64 * 1024 / 8];
    loop {
        let mut needed = 0u32;
        let mut returned = 0u32;
        // SAFETY: the byte view covers exactly `storage`'s allocation.
        let bytes = unsafe {
            std::slice::from_raw_parts_mut(storage.as_mut_ptr().cast::<u8>(), storage.len() * 8)
        };
        // SAFETY: all out-pointers are valid; `bytes` is a writable buffer.
        let result = unsafe {
            EnumServicesStatusExW(
                scm,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32 | SERVICE_DRIVER,
                SERVICE_ACTIVE,
                Some(bytes),
                &mut needed,
                &mut returned,
                Some(&mut resume),
                PCWSTR::null(),
            )
        };
        let entries = storage.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>();
        for i in 0..returned as usize {
            // SAFETY: the API wrote `returned` entries at the start of the buffer,
            // and their string pointers point into the same buffer.
            let name = unsafe { (*entries.add(i)).lpServiceName.to_string() };
            if let Ok(name) = name {
                names.push(name);
            }
        }
        match result {
            Ok(()) => return Ok(names),
            Err(e) if is_code(&e, ERROR_MORE_DATA) => {
                let want = (needed as usize).div_ceil(8) + 1;
                if want > storage.len() {
                    storage = vec![0; want];
                }
            }
            Err(e) => return Err(e),
        }
    }
}

/// File names of loaded kernel drivers. Best effort: empty on failure.
fn loaded_drivers() -> Vec<String> {
    let mut bases: Vec<*mut c_void> = vec![std::ptr::null_mut(); 1024];
    let mut needed = 0u32;
    loop {
        let cb = (bases.len() * size_of::<*mut c_void>()) as u32;
        // SAFETY: `bases` is valid for `cb` bytes.
        if unsafe { EnumDeviceDrivers(bases.as_mut_ptr(), cb, &mut needed) }.is_err() {
            return Vec::new();
        }
        if needed <= cb {
            break;
        }
        bases = vec![std::ptr::null_mut(); needed as usize / size_of::<*mut c_void>() + 16];
    }
    let count = needed as usize / size_of::<*mut c_void>();
    let mut names = Vec::with_capacity(count);
    for &base in &bases[..count.min(bases.len())] {
        if base.is_null() {
            continue;
        }
        let mut buf = [0u16; 260];
        // SAFETY: `buf` is a valid output buffer; `base` came from EnumDeviceDrivers.
        let n = unsafe { GetDeviceDriverBaseNameW(base, &mut buf) };
        if n > 0 {
            names.push(from_wide(&buf));
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_this_process_without_errors() {
        let s = collect(std::process::id());
        assert!(s.errors.is_empty(), "errors: {:?}", s.errors);
        assert!(!s.processes.is_empty());
        assert!(!s.services.is_empty());
        assert!(!s.modules.is_empty());
        assert!(!s.install_entries.is_empty());
        assert!(s.target_name.to_lowercase().ends_with(".exe"));
    }

    #[test]
    fn missing_process_is_unverifiable() {
        // PIDs are multiples of 4; this one can't exist.
        let s = collect(0xFFFF_FFF3);
        assert!(!s.errors.is_empty());
    }

    #[test]
    fn install_entries_include_parents_but_not_drive_root() {
        let base = std::env::temp_dir().join(format!("ot-guard-entries-{}", std::process::id()));
        let exe_dir = base.join("Game").join("Binaries").join("Win64");
        std::fs::create_dir_all(&exe_dir).unwrap();
        std::fs::create_dir_all(base.join("Game").join("EasyAntiCheat")).unwrap();
        std::fs::write(exe_dir.join("game.exe"), b"").unwrap();
        let entries = install_entries(&exe_dir.join("game.exe")).unwrap();
        std::fs::remove_dir_all(&base).unwrap();
        assert!(entries.iter().any(|e| e == "game.exe"));
        assert!(entries.iter().any(|e| e == "EasyAntiCheat"));
    }
}
