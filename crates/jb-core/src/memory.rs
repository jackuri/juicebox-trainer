//! Memory regions of a target (from `VirtualQueryEx`).

use serde::{Deserialize, Serialize};

/// A contiguous region of the target's address space with one protection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    pub base: u64,
    pub size: u64,
    /// Raw `PAGE_*` protection flags.
    pub protect: u32,
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    /// Part of a mapped image (exe/DLL).
    pub image: bool,
}

const PAGE_NOACCESS: u32 = 0x01;
const PAGE_READONLY: u32 = 0x02;
const PAGE_READWRITE: u32 = 0x04;
const PAGE_WRITECOPY: u32 = 0x08;
const PAGE_EXECUTE: u32 = 0x10;
const PAGE_EXECUTE_READ: u32 = 0x20;
const PAGE_EXECUTE_READWRITE: u32 = 0x40;
const PAGE_EXECUTE_WRITECOPY: u32 = 0x80;
const PAGE_GUARD: u32 = 0x100;

/// Decodes `PAGE_*` flags into (readable, writable, executable).
/// Guard and no-access pages count as unreadable.
pub fn decode_protect(protect: u32) -> (bool, bool, bool) {
    if protect & PAGE_GUARD != 0 || protect & PAGE_NOACCESS != 0 || protect == 0 {
        return (false, false, false);
    }
    let base = protect & 0xFF;
    let readable = base
        & (PAGE_READONLY
            | PAGE_READWRITE
            | PAGE_WRITECOPY
            | PAGE_EXECUTE_READ
            | PAGE_EXECUTE_READWRITE
            | PAGE_EXECUTE_WRITECOPY)
        != 0;
    let writable = base
        & (PAGE_READWRITE | PAGE_WRITECOPY | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY)
        != 0;
    let executable = base
        & (PAGE_EXECUTE | PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY)
        != 0;
    (readable, writable, executable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_common_protections() {
        assert_eq!(decode_protect(PAGE_READWRITE), (true, true, false));
        assert_eq!(decode_protect(PAGE_EXECUTE_READ), (true, false, true));
        assert_eq!(decode_protect(PAGE_READONLY), (true, false, false));
        assert_eq!(decode_protect(PAGE_EXECUTE_READWRITE), (true, true, true));
        assert_eq!(
            decode_protect(PAGE_READWRITE | PAGE_GUARD),
            (false, false, false)
        );
        assert_eq!(decode_protect(PAGE_NOACCESS), (false, false, false));
        assert_eq!(decode_protect(0), (false, false, false));
    }
}
