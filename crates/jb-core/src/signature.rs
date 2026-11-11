//! Signature (AOB) maker: turns a code address into a byte pattern that
//! finds the same code again after the game is updated or restarted.
//!
//! Starting at the address, instructions are added one at a time until the
//! pattern matches exactly once in the module. Bytes that change between
//! builds are wildcarded: jump and call targets, RIP-relative and absolute
//! addresses, and immediates that point into the module. Struct offsets
//! (`[rcx+10h]`) and other constants are kept; they make good anchors.

use std::ops::Range;

use iced_x86::{Decoder, DecoderOptions, Formatter, IntelFormatter, OpKind, Register};
use serde::Serialize;

use crate::aob::Pattern;
use crate::code::DecodedInstr;
use crate::error::{Error, Result};
use crate::process::Arch;
use crate::session::{AttachedProcess, find_in_image};

/// Longest signature made.
pub const MAX_SIGNATURE_LEN: usize = 64;

/// Fewest fixed bytes in a signature, so it isn't a fluke match.
const MIN_FIXED: usize = 5;

/// A unique signature for a code address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Signature {
    /// E.g. `8B 01 29 D0 89 01 E8 ?? ?? ?? ??`.
    pub pattern: String,
    pub module: String,
    /// Where the signature starts, relative to the module base.
    pub module_offset: u64,
    pub len: usize,
    /// The instructions it covers.
    pub instructions: Vec<DecodedInstr>,
}

/// Decodes instructions at `ip` and masks the bytes that are likely to change.
/// Stops at the first invalid instruction or after `max` bytes.
pub(crate) fn masked_instructions(
    bytes: &[u8],
    arch: Arch,
    ip: u64,
    module: &Range<u64>,
    max: usize,
) -> Vec<(DecodedInstr, Vec<Option<u8>>)> {
    let bits = match arch {
        Arch::X86 => 32,
        Arch::X64 => 64,
        _ => return Vec::new(),
    };
    let mut decoder = Decoder::with_ip(bits, bytes, ip, DecoderOptions::NONE);
    let mut f = IntelFormatter::new();
    let mut out = Vec::new();
    let mut total = 0;
    while decoder.can_decode() {
        let offset = decoder.position();
        let instr = decoder.decode();
        if instr.is_invalid() || total + instr.len() > max {
            break;
        }
        let co = decoder.get_constant_offsets(&instr);
        let raw = &bytes[offset..offset + instr.len()];
        let mut mask: Vec<Option<u8>> = raw.iter().map(|b| Some(*b)).collect();
        let mut wild = |at: usize, size: usize| {
            for m in mask.iter_mut().skip(at).take(size) {
                *m = None;
            }
        };
        if co.has_displacement() {
            let absolute = (0..instr.op_count()).any(|i| instr.op_kind(i) == OpKind::Memory)
                && instr.memory_base() == Register::None
                && instr.memory_index() == Register::None;
            if instr.is_ip_rel_memory_operand() || absolute {
                wild(co.displacement_offset(), co.displacement_size());
            }
        }
        if co.has_immediate() {
            let branch = (0..instr.op_count()).any(|i| {
                matches!(
                    instr.op_kind(i),
                    OpKind::NearBranch16
                        | OpKind::NearBranch32
                        | OpKind::NearBranch64
                        | OpKind::FarBranch16
                        | OpKind::FarBranch32
                )
            });
            let points_into_module = co.immediate_size() >= 4 && {
                let at = co.immediate_offset();
                let mut v = [0u8; 8];
                v[..co.immediate_size().min(8)]
                    .copy_from_slice(&raw[at..at + co.immediate_size().min(8)]);
                module.contains(&u64::from_le_bytes(v))
            };
            if branch || points_into_module {
                wild(co.immediate_offset(), co.immediate_size());
            }
        }
        let mut text = String::new();
        f.format(&instr, &mut text);
        out.push((
            DecodedInstr {
                offset,
                len: instr.len(),
                text,
            },
            mask,
        ));
        total += instr.len();
    }
    out
}

