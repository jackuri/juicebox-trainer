//! Checks for code bytes a cheat pack wants to write into a game.
//!
//! Packs are declarative data, and the app never runs arbitrary native code
//! written by a pack (or by the AI). A `patch` may only replace the bytes at a
//! site with a short run of plain instructions: data moves, arithmetic, NOPs,
//! compares, near jumps that stay inside the game's module, and `ret`.
//! Anything that could reach code outside the game or the OS directly (calls,
//! indirect jumps, `syscall`/`int`, far transfers, port I/O, privileged
//! instructions) is rejected. Code caves are planned for a later version.

use std::ops::Range;

use iced_x86::{
    Decoder, DecoderOptions, FlowControl, Formatter, Instruction, IntelFormatter, Mnemonic, OpKind,
};

use crate::error::{Error, Result};
use crate::process::Arch;

/// Longest replacement a patch may write.
pub const MAX_PATCH_LEN: usize = 32;

/// One decoded instruction.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodedInstr {
    /// Offset from the start of the bytes.
    pub offset: usize,
    pub len: usize,
    /// Intel syntax, e.g. `sub eax,edx`.
    pub text: String,
}

fn bitness(arch: Arch) -> Result<u32> {
    match arch {
        Arch::X86 => Ok(32),
        Arch::X64 => Ok(64),
        other => Err(Error::PatchRejected(format!(
            "code patches are not supported for {other} targets"
        ))),
    }
}

fn format(instr: &Instruction, f: &mut IntelFormatter) -> String {
    let mut s = String::new();
    f.format(instr, &mut s);
    s
}

/// Disassembles `bytes` as if placed at `ip`. Invalid bytes show as `(bad)`.
/// For display only; use [`check_patch`] before writing.
pub fn disassemble(bytes: &[u8], arch: Arch, ip: u64) -> Vec<DecodedInstr> {
    let Ok(bits) = bitness(arch) else {
        return Vec::new();
    };
    let mut decoder = Decoder::with_ip(bits, bytes, ip, DecoderOptions::NONE);
    let mut f = IntelFormatter::new();
    let mut out = Vec::new();
    let mut instr = Instruction::default();
    while decoder.can_decode() {
        let offset = decoder.position();
        decoder.decode_out(&mut instr);
        let text = if instr.is_invalid() {
            "(bad)".to_string()
        } else {
            format(&instr, &mut f)
        };
        out.push(DecodedInstr {
            offset,
            len: instr.len(),
            text,
        });
    }
    out
}

/// Ways into the kernel; iced files some of these under `Call`.
const SYSTEM_CALLS: &[Mnemonic] = &[
    Mnemonic::Syscall,
    Mnemonic::Sysenter,
    Mnemonic::Int,
    Mnemonic::Int1,
    Mnemonic::Int3,
    Mnemonic::Into,
];

/// Mnemonics rejected even though they don't change control flow.
const DENIED: &[Mnemonic] = &[
    Mnemonic::Hlt,
    Mnemonic::Cli,
    Mnemonic::Sti,
    Mnemonic::In,
    Mnemonic::Insb,
    Mnemonic::Insw,
    Mnemonic::Insd,
    Mnemonic::Out,
    Mnemonic::Outsb,
    Mnemonic::Outsw,
    Mnemonic::Outsd,
    Mnemonic::Rdmsr,
    Mnemonic::Wrmsr,
    Mnemonic::Lgdt,
    Mnemonic::Lidt,
    Mnemonic::Lldt,
    Mnemonic::Ltr,
    Mnemonic::Invlpg,
    Mnemonic::Wbinvd,
    Mnemonic::Invd,
    Mnemonic::Swapgs,
    Mnemonic::Clts,
    Mnemonic::Wrfsbase,
    Mnemonic::Wrgsbase,
];

