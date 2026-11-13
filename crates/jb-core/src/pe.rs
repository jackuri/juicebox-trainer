//! Reading a Windows program file (PE, 64-bit) from its bytes: sections, exported functions
//! and short stretches of code, for working out where an engine keeps things
//! ([`crate::unity`]). The file is only read; nothing is run.

use iced_x86::{Decoder, DecoderOptions, Instruction};

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section {
    pub rva: u32,
    pub virtual_size: u32,
    pub raw_offset: u32,
    pub raw_size: u32,
    pub executable: bool,
}

/// A 64-bit PE file's bytes and where its parts are.
#[derive(Debug)]
pub struct Pe<'a> {
    pub bytes: &'a [u8],
    /// Where the file prefers to be loaded (rip-relative targets are
    /// computed against it, then turned back into RVAs).
    pub image_base: u64,
    pub sections: Vec<Section>,
    exports: Vec<(String, u32)>,
}

impl<'a> Pe<'a> {
    /// Reads the headers and export names. `None` if it isn't a 64-bit PE.
    pub fn parse(bytes: &'a [u8]) -> Option<Self> {
        if bytes.get(..2)? != b"MZ" {
            return None;
        }
        let pe = u32_at(bytes, 0x3C)? as usize;
        if bytes.get(pe..pe + 4)? != b"PE\0\0" {
            return None;
        }
        let sections_n = usize::from(u16_at(bytes, pe + 6)?);
        let opt_size = usize::from(u16_at(bytes, pe + 20)?);
        let opt = pe + 24;
        if u16_at(bytes, opt)? != 0x20B {
            return None; // not PE32+
        }
        let image_base = u64::from_le_bytes(bytes.get(opt + 24..opt + 32)?.try_into().ok()?);
        let mut sections = Vec::with_capacity(sections_n);
        for i in 0..sections_n.min(96) {
            let s = opt + opt_size + i * 40;
            sections.push(Section {
                virtual_size: u32_at(bytes, s + 8)?,
                rva: u32_at(bytes, s + 12)?,
                raw_size: u32_at(bytes, s + 16)?,
                raw_offset: u32_at(bytes, s + 20)?,
                executable: u32_at(bytes, s + 36)? & 0x2000_0000 != 0,
            });
        }
        let mut pe = Self {
            bytes,
            image_base,
            sections,
            exports: Vec::new(),
        };
        pe.exports = pe.read_exports().unwrap_or_default();
        Some(pe)
    }

    /// Where an RVA's bytes are in the file.
    pub fn offset_of(&self, rva: u32) -> Option<usize> {
        self.sections.iter().find_map(|s| {
            let size = s.virtual_size.max(s.raw_size);
            (rva >= s.rva && rva < s.rva.saturating_add(size) && rva - s.rva < s.raw_size)
                .then(|| (s.raw_offset + (rva - s.rva)) as usize)
        })
    }

    /// The RVA of a file offset.
    pub fn rva_of(&self, offset: usize) -> Option<u32> {
        let offset = u32::try_from(offset).ok()?;
        self.sections.iter().find_map(|s| {
            (offset >= s.raw_offset && offset < s.raw_offset.saturating_add(s.raw_size))
                .then(|| s.rva + (offset - s.raw_offset))
        })
    }

    fn cstr(&self, rva: u32) -> Option<String> {
        let at = self.offset_of(rva)?;
        let rest = self.bytes.get(at..)?;
        let len = rest.iter().take(512).position(|&c| c == 0)?;
        Some(String::from_utf8_lossy(&rest[..len]).into_owned())
    }

    fn read_exports(&self) -> Option<Vec<(String, u32)>> {
        let pe = u32_at(self.bytes, 0x3C)? as usize;
        let dir_rva = u32_at(self.bytes, pe + 24 + 112)?;
        if dir_rva == 0 {
            return Some(Vec::new());
        }
        let d = self.offset_of(dir_rva)?;
        let names_n = u32_at(self.bytes, d + 24)? as usize;
        let funcs = self.offset_of(u32_at(self.bytes, d + 28)?)?;
        let names = self.offset_of(u32_at(self.bytes, d + 32)?)?;
        let ordinals = self.offset_of(u32_at(self.bytes, d + 36)?)?;
        let mut out = Vec::with_capacity(names_n.min(100_000));
        for i in 0..names_n.min(100_000) {
            let name = self.cstr(u32_at(self.bytes, names + i * 4)?)?;
            let ord = usize::from(u16_at(self.bytes, ordinals + i * 2)?);
            out.push((name, u32_at(self.bytes, funcs + ord * 4)?));
        }
        Some(out)
    }