fn render(mask: &[Option<u8>]) -> String {
    mask.iter()
        .map(|b| b.map_or_else(|| "??".to_string(), |v| format!("{v:02X}")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Makes the shortest unique signature starting at `address`, which must be
/// the start of an instruction inside a loaded module.
pub fn make(process: &AttachedProcess, address: u64) -> Result<Signature> {
    let module = process
        .refresh_modules()?
        .into_iter()
        .find(|m| m.contains(address))
        .ok_or_else(|| Error::InvalidAddress(format!("{address:#X} is not inside a module")))?;
    let image = process.module_image(&module)?;
    let (start, buf) = image
        .iter()
        .find(|(s, b)| (*s..*s + b.len() as u64).contains(&address))
        .ok_or(Error::Read { address, len: 1 })?;
    let from = (address - start) as usize;
    let bytes = &buf[from..buf.len().min(from + MAX_SIGNATURE_LEN + 16)];
    let instrs = masked_instructions(
        bytes,
        process.arch(),
        address,
        &(module.base..module.end()),
        MAX_SIGNATURE_LEN,
    );
    let mut mask: Vec<Option<u8>> = Vec::new();
    let mut used = Vec::new();
    for (d, m) in instrs {
        mask.extend(m);
        used.push(d);
        if mask.iter().filter(|b| b.is_some()).count() < MIN_FIXED {
            continue;
        }
        // Trailing wildcards add nothing.
        let end = mask.iter().rposition(|b| b.is_some()).map_or(0, |i| i + 1);
        let pattern = Pattern::from_masked(mask[..end].to_vec())?;
        let hits = find_in_image(&image, &pattern, 2);
        if hits == [address] {
            return Ok(Signature {
                pattern: render(&mask[..end]),
                module: module.name.clone(),
                module_offset: address - module.base,
                len: end,
                instructions: used,
            });
        }
    }
    Err(Error::SignatureNotUnique {
        address,
        len: MAX_SIGNATURE_LEN,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn masked(bytes: &[u8], arch: Arch, ip: u64) -> String {
        let module = 0x40_0000..0x50_0000;
        let m: Vec<Option<u8>> = masked_instructions(bytes, arch, ip, &module, 64)
            .into_iter()
            .flat_map(|(_, m)| m)
            .collect();
        render(&m)
    }

    #[test]
    fn wildcards_addresses_and_branches_only() {
        // mov eax,[rcx]; sub eax,edx; mov [rcx],eax
        assert_eq!(
            masked(&[0x8B, 0x01, 0x29, 0xD0, 0x89, 0x01], Arch::X64, 0x40_1000),
            "8B 01 29 D0 89 01"
        );
        // mov rcx,[rip+x]
        assert_eq!(
            masked(
                &[0x48, 0x8B, 0x0D, 0x10, 0x20, 0x00, 0x00],
                Arch::X64,
                0x40_1000
            ),
            "48 8B 0D ?? ?? ?? ??"
        );
        // call rel32; jz short
        assert_eq!(
            masked(&[0xE8, 1, 2, 3, 4, 0x74, 0x05], Arch::X64, 0x40_1000),
            "E8 ?? ?? ?? ?? 74 ??"
        );
        // mov ecx,[0x401000] (x86 absolute)
        assert_eq!(
            masked(&[0x8B, 0x0D, 0x00, 0x10, 0x40, 0x00], Arch::X86, 0x40_1000),
            "8B 0D ?? ?? ?? ??"
        );
        // push 0x401234 (an address in the module) vs push 0x3E7 (a constant)
        assert_eq!(
            masked(
                &[0x68, 0x34, 0x12, 0x40, 0x00, 0x68, 0xE7, 0x03, 0, 0],
                Arch::X86,
                0x40_1000
            ),
            "68 ?? ?? ?? ?? 68 E7 03 00 00"
        );
        // mov eax,[rcx+10h] keeps the struct offset
        assert_eq!(
            masked(&[0x8B, 0x41, 0x10], Arch::X64, 0x40_1000),
            "8B 41 10"
        );
    }

    #[test]
    fn stops_at_bad_bytes_and_length() {
        let module = 0x40_0000..0x50_0000;
        let out = masked_instructions(&[0x90, 0x90, 0xC7], Arch::X64, 0x40_1000, &module, 64);
        assert_eq!(out.len(), 2);
        let out = masked_instructions(&[0x90; 10], Arch::X64, 0x40_1000, &module, 4);
        assert_eq!(out.len(), 4);
    }
}