fn touches_system_register(instr: &Instruction) -> bool {
    (0..instr.op_count()).any(|i| {
        instr.op_kind(i) == OpKind::Register && {
            let r = instr.op_register(i);
            r.is_cr() || r.is_dr() || r.is_tr() || r.is_segment_register()
        }
    })
}

/// Checks replacement bytes for a patch written at `ip`.
///
/// `module` is the address range of the game module the patch lives in; near
/// jumps must land inside it. Returns the decoded instructions on success.
pub fn check_patch(
    bytes: &[u8],
    arch: Arch,
    ip: u64,
    module: Range<u64>,
) -> Result<Vec<DecodedInstr>> {
    let reject = |why: String| Err(Error::PatchRejected(why));
    if bytes.is_empty() {
        return reject("the replacement is empty".into());
    }
    if bytes.len() > MAX_PATCH_LEN {
        return reject(format!(
            "the replacement is {} bytes; the limit is {MAX_PATCH_LEN}",
            bytes.len()
        ));
    }
    let bits = bitness(arch)?;
    let mut decoder = Decoder::with_ip(bits, bytes, ip, DecoderOptions::NONE);
    let mut f = IntelFormatter::new();
    let mut out = Vec::new();
    let mut instr = Instruction::default();
    while decoder.can_decode() {
        let offset = decoder.position();
        decoder.decode_out(&mut instr);
        if instr.is_invalid() {
            return reject(format!(
                "bytes at +{offset} are not a complete, valid {arch} instruction"
            ));
        }
        let text = format(&instr, &mut f);
        let fail = |why: &str| {
            Err(Error::PatchRejected(format!(
                "'{text}' at +{offset}: {why}"
            )))
        };
        if SYSTEM_CALLS.contains(&instr.mnemonic()) {
            return fail("system calls and interrupts are not allowed");
        }
        match instr.flow_control() {
            FlowControl::Next => {}
            FlowControl::Return => {
                if instr.mnemonic() != Mnemonic::Ret {
                    return fail("only a near ret is allowed");
                }
            }
            FlowControl::UnconditionalBranch | FlowControl::ConditionalBranch => {
                let near = matches!(
                    instr.op0_kind(),
                    OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64
                );
                if !near {
                    return fail("only near jumps with a fixed target are allowed");
                }
                let target = instr.near_branch_target();
                if !module.contains(&target) {
                    return fail(&format!(
                        "jumps to {target:#X}, outside the game module ({:#X}..{:#X})",
                        module.start, module.end
                    ));
                }
            }
            FlowControl::Call | FlowControl::IndirectCall => {
                return fail("calls are not allowed in a patch");
            }
            FlowControl::IndirectBranch => {
                return fail("indirect jumps are not allowed in a patch");
            }
            FlowControl::Interrupt => {
                return fail("system calls and interrupts are not allowed");
            }
            FlowControl::XbeginXabortXend | FlowControl::Exception => {
                return fail("this instruction is not allowed in a patch");
            }
        }
        if DENIED.contains(&instr.mnemonic()) || touches_system_register(&instr) {
            return fail("privileged or I/O instructions are not allowed");
        }
        out.push(DecodedInstr {
            offset,
            len: instr.len(),
            text,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODULE: Range<u64> = 0x1000..0x9000;

    fn ok(bytes: &[u8], arch: Arch) -> Vec<String> {
        check_patch(bytes, arch, 0x2000, MODULE)
            .unwrap_or_else(|e| panic!("{bytes:02X?} on {arch}: {e}"))
            .into_iter()
            .map(|i| i.text)
            .collect()
    }

    fn rejected(bytes: &[u8], arch: Arch) -> String {
        match check_patch(bytes, arch, 0x2000, MODULE) {
            Err(Error::PatchRejected(why)) => why,
            other => panic!("{bytes:02X?} on {arch} should be rejected, got {other:?}"),
        }
    }

    #[test]
    fn allows_common_trainer_patches() {
        for arch in [Arch::X86, Arch::X64] {
            assert_eq!(ok(&[0x90, 0x90], arch), ["nop", "nop"]);
            assert_eq!(ok(&[0x29, 0xD0], arch), ["sub eax,edx"]);
            // mov al,1 ; ret
            assert_eq!(ok(&[0xB0, 0x01, 0xC3], arch), ["mov al,1", "ret"]);
            // jz -> jmp short (inside the module)
            assert_eq!(ok(&[0xEB, 0x10], arch).len(), 1);
            // xor eax,eax
            assert_eq!(ok(&[0x31, 0xC0], arch), ["xor eax,eax"]);
            // mov dword ptr [ecx/rcx],0x3E7
            assert_eq!(ok(&[0xC7, 0x01, 0xE7, 0x03, 0x00, 0x00], arch).len(), 1);
        }
        // x64 multi-byte nop
        assert_eq!(ok(&[0x0F, 0x1F, 0x44, 0x00, 0x00], Arch::X64).len(), 1);
    }

    #[test]
    fn rejects_dangerous_instructions() {
        for arch in [Arch::X86, Arch::X64] {
            assert!(rejected(&[0xE8, 0, 0, 0, 0], arch).contains("calls"));
            assert!(rejected(&[0xFF, 0xD0], arch).contains("calls")); // call eax/rax
            assert!(rejected(&[0xFF, 0xE0], arch).contains("indirect")); // jmp eax/rax
            assert!(rejected(&[0xCD, 0x2E], arch).contains("system calls")); // int 2e
            assert!(rejected(&[0xCC], arch).contains("system calls")); // int3
            assert!(rejected(&[0x0F, 0x0B], arch).contains("not allowed")); // ud2
            assert!(rejected(&[0xF4], arch).contains("privileged")); // hlt
            assert!(rejected(&[0xEC], arch).contains("privileged")); // in al,dx
            assert!(rejected(&[0xCB], arch).contains("near ret")); // retf
            assert!(rejected(&[0x0F, 0x22, 0xC0], arch).contains("privileged")); // mov cr0,eax
            assert!(rejected(&[0x8E, 0xD8], arch).contains("privileged")); // mov ds,ax
        }
        let why = rejected(&[0x0F, 0x05], Arch::X64);
        assert!(why.contains("system calls"), "{why}"); // syscall
        assert!(rejected(&[0x0F, 0x34], Arch::X86).contains("system calls")); // sysenter
        assert!(rejected(&[0xEA, 0, 0, 0, 0, 0x23, 0], Arch::X86).contains("near jumps")); // jmp far
    }

    #[test]
    fn rejects_jumps_out_of_the_module() {
        // jmp rel32 to 0x2000 + 5 + 0x10000 = 0x12005, outside 0x1000..0x9000
        let why = rejected(&[0xE9, 0x00, 0x00, 0x01, 0x00], Arch::X64);
        assert!(why.contains("outside the game module"), "{why}");
    }

    #[test]
    fn rejects_truncated_empty_and_long() {
        assert!(rejected(&[0xC7, 0x01, 0xE7], Arch::X64).contains("complete"));
        assert!(rejected(&[], Arch::X64).contains("empty"));
        assert!(rejected(&[0x90; MAX_PATCH_LEN + 1], Arch::X64).contains("limit"));
        assert!(rejected(&[0x90], Arch::Arm64).contains("not supported"));
    }

    #[test]
    fn disassembles_for_display() {
        let d = disassemble(&[0x8B, 0x01, 0x29, 0xD0, 0x89, 0x01], Arch::X64, 0);
        let text: Vec<_> = d.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(text, ["mov eax,[rcx]", "sub eax,edx", "mov [rcx],eax"]);
        assert_eq!(d[1].offset, 2);
        let x86 = disassemble(&[0x8B, 0x01], Arch::X86, 0);
        assert_eq!(x86[0].text, "mov eax,[ecx]");
    }
}