    /// An exported function's RVA.
    pub fn export(&self, name: &str) -> Option<u32> {
        self.exports
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, r)| *r)
    }

    /// Up to `max` instructions from `rva`, past any `ret` inside the function: stops at the
    /// `int3` padding between functions (or bytes that aren't code).
    pub fn code(&self, rva: u32, max: usize) -> Vec<Instruction> {
        let Some(at) = self.offset_of(rva) else {
            return Vec::new();
        };
        let end = (at + max * 15).min(self.bytes.len());
        let mut d = Decoder::with_ip(
            64,
            &self.bytes[at..end],
            self.image_base + u64::from(rva),
            DecoderOptions::NONE,
        );
        let mut out = Vec::new();
        while d.can_decode() && out.len() < max {
            let i = d.decode();
            if i.mnemonic() == iced_x86::Mnemonic::Int3 || i.is_invalid() {
                break;
            }
            out.push(i);
        }
        out
    }

    /// A function's instructions, after following up to two leading `jmp`s (thunks).
    pub fn function(&self, rva: u32, max: usize) -> Vec<Instruction> {
        let mut rva = rva;
        for _ in 0..3 {
            let code = self.code(rva, max);
            match code.first() {
                Some(i) if i.is_jmp_near() => {
                    let Some(next) = self.rva_of_va(i.near_branch_target()) else {
                        return code;
                    };
                    rva = next;
                }
                _ => return code,
            }
        }
        self.code(rva, max)
    }

    /// An absolute address (as if loaded at the preferred base) as an RVA.
    pub fn rva_of_va(&self, va: u64) -> Option<u32> {
        u32::try_from(va.checked_sub(self.image_base)?).ok()
    }

    /// The RVA of the first `needle` (with its ending zero) in the file.
    pub fn find_cstr(&self, needle: &str) -> Option<u32> {
        let mut want = needle.as_bytes().to_vec();
        want.push(0);
        let at = self
            .bytes
            .windows(want.len())
            .position(|w| w == want.as_slice())?;
        self.rva_of(at)
    }

    /// Every `lea r64, [rip+…]` in the code that points at `target_rva`: the RVA of each
    /// instruction. Found by byte pattern (REX.W 8D, mod 00, rm 101), which is fast enough
    /// for game code of hundreds of megabytes.
    pub fn lea_refs(&self, target_rva: u32) -> Vec<u32> {
        let mut out = Vec::new();
        for s in self.sections.iter().filter(|s| s.executable) {
            let start = s.raw_offset as usize;
            let end = (start + s.raw_size as usize).min(self.bytes.len());
            let code = &self.bytes[start..end];
            let mut i = 0;
            while i + 7 <= code.len() {
                if (code[i] == 0x48 || code[i] == 0x4C)
                    && code[i + 1] == 0x8D
                    && code[i + 2] & 0xC7 == 0x05
                {
                    let disp =
                        i32::from_le_bytes([code[i + 3], code[i + 4], code[i + 5], code[i + 6]]);
                    let next = s.rva as i64 + i as i64 + 7;
                    if next + i64::from(disp) == i64::from(target_rva) {
                        out.push(s.rva + i as u32);
                    }
                }
                i += 1;
            }
        }
        out
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A tiny PE32+ with one executable section at RVA 0x1000 holding `code`, and exports.
    pub fn tiny_pe(code: &[u8], exports: &[(&str, u32)]) -> Vec<u8> {
        let mut b = vec![0u8; 0x400];
        b[0..2].copy_from_slice(b"MZ");
        b[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        b[0x86..0x88].copy_from_slice(&2u16.to_le_bytes()); // sections
        b[0x94..0x96].copy_from_slice(&0xF0u16.to_le_bytes()); // optional header size
        let opt = 0x98;
        b[opt..opt + 2].copy_from_slice(&0x20Bu16.to_le_bytes());
        b[opt + 24..opt + 32].copy_from_slice(&0x1_8000_0000u64.to_le_bytes());
        // Export directory at RVA 0x2000 (second section).
        b[opt + 112..opt + 116].copy_from_slice(&0x2000u32.to_le_bytes());
        let sec = opt + 0xF0;
        let text_len = code.len().next_multiple_of(0x200).max(0x200) as u32;
        let put =
            |b: &mut Vec<u8>, at: usize, name: &[u8], rva: u32, raw: u32, size: u32, flags: u32| {
                b[at..at + name.len()].copy_from_slice(name);
                b[at + 8..at + 12].copy_from_slice(&size.to_le_bytes());
                b[at + 12..at + 16].copy_from_slice(&rva.to_le_bytes());
                b[at + 16..at + 20].copy_from_slice(&size.to_le_bytes());
                b[at + 20..at + 24].copy_from_slice(&raw.to_le_bytes());
                b[at + 36..at + 40].copy_from_slice(&flags.to_le_bytes());
            };
        put(&mut b, sec, b".text", 0x1000, 0x400, text_len, 0x6000_0020);
        put(
            &mut b,
            sec + 40,
            b".edata",
            0x2000,
            0x400 + text_len,
            0x1000,
            0x4000_0040,
        );
        let mut text = code.to_vec();
        text.resize(text_len as usize, 0xCC);
        b.extend_from_slice(&text);
        // Export directory: names, functions, ordinals, strings.
        let mut e = vec![0u8; 0x1000];
        let n = exports.len() as u32;
        let w =
            |e: &mut Vec<u8>, at: usize, v: u32| e[at..at + 4].copy_from_slice(&v.to_le_bytes());
        w(&mut e, 20, n);
        w(&mut e, 24, n);
        w(&mut e, 28, 0x2100);
        w(&mut e, 32, 0x2200);
        w(&mut e, 36, 0x2300);
        let mut strings = 0x400usize;
        for (i, (name, rva)) in exports.iter().enumerate() {
            w(&mut e, 0x100 + i * 4, *rva);
            w(&mut e, 0x200 + i * 4, 0x2000 + strings as u32);
            e[0x300 + i * 2..0x302 + i * 2].copy_from_slice(&(i as u16).to_le_bytes());
            e[strings..strings + name.len()].copy_from_slice(name.as_bytes());
            strings += name.len() + 1;
        }
        b.extend_from_slice(&e);
        b
    }

    #[test]
    fn reads_exports_and_code() {
        // 0x1000: mov rax,[rcx+10h]; ret   0x1010: jmp 0x1000
        let mut code = vec![0xCCu8; 0x20];
        code[0..5].copy_from_slice(&[0x48, 0x8B, 0x41, 0x10, 0xC3]);
        code[0x10..0x15].copy_from_slice(&[0xE9, 0xEB, 0xFF, 0xFF, 0xFF]);
        let bytes = tiny_pe(&code, &[("get_name", 0x1000), ("thunk", 0x1010)]);
        let pe = Pe::parse(&bytes).unwrap();
        assert_eq!(pe.export("get_name"), Some(0x1000));
        assert_eq!(pe.export("nope"), None);
        let f = pe.function(pe.export("thunk").unwrap(), 8);
        assert_eq!(f[0].memory_displacement64(), 0x10);
        assert_eq!(f.len(), 2, "stops at the padding");
        assert!(Pe::parse(b"not a program").is_none());
    }

    #[test]
    fn finds_lea_references() {
        // 0x1000: lea rcx,[rip+0x1000] (-> 0x2007)
        let code = [0x48, 0x8D, 0x0D, 0x00, 0x10, 0x00, 0x00, 0xC3];
        let bytes = tiny_pe(&code, &[]);
        let pe = Pe::parse(&bytes).unwrap();
        assert_eq!(pe.lea_refs(0x2007), vec![0x1000]);
        assert!(pe.lea_refs(0x2008).is_empty());
    }
}
