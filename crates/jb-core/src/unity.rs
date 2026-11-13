//! Instant cheats for Unity IL2CPP games (v1.37.0), the way [`crate::engine_cheats`] does
//! them for Unreal: from what every game on the engine has, as ordinary pointer paths that
//! go in a declarative pack. Reading only; nothing in the game is changed or run.
//!
//! How the game is read:
//! - **Where IL2CPP keeps things** is worked out from `GameAssembly.dll` on disk: its exported
//!   helper functions (`il2cpp_class_get_name`, `il2cpp_field_get_offset`, …) are a few
//!   instructions each, and their memory offsets are the layout of this game's Unity version
//!   ([`Layout::from_game_assembly`]). Nothing is called; the code is only decoded.
//! - **Classes** come from IL2CPP's table of loaded classes (one pointer per class, filled as
//!   the game uses them), found the same way.
//! - **Game speed** is Unity's `Time.timeScale`: GameAssembly keeps the address of Unity's
//!   own `get_timeScale` once the game has used it; that function's few instructions say
//!   where the value is ([`time_scale_path`]).
//! - **The player's values** (`Health`, `Ammo`, `Money`, …) are looked for by name on objects
//!   reached from the game's own static fields (`PlayerManager.Instance.player.health`), only
//!   along paths that name the player, never an enemy's ([`find_unity`]).

use std::collections::{HashMap, HashSet};

use iced_x86::{Instruction, MemorySize, Mnemonic, OpKind, Register};

use crate::engine_cheats::{Control, EngineCheat, Found, MONEY, PLAYER_VALUES, around};
use crate::error::{Error, Result};
use crate::pe::Pe;
use crate::unreal::Mem;
use crate::value::ValueType;

fn bad(why: impl Into<String>) -> Error {
    Error::Debugger(why.into())
}

const NOTE: &str = "An instant engine cheat: it works in many Unity games, not every one. If it \
                    does nothing, this game keeps the value its own way (the AI helper can look).";

// Il2CppTypeEnum values.
const T_I2: u8 = 0x06;
const T_I4: u8 = 0x08;
const T_U4: u8 = 0x09;
const T_I8: u8 = 0x0A;
const T_R4: u8 = 0x0C;
const T_R8: u8 = 0x0D;
const T_CLASS: u8 = 0x12;
const T_GENERICINST: u8 = 0x15;
const T_OBJECT: u8 = 0x1C;
/// A class's own type parameter (`T m_Value` in `Reference<T>`).
const T_VAR: u8 = 0x13;
// FieldAttributes.
const F_STATIC: u16 = 0x10;
const F_LITERAL: u16 = 0x40;

/// Where this game's IL2CPP keeps things (byte offsets), read from GameAssembly.dll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub class_name: u64,
    pub class_namespace: u64,
    pub class_parent: u64,
    pub class_fields: u64,
    pub class_field_count: u64,
    pub class_static_fields: u64,
    pub field_name: u64,
    pub field_type: u64,
    pub field_offset: u64,
    pub field_size: u64,
    /// Il2CppType: the type byte; the field's attributes are the u16 before it.
    pub type_kind: u64,
    /// The RVA (in GameAssembly.dll) of the pointer to the table of loaded classes.
    pub class_table: u32,
}

fn is_gpr_base(i: &Instruction) -> bool {
    let b = i.memory_base();
    b.is_gpr64() && b != Register::RSP && b != Register::RIP
}

/// The displacement of the first load (`mov/movsx/movzx/movsxd r, [reg+disp]`).
fn first_load(code: &[Instruction]) -> Option<u64> {
    code.iter()
        .find(|i| {
            matches!(
                i.mnemonic(),
                Mnemonic::Mov | Mnemonic::Movsx | Mnemonic::Movzx | Mnemonic::Movsxd
            ) && i.op_count() == 2
                && i.op1_kind() == OpKind::Memory
                && is_gpr_base(i)
        })
        .map(Instruction::memory_displacement64)
}

/// `lea rax, [rcx+disp]`.
fn first_lea(code: &[Instruction]) -> Option<u64> {
    code.iter()
        .find(|i| i.mnemonic() == Mnemonic::Lea && is_gpr_base(i))
        .map(Instruction::memory_displacement64)
}

impl Layout {
    /// Reads the layout from GameAssembly.dll's bytes.
    pub fn from_game_assembly(pe: &Pe<'_>) -> Result<Self> {
        let f = |name: &str| -> Result<Vec<Instruction>> {
            let rva = pe
                .export(name)
                .ok_or_else(|| bad(format!("GameAssembly.dll has no {name}")))?;
            Ok(pe.function(rva, 120))
        };
        let missing = |what: &str| bad(format!("couldn't read where IL2CPP keeps {what}"));
        let class_name =
            first_load(&f("il2cpp_class_get_name")?).ok_or_else(|| missing("class names"))?;
        let class_namespace =
            first_load(&f("il2cpp_class_get_namespace")?).ok_or_else(|| missing("namespaces"))?;
        let class_parent =
            first_load(&f("il2cpp_class_get_parent")?).ok_or_else(|| missing("parents"))?;
        let field_name =
            first_load(&f("il2cpp_field_get_name")?).ok_or_else(|| missing("field names"))?;
        let field_type =
            first_load(&f("il2cpp_field_get_type")?).ok_or_else(|| missing("field types"))?;
        let field_offset =
            first_load(&f("il2cpp_field_get_offset")?).ok_or_else(|| missing("field offsets"))?;
        let type_kind =
            first_load(&f("il2cpp_type_get_type")?).ok_or_else(|| missing("type kinds"))?;
        if first_lea(&f("il2cpp_class_get_type")?).is_none() {
            return Err(missing("class types"));
        }

        // il2cpp_class_get_fields: `movzx ecx, word [rcx+count]`, `shl rcx, 5`, `mov rax, [rbx+fields]`.
        let fields_code = f("il2cpp_class_get_fields")?;
        let class_field_count = fields_code
            .iter()
            .find(|i| {
                i.mnemonic() == Mnemonic::Movzx
                    && i.op1_kind() == OpKind::Memory
                    && i.memory_size() == MemorySize::UInt16
                    && is_gpr_base(i)
            })
            .or_else(|| {
                fields_code.iter().find(|i| {
                    i.mnemonic() == Mnemonic::Cmp
                        && i.op0_kind() == OpKind::Memory
                        && i.memory_size() == MemorySize::UInt16
                        && is_gpr_base(i)
                })
            })
            .map(Instruction::memory_displacement64)
            .ok_or_else(|| missing("field counts"))?;
        let field_size = fields_code
            .iter()
            .find(|i| i.mnemonic() == Mnemonic::Shl && i.op1_kind() == OpKind::Immediate8)
            .map(|i| 1u64 << i.immediate8())
            .or_else(|| {
                fields_code
                    .iter()
                    .find(|i| {
                        i.mnemonic() == Mnemonic::Add && i.op1_kind() == OpKind::Immediate8to64
                    })
                    .map(|i| i.immediate8to64() as u64)
            })
            .filter(|s| (0x18..=0x40).contains(s))
            .ok_or_else(|| missing("field records"))?;
        let class_fields = fields_code
            .iter()
            .find(|i| {
                i.mnemonic() == Mnemonic::Mov
                    && i.op1_kind() == OpKind::Memory
                    && i.memory_size() == MemorySize::UInt64
                    && is_gpr_base(i)
                    && (0x20..0x400).contains(&i.memory_displacement64())
            })
            .map(Instruction::memory_displacement64)
            .ok_or_else(|| missing("field lists"))?;

        // il2cpp_field_static_get_value: `add rbx, [rax+static_fields]` (the field's class's
        // static data plus the field's offset).
        let static_code = f("il2cpp_field_static_get_value")?;
        let class_static_fields = static_code
            .iter()
            .find(|i| {
                i.mnemonic() == Mnemonic::Add
                    && i.op1_kind() == OpKind::Memory
                    && i.memory_size() == MemorySize::UInt64
                    && is_gpr_base(i)
                    && (0x40..0x400).contains(&i.memory_displacement64())
            })
            .map(Instruction::memory_displacement64)
            .ok_or_else(|| missing("static fields"))?;

        let class_table = class_table(pe).ok_or_else(|| missing("its table of classes"))?;
        Ok(Self {
            class_name,
            class_namespace,
            class_parent,
            class_fields,
            class_field_count,
            class_static_fields,
            field_name,
            field_type,
            field_offset,
            field_size,
            type_kind,
            class_table,
        })
    }
}

/// The table of loaded classes: `il2cpp_image_get_class` ends in a function that does
/// `mov rax, [rip+table]` then `lea rsi, [rax+index*8]`.
fn class_table(pe: &Pe<'_>) -> Option<u32> {
    let start = pe.export("il2cpp_image_get_class")?;
    let first = pe.function(start, 60);
    let mut places = vec![first.clone()];
    for i in &first {
        if (i.is_call_near() || i.is_jmp_near())
            && let Some(rva) = pe.rva_of_va(i.near_branch_target())
        {
            places.push(pe.code(rva, 60));
        }
    }
    for code in &places {
        for (n, i) in code.iter().enumerate() {
            if i.mnemonic() != Mnemonic::Mov
                || i.op1_kind() != OpKind::Memory
                || i.memory_base() != Register::RIP
                || !i.op0_register().is_gpr64()
            {
                continue;
            }
            let reg = i.op0_register();
            let indexed = code[n + 1..code.len().min(n + 4)].iter().any(|j| {
                j.op_count() == 2
                    && (j.op1_kind() == OpKind::Memory || j.op0_kind() == OpKind::Memory)
                    && j.memory_base() == reg
                    && j.memory_index() != Register::None
                    && j.memory_index_scale() == 8
            });
            if indexed {
                return pe.rva_of_va(i.memory_displacement64());
            }
        }
    }
    None
}

/// Where GameAssembly keeps the address of Unity's `Time.get_timeScale` once the game has
/// used it: the code that looks it up by name stores it with `mov [rip+cache], rax`.
pub fn time_scale_cache(pe: &Pe<'_>) -> Option<u32> {
    let name = pe.find_cstr("UnityEngine.Time::get_timeScale()")?;
    for at in pe.lea_refs(name) {
        let code = pe.code(at, 12);
        if let Some(store) = code.iter().find(|i| {
            i.mnemonic() == Mnemonic::Mov
                && i.op0_kind() == OpKind::Memory
                && i.memory_base() == Register::RIP
                && i.memory_size() == MemorySize::UInt64
        }) {
            return pe.rva_of_va(store.memory_displacement64());
        }
    }
    None
}

/// The code of one of Unity's own engine functions (an "internal call"), by its name
/// (`UnityEngine.Time::get_timeScale`), from UnityPlayer.dll. Unity lists the names in one array
/// and the functions in another just before it (with an empty entry between them), in the same
/// order; nothing is run.
pub fn native_function(pe: &Pe<'_>, name: &str) -> Option<u32> {
    let name_rva = pe.find_cstr(name)?;
    let want = (pe.image_base + u64::from(name_rva)).to_le_bytes();
    let b = pe.bytes;
    let entry = b.windows(8).position(|w| w == want)?;
    let is_name = |at: usize| -> bool {
        let Some(v) = b.get(at..at + 8) else {
            return false;
        };
        let v = u64::from_le_bytes(v.try_into().unwrap_or_default());
        pe.rva_of_va(v)
            .and_then(|r| pe.offset_of(r))
            .and_then(|o| b.get(o..o + 256))
            .is_some_and(|s| {
                let len = s.iter().position(|&c| c == 0).unwrap_or(0);
                len > 4 && s[..len].windows(2).any(|w| w == b"::")
            })
    };
    let mut start = entry;
    while start >= 8 && is_name(start - 8) {
        start -= 8;
    }
    let mut end = entry;
    while is_name(end) {
        end += 8;
    }
    let count = (end - start) / 8;
    let index = (entry - start) / 8;
    // The functions: `count` entries, then an empty one, then the names.
    if start < 8 + count * 8 || b.get(start - 8..start)? != [0u8; 8] {
        return None;
    }
    let funcs = start - 8 - count * 8;
    let code = |at: usize| -> Option<u32> {
        let v = u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?);
        let rva = pe.rva_of_va(v)?;
        pe.sections
            .iter()
            .any(|s| s.executable && rva >= s.rva && rva < s.rva + s.virtual_size.max(s.raw_size))
            .then_some(rva)
    };
    // Every entry must be code, or this isn't the table.
    if code(funcs).is_none() || code(funcs + (count - 1) * 8).is_none() {
        return None;
    }
    code(funcs + index * 8)
}

/// A pointer path: start at `base`, then for each offset read a pointer and add it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    pub base: u64,
    pub offsets: Vec<i64>,
}

impl Path {
    fn add(&self, n: i64) -> Self {
        let mut p = self.clone();
        match p.offsets.last_mut() {
            Some(last) => *last += n,
            None => p.base = p.base.wrapping_add(n as u64),
        }
        p
    }

    fn read(&self) -> Self {
        let mut p = self.clone();
        p.offsets.push(0);
        p
    }

    /// The address the path ends at, read now.
    pub fn resolve<M: Mem>(&self, mem: &M) -> Option<u64> {
        let mut at = self.base;
        for &o in &self.offsets {
            at = ptr(mem, at)?.wrapping_add(o as u64);
        }
        Some(at)
    }
}

fn ptr<M: Mem>(mem: &M, at: u64) -> Option<u64> {
    let b = mem.read(at, 8)?;
    let v = u64::from_le_bytes(b.try_into().ok()?);
    (0x10000..0x8000_0000_0000).contains(&v).then_some(v)
}

/// Follows Unity's native `get_timeScale` (at `func`, in the running game) to the address of
/// the value: registers are tracked through `mov`/`lea`/`add` and one level of calls, until
/// `movss xmm0, [reg+X]`.
pub fn time_scale_path<M: Mem>(mem: &M, func: u64) -> Option<Path> {
    fn run<M: Mem>(mem: &M, at: u64, depth: usize) -> (Option<Path>, Option<Path>) {
        let Some(bytes) = mem.read(at, 160) else {
            return (None, None);
        };
        let mut d = iced_x86::Decoder::with_ip(64, &bytes, at, iced_x86::DecoderOptions::NONE);
        let mut regs: HashMap<Register, Path> = HashMap::new();
        let full = |r: Register| r.full_register();
        for _ in 0..40 {
            if !d.can_decode() {
                break;
            }
            let i = d.decode();
            match i.mnemonic() {
                Mnemonic::Ret => return (None, regs.get(&Register::RAX).cloned()),
                Mnemonic::Jmp if i.is_jmp_near() && depth < 2 => {
                    return run(mem, i.near_branch_target(), depth + 1);
                }
                Mnemonic::Call if i.is_call_near() && depth < 2 => {
                    let (value, rax) = run(mem, i.near_branch_target(), depth + 1);
                    if value.is_some() {
                        return (value, None);
                    }
                    regs.clear();
                    if let Some(p) = rax {
                        regs.insert(Register::RAX, p);
                    }
                }
                Mnemonic::Movss | Mnemonic::Movd | Mnemonic::Vmovss
                    if i.op0_register() == Register::XMM0
                        && i.op_count() >= 2
                        && i.op1_kind() == OpKind::Memory =>
                {
                    let disp = i.memory_displacement64() as i64;
                    if i.memory_base() == Register::RIP {
                        return (
                            Some(Path {
                                base: i.memory_displacement64(),
                                offsets: Vec::new(),
                            }),
                            None,
                        );
                    }
                    if let Some(p) = regs.get(&full(i.memory_base())) {
                        return (Some(p.add(disp)), None);
                    }
                }
                Mnemonic::Mov
                    if i.op0_kind() == OpKind::Register && i.op0_register().is_gpr64() =>
                {
                    let dst = i.op0_register();
                    let new = match i.op1_kind() {
                        OpKind::Memory
                            if i.memory_base() == Register::RIP
                                && i.memory_index() == Register::None =>
                        {
                            Some(Path {
                                base: i.memory_displacement64(),
                                offsets: vec![0],
                            })
                        }
                        OpKind::Memory if i.memory_index() == Register::None => regs
                            .get(&full(i.memory_base()))
                            .map(|p| p.add(i.memory_displacement64() as i64).read()),
                        OpKind::Register => regs.get(&full(i.op1_register())).cloned(),
                        _ => None,
                    };
                    match new {
                        Some(p) => {
                            regs.insert(dst, p);
                        }
                        None => {
                            regs.remove(&dst);
                        }
                    }
                }
                Mnemonic::Lea
                    if i.op0_register().is_gpr64() && i.memory_index() == Register::None =>
                {
                    let dst = i.op0_register();
                    let new = if i.memory_base() == Register::RIP {
                        Some(Path {
                            base: i.memory_displacement64(),
                            offsets: Vec::new(),
                        })
                    } else {
                        regs.get(&full(i.memory_base()))
                            .map(|p| p.add(i.memory_displacement64() as i64))
                    };
                    match new {
                        Some(p) => {
                            regs.insert(dst, p);
                        }
                        None => {
                            regs.remove(&dst);
                        }
                    }
                }
                Mnemonic::Add
                    if i.op0_kind() == OpKind::Register
                        && i.op1_kind() == OpKind::Immediate8to64
                        || i.op1_kind() == OpKind::Immediate32to64 =>
                {
                    if let Some(p) = regs.get(&i.op0_register()).cloned() {
                        regs.insert(i.op0_register(), p.add(i.immediate(1) as i64));
                    }
                }
                _ => {
                    if i.op0_kind() == OpKind::Register && i.op0_register().is_gpr64() {
                        regs.remove(&i.op0_register());
                    }
                }
            }
        }
        (None, None)
    }
    run(mem, func, 0).0
}

// ----- the running game's classes ----------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub kind: u8,
    pub attrs: u16,
    pub offset: i32,
}

impl Field {
    fn is_static(&self) -> bool {
        self.attrs & F_STATIC != 0
    }
}

#[derive(Debug, Clone)]
pub struct Class {
    pub address: u64,
    pub name: String,
    pub namespace: String,
    pub parent: Option<u64>,
    pub fields: Vec<Field>,
    pub static_fields: Option<u64>,
}

impl Class {
    pub fn full_name(&self) -> String {
        if self.namespace.is_empty() {
            self.name.clone()
        } else {
            format!("{}.{}", self.namespace, self.name)
        }
    }
}

/// The running game's IL2CPP classes, read through `mem`.
pub struct Runtime<'m, M: Mem> {
    mem: &'m M,
    layout: Layout,
    /// Where GameAssembly.dll is loaded.
    module_base: u64,
    classes: std::cell::RefCell<HashMap<u64, Option<Class>>>,
}

impl<'m, M: Mem> std::fmt::Debug for Runtime<'m, M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime")
            .field("layout", &self.layout)
            .finish_non_exhaustive()
    }
}

fn cstr<M: Mem>(mem: &M, at: u64) -> Option<String> {
    let b = mem.read(at, 128)?;
    let len = b.iter().position(|&c| c == 0)?;
    let s = std::str::from_utf8(&b[..len]).ok()?;
    s.chars().all(|c| !c.is_control()).then(|| s.to_string())
}

impl<'m, M: Mem> Runtime<'m, M> {
    pub fn new(mem: &'m M, layout: Layout, module_base: u64) -> Self {
        Self {
            mem,
            layout,
            module_base,
            classes: Default::default(),
        }
    }

    /// The table of loaded classes and its address path's start.
    fn table(&self) -> Option<u64> {
        ptr(
            self.mem,
            self.module_base + u64::from(self.layout.class_table),
        )
    }

    /// Every loaded class among the first `count` (the metadata's number of classes):
    /// `(index, class address)`.
    pub fn loaded(&self, count: usize) -> Vec<(usize, u64)> {
        let Some(table) = self.table() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        const CHUNK: usize = 4096;
        for start in (0..count).step_by(CHUNK) {
            let n = CHUNK.min(count - start);
            let Some(b) = self.mem.read(table + start as u64 * 8, n * 8) else {
                continue;
            };
            for (k, c) in b.as_chunks::<8>().0.iter().enumerate() {
                let v = u64::from_le_bytes(*c);
                if (0x10000..0x8000_0000_0000).contains(&v) {
                    out.push((start + k, v));
                }
            }
        }
        out
    }

    /// The class at `address` (cached).
    pub fn class(&self, address: u64) -> Option<Class> {
        if let Some(c) = self.classes.borrow().get(&address) {
            return c.clone();
        }
        let c = self.read_class(address);
        self.classes.borrow_mut().insert(address, c.clone());
        c
    }

    fn read_class(&self, at: u64) -> Option<Class> {
        let l = &self.layout;
        let name = cstr(self.mem, ptr(self.mem, at + l.class_name)?)?;
        if name.is_empty() {
            return None;
        }
        let namespace = ptr(self.mem, at + l.class_namespace)
            .and_then(|p| cstr(self.mem, p))
            .unwrap_or_default();
        let parent = ptr(self.mem, at + l.class_parent);
        let count = usize::from(self.mem.u16(at + l.class_field_count)?);
        let mut fields = Vec::new();
        if count > 0 && count < 4096 {
            let list = ptr(self.mem, at + l.class_fields)?;
            let raw = self.mem.read(list, count * l.field_size as usize)?;
            for k in 0..count {
                let r = &raw[k * l.field_size as usize..];
                let word = |o: u64| {
                    u64::from_le_bytes(r[o as usize..o as usize + 8].try_into().unwrap_or_default())
                };
                let Some(fname) = cstr(self.mem, word(l.field_name)) else {
                    continue;
                };
                let offset = i32::from_le_bytes(
                    r[l.field_offset as usize..l.field_offset as usize + 4]
                        .try_into()
                        .unwrap_or_default(),
                );
                let Some(ty) = self.mem.read(word(l.field_type), l.type_kind as usize + 1) else {
                    continue;
                };
                let kind = ty[l.type_kind as usize];
                let attrs = u16::from_le_bytes([
                    ty[l.type_kind as usize - 2],
                    ty[l.type_kind as usize - 1],
                ]);
                fields.push(Field {
                    name: fname,
                    kind,
                    attrs,
                    offset,
                });
            }
        }
        Some(Class {
            address: at,
            name,
            namespace,
            parent,
            fields,
            static_fields: ptr(self.mem, at + l.class_static_fields),
        })
    }

    /// The class and its parents, nearest first (at most 16).
    fn chain(&self, class: u64) -> Vec<Class> {
        let mut out = Vec::new();
        let mut at = Some(class);
        while let Some(a) = at {
            if out.len() >= 16 {
                break;
            }
            let Some(c) = self.class(a) else { break };
            at = c.parent;
            out.push(c);
        }
        out
    }

    /// An object's instance fields (its class's and its parents'), and the class names.
    fn object(&self, object: u64) -> Option<(Vec<Field>, Vec<String>)> {
        let class = ptr(self.mem, object)?;
        let chain = self.chain(class);
        if chain.is_empty() {
            return None;
        }
        let names = chain.iter().map(|c| c.name.clone()).collect();
        let fields = chain
            .into_iter()
            .flat_map(|c| c.fields)
            .filter(|f| !f.is_static() && f.offset > 0)
            .collect();
        Some((fields, names))
    }
}

// ----- finding the cheats --------------------------------------------------------------------

/// Words of a name: `m_LocalPlayerHealth` → `local`, `player`, `health`.
fn words(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in name.chars() {
        if !c.is_ascii_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
            continue;
        }
        if c.is_ascii_uppercase() && prev_lower && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        cur.push(c.to_ascii_lowercase());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// A field name as the name lists have it: `<Health>k__BackingField`, `m_Health`, `_health`
/// → `health`.
fn norm(name: &str) -> String {
    let inner = name
        .strip_prefix('<')
        .and_then(|r| r.split('>').next())
        .unwrap_or(name);
    let inner = inner.strip_prefix("m_").unwrap_or(inner);
    inner
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

const GOOD: &[&str] = &[
    "player",
    "local",
    "hero",
    "character",
    "avatar",
    "protagonist",
    "me",
    "self",
];
/// Words on a path that mean it leads to someone or something else.
const BAD: &[&str] = &[
    "enemy",
    "enemies",
    "ai",
    "npc",
    "npcs",
    "boss",
    "monster",
    "creature",
    "target",
    "turret",
    "spawner",
    "debug",
    "dev",
    "test",
    "gui",
    "ui",
    "hud",
    "preview",
    "tuning",
    "settings",
    "config",
    "preset",
    "template",
    "prefab",
    "editor",
    "cheat",
    "other",
    "others",
    "remote",
    "last",
    "previous",
    "spectator",
    "observer",
    "observers",
    "event",
    "events",
    "delegate",
    "callback",
    "subscription",
    "subscriptions",
    "cache",
    "pool",
];

fn value_type(kind: u8) -> Option<ValueType> {
    Some(match kind {
        T_R4 => ValueType::F32,
        T_R8 => ValueType::F64,
        T_I4 => ValueType::I32,
        T_U4 => ValueType::U32,
        T_I8 => ValueType::I64,
        T_I2 => ValueType::I16,
        _ => return None,
    })
}

fn read_num<M: Mem>(mem: &M, at: u64, ty: ValueType) -> Option<f64> {
    let b = mem.read(at, ty.size(crate::Arch::X64))?;
    Some(match ty {
        ValueType::F32 => f64::from(f32::from_le_bytes(b.try_into().ok()?)),
        ValueType::F64 => f64::from_le_bytes(b.try_into().ok()?),
        ValueType::I32 => f64::from(i32::from_le_bytes(b.try_into().ok()?)),
        ValueType::U32 => f64::from(u32::from_le_bytes(b.try_into().ok()?)),
        ValueType::I64 => i64::from_le_bytes(b.try_into().ok()?) as f64,
        ValueType::I16 => f64::from(i16::from_le_bytes(b.try_into().ok()?)),
        _ => return None,
    })
    .filter(|v| v.is_finite())
}

/// A value found on an object reached from a static field.
#[derive(Debug, Clone)]
struct Hit {
    offsets: Vec<i64>,
    ty: ValueType,
    now: f64,
    /// `PlayerManager.Instance → player → Health`.
    found_in: String,
    /// The value name it matched (`health`, `gold`).
    field: String,
    /// Its field name in the game (`m_ServerHealth`).
    display: String,
    /// The object's numbers named like a maximum (`maxhealth`, 100).
    maxes: Vec<(String, f64)>,
    score: i32,
    depth: usize,
}

/// Finds instant cheats in a running Unity IL2CPP game.
///
/// - `rt`: the game's classes; `class_count`: how many the metadata lists.
/// - `ga_table`: the module-relative start of class paths (`GameAssembly.dll+layout.class_table`).
/// - `time_scale`: the path to `Time.timeScale` and the module it starts in, if found.
pub fn find_unity<M: Mem>(
    rt: &Runtime<'_, M>,
    class_count: usize,
    time_scale: Option<(String, u64, Path)>,
) -> Result<Found> {
    let mem = rt.mem;
    let mut cheats = Vec::new();
    let mut not_found = Vec::new();

    match time_scale.and_then(|(module, module_base, p)| {
        let at = p.resolve(mem)?;
        let now = read_num(mem, at, ValueType::F32)?;
        ((0.0..=100.0).contains(&now)).then_some((module, module_base, p, now))
    }) {
        Some((module, module_base, p, now)) => cheats.push(EngineCheat {
            id: "game_speed".into(),
            name: "Game speed".into(),
            category: "World".into(),
            description: format!(
                "1 is normal; lower slows the game down, higher speeds it up. {NOTE}"
            ),
            ty: ValueType::F32,
            offsets: p.offsets,
            base: Some((module, p.base - module_base)),
            found_in: "Time.timeScale".into(),
            now,
            control: Control::Slider {
                min: 0.1,
                max: 5.0,
                step: 0.1,
                default: 1.0,
            },
        }),
        None => not_found.push("Game speed".into()),
    }

    let hits = player_values(rt, class_count);
    let module_rel = u64::from(rt.layout.class_table);
    for (id, name, names, max_names) in PLAYER_VALUES {
        let best = hits
            .iter()
            .filter(|h| names.contains(&h.field.as_str()) && h.score > 0)
            .filter(|h| {
                if *id == "ammo" {
                    h.now >= 0.0
                } else {
                    h.now > 0.0
                }
            })
            .max_by_key(|h| (h.score, std::cmp::Reverse(h.depth)));
        match best {
            Some(h) => {
                let max = h
                    .maxes
                    .iter()
                    .find(|(m, v)| max_names.contains(&m.as_str()) && *v >= h.now && *v > 0.0)
                    .map(|(_, v)| *v);
                let keep_at = max.or((h.now > 0.0).then_some(h.now));
                cheats.push(EngineCheat {
                    id: (*id).into(),
                    name: (*name).into(),
                    category: "Player".into(),
                    description: format!(
                        "Keeps {} at {}. {NOTE}",
                        norm_display(&h.display),
                        keep_at.map_or("where it is".into(), |v| format!("{v}"))
                    ),
                    ty: h.ty,
                    offsets: h.offsets.clone(),
                    base: Some(("GameAssembly.dll".into(), module_rel)),
                    found_in: h.found_in.clone(),
                    now: h.now,
                    control: Control::Keep { at: keep_at },
                });
            }
            None => {
                let what = name.trim_start_matches("Infinite ");
                let mut c = what.chars();
                not_found.push(c.next().map_or_else(String::new, |f| {
                    f.to_ascii_uppercase().to_string() + c.as_str()
                }));
            }
        }
    }
    let money = hits
        .iter()
        .filter(|h| MONEY.contains(&h.field.as_str()) && h.score >= 0 && h.now >= 0.0)
        .max_by_key(|h| (h.score, std::cmp::Reverse(h.depth)));
    match money {
        Some(h) => cheats.push(EngineCheat {
            id: "money".into(),
            name: format!("Set {}", norm_display(&h.display)),
            category: "Items".into(),
            description: format!(
                "Type how much {} you want. {NOTE}",
                norm_display(&h.display)
            ),
            ty: h.ty,
            offsets: h.offsets.clone(),
            base: Some(("GameAssembly.dll".into(), module_rel)),
            found_in: h.found_in.clone(),
            now: h.now,
            control: Control::Set,
        }),
        None => not_found.push("Money".into()),
    }
    let _ = around; // movement isn't standard in Unity

    Ok(Found {
        global: 0,
        cheats,
        not_found,
    })
}

/// `<Gold>k__BackingField` → `Gold`, `m_credits` → `credits`.
fn norm_display(name: &str) -> String {
    let inner = name
        .strip_prefix('<')
        .and_then(|r| r.split('>').next())
        .unwrap_or(name);
    inner
        .trim_start_matches("m_")
        .trim_start_matches('_')
        .to_string()
}

/// A field name as a value name: `serverhealth` and `currenthealth` count as `health`.
fn value_name<'a>(n: &'a str, list: &[&'static str]) -> Option<&'static str> {
    let mut n: &'a str = n;
    loop {
        if let Some(found) = list.iter().find(|x| **x == n) {
            return Some(found);
        }
        n = n
            .strip_prefix("server")
            .or_else(|| n.strip_prefix("current"))
            .or_else(|| n.strip_prefix("player"))?;
    }
}

/// Fields that hold a value inside a wrapper object (`ReactiveProperty<float>.value`).
const WRAPPED: &[&str] = &["value", "internalvalue", "currentvalue", "val"];

/// Words in a value's own name that mean it isn't the live value.
const NOT_LIVE: &[&str] = &[
    "default", "base", "initial", "start", "starting", "original", "min", "last", "previous",
    "regen", "old", "saved", "target", "display", "shown",
];

/// A number on `object` in field `f`, directly or inside a wrapper object:
/// `(offsets added, type, now)`.
fn number_at<M: Mem>(
    rt: &Runtime<'_, M>,
    object: u64,
    f: &Field,
) -> Option<(Vec<i64>, ValueType, f64)> {
    let mem = rt.mem;
    if let Some(ty) = value_type(f.kind) {
        let now = read_num(mem, object + f.offset as u64, ty)?;
        return Some((vec![i64::from(f.offset)], ty, now));
    }
    if !matches!(f.kind, T_CLASS | T_GENERICINST) {
        return None;
    }
    let inner = ptr(mem, object + f.offset as u64)?;
    let (fields, _) = rt.object(inner)?;
    let v = fields
        .iter()
        .find(|w| WRAPPED.contains(&norm(&w.name).as_str()) && value_type(w.kind).is_some())?;
    let ty = value_type(v.kind)?;
    let now = read_num(mem, inner + v.offset as u64, ty)?;
    Some((vec![i64::from(f.offset), i64::from(v.offset)], ty, now))
}

/// How far from a static field to look (fields followed), and how much "cost" a path may add up.
const MAX_DEPTH: usize = 10;
const MAX_COST: u32 = 20;
/// Objects looked at, at most.
const MAX_OBJECTS: usize = 120_000;

/// Classes that never hold a game's own values (engine and library types).
fn library(namespace: &str) -> bool {
    [
        "System",
        "UnityEngine",
        "Unity.",
        "TMPro",
        "Mono",
        "Cysharp",
        "DG.Tweening",
        "Newtonsoft",
        "MessagePack",
    ]
    .iter()
    .any(|p| namespace.starts_with(p))
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Node {
    cost: std::cmp::Reverse<u32>,
    seq: std::cmp::Reverse<u64>,
    object: u64,
    depth: usize,
    path: Vec<i64>,
    words: Vec<String>,
    desc: String,
}

/// Every named number on objects reached from static fields, with how "player" its path is.
/// Paths that name the player or the local player are looked at first; paths through an enemy,
/// a menu, settings or another player are never followed.
fn player_values<M: Mem>(rt: &Runtime<'_, M>, class_count: usize) -> Vec<Hit> {
    let mem = rt.mem;
    let l = rt.layout;
    let all_names: Vec<&'static str> = PLAYER_VALUES
        .iter()
        .flat_map(|(_, _, names, _)| names.iter().copied())
        .chain(MONEY.iter().copied())
        .collect();
    let max_names: Vec<&'static str> = PLAYER_VALUES
        .iter()
        .flat_map(|(_, _, _, m)| m.iter().copied())
        .collect();
    let edge_cost = |w: &[String]| -> Option<u32> {
        if w.iter().any(|x| BAD.contains(&x.as_str())) {
            None
        } else if w.iter().any(|x| GOOD.contains(&x.as_str())) {
            Some(0)
        } else {
            Some(2)
        }
    };

    let mut heap = std::collections::BinaryHeap::new();
    let mut seq = 0u64;
    let mut seen: HashSet<u64> = HashSet::new();
    for (index, class_at) in rt.loaded(class_count) {
        let Some(class) = rt.class(class_at) else {
            continue;
        };
        let Some(statics) = class.static_fields else {
            continue;
        };
        if class.name.contains('<') || library(&class.namespace) {
            continue;
        }
        for f in &class.fields {
            if !f.is_static() || f.attrs & F_LITERAL != 0 || f.offset < 0 {
                continue;
            }
            if !matches!(f.kind, T_CLASS | T_GENERICINST | T_OBJECT) {
                continue;
            }
            let mut w = words(&class.name);
            w.extend(words(&f.name));
            let Some(cost) = edge_cost(&w) else { continue };
            let Some(object) = ptr(mem, statics + f.offset as u64) else {
                continue;
            };
            if !seen.insert(object) {
                continue;
            }
            seq += 1;
            heap.push(Node {
                cost: std::cmp::Reverse(cost),
                seq: std::cmp::Reverse(seq),
                object,
                depth: 0,
                path: vec![
                    index as i64 * 8,
                    l.class_static_fields as i64,
                    i64::from(f.offset),
                ],
                words: w,
                desc: format!("{}.{}", class.full_name(), f.name),
            });
        }
    }

    let mut hits = Vec::new();
    let mut looked = 0;
    while let Some(node) = heap.pop() {
        looked += 1;
        if looked > MAX_OBJECTS {
            break;
        }
        let Some(class) = ptr(mem, node.object) else {
            continue;
        };
        let chain = rt.chain(class);
        let Some(first) = chain.first() else { continue };
        if library(&first.namespace) {
            continue;
        }
        // Often a settings asset rather than the live game (but some games keep live values in
        // them too): counts against it, the words on its path decide.
        let asset = chain.iter().any(|c| c.name == "ScriptableObject");
        let fields: Vec<Field> = chain
            .iter()
            .flat_map(|c| c.fields.iter().cloned())
            .filter(|f| !f.is_static() && f.offset > 0)
            .collect();
        // A Unity object that was destroyed (its native side is gone).
        if let Some(cached) = fields.iter().find(|f| f.name == "m_CachedPtr")
            && ptr(mem, node.object + cached.offset as u64).is_none()
        {
            continue;
        }
        let mut words_here = node.words.clone();
        words_here.extend(words(&first.name));

        let maxes: Vec<(String, f64)> = fields
            .iter()
            .filter_map(|f| {
                let n = norm(&f.name);
                let m = value_name(&n, &max_names)?;
                Some((m.to_string(), number_at(rt, node.object, f)?.2))
            })
            .collect();
        for f in &fields {
            let n = norm(&f.name);
            let Some(name) = value_name(&n, &all_names) else {
                continue;
            };
            let own = words(&f.name);
            if own.iter().any(|x| NOT_LIVE.contains(&x.as_str())) {
                continue;
            }
            let Some((extra, ty, now)) = number_at(rt, node.object, f) else {
                continue;
            };
            if now.abs() > 1e9 {
                continue;
            }
            let good = words_here
                .iter()
                .chain(own.iter())
                .filter(|x| GOOD.contains(&x.as_str()))
                .count() as i32;
            // The host's copy of a networked value is the one that counts.
            let server = own.iter().any(|x| x == "server" || x == "authority");
            let score = good * 3 + if server { 2 } else { 0 }
                - if asset { 1 } else { 0 }
                - (node.cost.0 as i32) / 2;
            let mut offsets = node.path.clone();
            offsets.extend(extra);
            hits.push(Hit {
                offsets,
                ty,
                now,
                found_in: format!("{} → {}", node.desc, f.name),
                field: name.to_string(),
                display: f.name.clone(),
                maxes: maxes.clone(),
                score: if good > 0 { score.max(1) } else { 0 },
                depth: node.depth,
            });
        }
        if node.depth >= MAX_DEPTH {
            continue;
        }
        for f in &fields {
            if !matches!(f.kind, T_CLASS | T_OBJECT | T_GENERICINST | T_VAR)
                || f.name == "m_CachedPtr"
            {
                continue;
            }
            let Some(next) = ptr(mem, node.object + f.offset as u64) else {
                continue;
            };
            let next_class = ptr(mem, next).and_then(|k| rt.class(k));
            let mut w = words(&f.name);
            if let Some(c) = &next_class {
                w.extend(words(&c.name));
            }
            let Some(step) = edge_cost(&w) else { continue };
            let cost = node.cost.0 + step;
            if cost > MAX_COST || !seen.insert(next) {
                continue;
            }
            let mut words2 = node.words.clone();
            words2.extend(words(&f.name));
            let mut p2 = node.path.clone();
            p2.push(i64::from(f.offset));
            seq += 1;
            heap.push(Node {
                cost: std::cmp::Reverse(cost),
                seq: std::cmp::Reverse(seq),
                object: next,
                depth: node.depth + 1,
                path: p2,
                words: words2,
                desc: format!("{} → {}", node.desc, f.name),
            });
        }
    }
    hits
}

/// Finds the instant cheats of the attached Unity IL2CPP game (`exe` is its exe on disk).
/// Reads GameAssembly.dll and the metadata from the game folder, and the game's memory.
pub fn find_in_process(
    process: &crate::session::AttachedProcess,
    exe: &std::path::Path,
) -> Result<Found> {
    let folder = exe
        .parent()
        .ok_or_else(|| bad("can't see the game's folder"))?;
    let ga_file = folder.join("GameAssembly.dll");
    let bytes = std::fs::read(&ga_file).map_err(|e| bad(format!("{}: {e}", ga_file.display())))?;
    let pe =
        Pe::parse(&bytes).ok_or_else(|| bad("GameAssembly.dll isn't a 64-bit program file"))?;
    let layout = Layout::from_game_assembly(&pe)?;
    let metadata = crate::il2cpp::metadata_path(exe)
        .ok_or_else(|| bad("the game's IL2CPP metadata file wasn't found"))?;
    let meta = std::fs::read(&metadata).map_err(|e| bad(format!("{}: {e}", metadata.display())))?;
    let count = crate::il2cpp::type_count(&meta)?;
    drop(meta);
    let ga = process.module("GameAssembly.dll")?;
    // Unity's get_timeScale: where GameAssembly keeps it once used, else from UnityPlayer.dll's
    // own list of engine functions (games that never change their speed).
    let from_cache =
        time_scale_cache(&pe).and_then(|cache| ptr(process, ga.base + u64::from(cache)));
    let from_player = || -> Option<u64> {
        let up = process.module("UnityPlayer.dll").ok()?;
        let up_bytes = std::fs::read(folder.join("UnityPlayer.dll")).ok()?;
        let up_pe = Pe::parse(&up_bytes)?;
        native_function(&up_pe, "UnityEngine.Time::get_timeScale")
            .map(|rva| up.base + u64::from(rva))
    };
    let time_scale = from_cache.or_else(from_player).and_then(|func| {
        let path = time_scale_path(process, func)?;
        let module = process
            .modules()
            .into_iter()
            .find(|m| path.base >= m.base && path.base < m.end())?;
        Some((module.name, module.base, path))
    });
    drop(bytes);
    let rt = Runtime::new(process, layout, ga.base);
    if rt.loaded(count).is_empty() {
        return Err(bad(
            "the game hasn't loaded its classes yet: start playing, then try again",
        ));
    }
    find_unity(&rt, count, time_scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pe::tests::tiny_pe;
    use std::cell::RefCell;

    #[test]
    fn names_and_words() {
        assert_eq!(norm("<Health>k__BackingField"), "health");
        assert_eq!(norm("m_CurrentHealth"), "currenthealth");
        assert_eq!(norm("_gold"), "gold");
        assert_eq!(
            words("m_LocalPlayerHealth"),
            ["m", "local", "player", "health"]
        );
        assert_eq!(words("AIController"), ["aicontroller"]);
        assert_eq!(words("EnemyAI"), ["enemy", "ai"]);
        assert_eq!(norm_display("<Gold>k__BackingField"), "Gold");
        assert_eq!(norm_display("m_Credits"), "Credits");
    }

    /// The exports of JumpSpace's GameAssembly.dll (Unity 6000.3), as code bytes.
    fn game_assembly() -> Vec<u8> {
        let mut code = vec![0xCCu8; 0x400];
        let mut put = |at: usize, b: &[u8]| code[at..at + b.len()].copy_from_slice(b);
        put(0x000, &[0x48, 0x8B, 0x41, 0x10, 0xC3]); // class_get_name: mov rax,[rcx+10h]
        put(0x010, &[0x48, 0x8B, 0x41, 0x18, 0xC3]); // namespace
        put(0x020, &[0x48, 0x8B, 0x41, 0x58, 0xC3]); // parent
        put(0x030, &[0x48, 0x8B, 0x01, 0xC3]); // field_get_name: mov rax,[rcx]
        put(0x040, &[0x48, 0x8B, 0x41, 0x08, 0xC3]); // field_get_type
        put(0x050, &[0x48, 0x63, 0x41, 0x18, 0xC3]); // field_get_offset: movsxd rax,[rcx+18h]
        put(0x060, &[0x0F, 0xBE, 0x41, 0x0A, 0xC3]); // type_get_type: movsx eax,byte [rcx+0Ah]
        put(0x070, &[0x48, 0x8D, 0x41, 0x20, 0xC3]); // class_get_type: lea rax,[rcx+20h]
        // class_get_fields: movzx ecx,word [rcx+124h]; mov rax,[rdx]; shl rcx,5; add rax,20h;
        // add rcx,[rbx+80h]; ... mov rax,[rbx+80h]; ret
        put(
            0x080,
            &[
                0x0F, 0xB7, 0x89, 0x24, 0x01, 0x00, 0x00, 0x48, 0x8B, 0x02, 0x48, 0xC1, 0xE1, 0x05,
                0x48, 0x83, 0xC0, 0x20, 0x48, 0x8B, 0x83, 0x80, 0x00, 0x00, 0x00, 0xC3,
            ],
        );
        // field_static_get_value: movsxd rbx,[rsi+18h]; add rbx,[rax+0B8h]; ret
        put(
            0x0C0,
            &[
                0x48, 0x63, 0x5E, 0x18, 0x48, 0x03, 0x98, 0xB8, 0x00, 0x00, 0x00, 0xC3,
            ],
        );
        // image_get_class: jmp 0x1100 (the class table lookup)
        put(0x0E0, &[0xE9, 0x1B, 0x00, 0x00, 0x00]);
        // 0x1100: mov rax,[rip+0xF00] (-> 0x2007); lea rsi,[rax+rdi*8]; mov rbx,[rsi]; ret
        put(
            0x100,
            &[
                0x48, 0x8B, 0x05, 0x00, 0x0F, 0x00, 0x00, 0x48, 0x8D, 0x34, 0xF8, 0x48, 0x8B, 0x1E,
                0xC3,
            ],
        );
        tiny_pe(
            &code,
            &[
                ("il2cpp_class_get_name", 0x1000),
                ("il2cpp_class_get_namespace", 0x1010),
                ("il2cpp_class_get_parent", 0x1020),
                ("il2cpp_field_get_name", 0x1030),
                ("il2cpp_field_get_type", 0x1040),
                ("il2cpp_field_get_offset", 0x1050),
                ("il2cpp_type_get_type", 0x1060),
                ("il2cpp_class_get_type", 0x1070),
                ("il2cpp_class_get_fields", 0x1080),
                ("il2cpp_field_static_get_value", 0x10C0),
                ("il2cpp_image_get_class", 0x10E0),
            ],
        )
    }

    pub(crate) fn jumpspace_layout() -> Layout {
        Layout {
            class_name: 0x10,
            class_namespace: 0x18,
            class_parent: 0x58,
            class_fields: 0x80,
            class_field_count: 0x124,
            class_static_fields: 0xB8,
            field_name: 0,
            field_type: 8,
            field_offset: 0x18,
            field_size: 0x20,
            type_kind: 0xA,
            class_table: 0x2007,
        }
    }

    #[test]
    fn reads_the_layout_from_the_exports() {
        let bytes = game_assembly();
        let pe = Pe::parse(&bytes).unwrap();
        assert_eq!(Layout::from_game_assembly(&pe).unwrap(), jumpspace_layout());
        // Something else (no IL2CPP exports): explained.
        let other = tiny_pe(&[0xC3], &[]);
        let err = Layout::from_game_assembly(&Pe::parse(&other).unwrap()).unwrap_err();
        assert!(err.to_string().contains("il2cpp_class_get_name"), "{err}");
    }

    /// A fake game memory: bytes at addresses.
    #[derive(Default)]
    struct Fake {
        bytes: RefCell<HashMap<u64, u8>>,
    }

    impl Fake {
        fn put(&self, at: u64, b: &[u8]) {
            let mut m = self.bytes.borrow_mut();
            for (i, x) in b.iter().enumerate() {
                m.insert(at + i as u64, *x);
            }
        }
        fn p64(&self, at: u64, v: u64) {
            self.put(at, &v.to_le_bytes());
        }
        fn s(&self, at: u64, s: &str) {
            let mut b = s.as_bytes().to_vec();
            b.push(0);
            self.put(at, &b);
        }
    }

    impl Mem for Fake {
        fn read(&self, address: u64, len: usize) -> Option<Vec<u8>> {
            let m = self.bytes.borrow();
            // Strings are read 128 bytes at a time: unknown bytes after a string read as 0.
            let first = m.get(&address)?;
            let mut out = vec![*first];
            for i in 1..len as u64 {
                out.push(*m.get(&(address + i)).unwrap_or(&0));
            }
            Some(out)
        }
        fn heap_regions(&self) -> Vec<(u64, u64)> {
            Vec::new()
        }
    }

    const GA: u64 = 0x7FF0_0000_0000;

    /// A class at `at` with `fields` (name, kind, attrs, offset); strings and lists nearby.
    fn class(
        m: &Fake,
        at: u64,
        ns: &str,
        name: &str,
        parent: u64,
        statics: u64,
        fields: &[(&str, u8, u16, i32)],
    ) {
        m.put(at, &[0u8; 0x140]);
        m.s(at + 0x1000, name);
        m.s(at + 0x1100, ns);
        m.p64(at + 0x10, at + 0x1000);
        m.p64(at + 0x18, at + 0x1100);
        m.p64(at + 0x58, parent);
        m.p64(at + 0xB8, statics);
        m.put(at + 0x124, &(fields.len() as u16).to_le_bytes());
        let list = at + 0x2000;
        m.p64(at + 0x80, list);
        for (k, (fname, kind, attrs, off)) in fields.iter().enumerate() {
            let r = list + k as u64 * 0x20;
            let name_at = at + 0x3000 + k as u64 * 0x40;
            let ty_at = at + 0x5000 + k as u64 * 0x10;
            m.s(name_at, fname);
            m.p64(r, name_at);
            m.p64(r + 8, ty_at);
            m.put(r + 0x18, &off.to_le_bytes());
            m.put(ty_at, &[0u8; 8]);
            m.put(ty_at + 8, &attrs.to_le_bytes());
            m.put(ty_at + 0xA, &[*kind]);
        }
    }

    #[test]
    fn finds_the_players_values_and_not_an_enemys() {
        let m = Fake::default();
        let table = 0x1_0000_0000u64;
        m.p64(GA + 0x2007, table);
        // Classes 0: PlayerManager (static Instance), 1: Player, 2: EnemyManager (static boss),
        // 3: Enemy, 4: Wallet holder GameState (static Current).
        let (pm, player, em, enemy, gs) = (
            0x2_0000_0000u64,
            0x2_0001_0000,
            0x2_0002_0000,
            0x2_0003_0000,
            0x2_0004_0000,
        );
        for (i, c) in [pm, player, em, enemy, gs].iter().enumerate() {
            m.p64(table + i as u64 * 8, *c);
        }
        for i in 5..8u64 {
            m.p64(table + i * 8, 0);
        }
        let (pm_statics, em_statics, gs_statics) = (0x3_0000_0000u64, 0x3_0001_0000, 0x3_0002_0000);
        class(
            &m,
            pm,
            "Game",
            "PlayerManager",
            0,
            pm_statics,
            &[("Instance", T_CLASS, F_STATIC, 0)],
        );
        class(
            &m,
            player,
            "Game",
            "PlayerCharacter",
            0,
            0,
            &[
                ("self", T_CLASS, 0, 0x10),
                ("m_CurrentHealth", T_R4, 0, 0x18),
                ("m_MaxHealth", T_R4, 0, 0x1C),
                ("<Ammo>k__BackingField", T_I4, 0, 0x20),
            ],
        );
        class(
            &m,
            em,
            "Game",
            "EnemyManager",
            0,
            em_statics,
            &[("boss", T_CLASS, F_STATIC, 0)],
        );
        class(
            &m,
            enemy,
            "Game",
            "Enemy",
            0,
            0,
            &[("health", T_R4, 0, 0x10)],
        );
        class(
            &m,
            gs,
            "Game",
            "GameState",
            0,
            gs_statics,
            &[
                ("Current", T_CLASS, F_STATIC, 8),
                ("gold", T_I4, F_STATIC | F_LITERAL, 0),
            ],
        );
        // Objects.
        let (pm_obj, player_obj, boss_obj) = (0x4_0000_0000u64, 0x4_0001_0000, 0x4_0002_0000);
        m.p64(pm_statics, pm_obj);
        m.put(pm_obj, &[0u8; 0x40]);
        m.p64(pm_obj, pm); // a manager object with no instance fields: the static points to it
        // PlayerManager.Instance's class here is PlayerCharacter itself (an object reached directly).
        m.p64(pm_statics, player_obj);
        m.put(player_obj, &[0u8; 0x40]);
        m.p64(player_obj, player);
        m.p64(player_obj + 0x10, player_obj); // points at itself: seen once
        m.put(player_obj + 0x18, &35.5f32.to_le_bytes());
        m.put(player_obj + 0x1C, &100f32.to_le_bytes());
        m.put(player_obj + 0x20, &12i32.to_le_bytes());
        m.p64(em_statics, boss_obj);
        m.put(boss_obj, &[0u8; 0x20]);
        m.p64(boss_obj, enemy);
        m.put(boss_obj + 0x10, &5000f32.to_le_bytes());
        m.put(gs_statics, &[0u8; 0x10]);

        let rt = Runtime::new(&m, jumpspace_layout(), GA);
        assert_eq!(rt.loaded(8).len(), 5);
        let found = find_unity(&rt, 8, None).unwrap();
        let health = found
            .cheats
            .iter()
            .find(|c| c.id == "health")
            .expect("health");
        // table → [0] PlayerManager → static fields → Instance (+0) → +0x18
        assert_eq!(health.offsets, vec![0, 0xB8, 0, 0x18]);
        assert_eq!(health.base, Some(("GameAssembly.dll".into(), 0x2007)));
        assert_eq!(health.control, Control::Keep { at: Some(100.0) });
        assert!(
            health.found_in.contains("PlayerManager.Instance"),
            "{}",
            health.found_in
        );
        let ammo = found.cheats.iter().find(|c| c.id == "ammo").expect("ammo");
        assert_eq!(ammo.ty, ValueType::I32);
        assert_eq!(ammo.offsets, vec![0, 0xB8, 0, 0x20]);
        assert!(
            found.cheats.iter().all(|c| c.now != 5000.0),
            "never the boss"
        );
        assert!(found.not_found.contains(&"Game speed".to_string()));
        assert!(
            found.not_found.contains(&"Money".to_string()),
            "a const isn't money"
        );

        // The paths work against the fake game.
        let p = Path {
            base: GA + 0x2007,
            offsets: health.offsets.clone(),
        };
        assert_eq!(p.resolve(&m), Some(player_obj + 0x18));
    }

    #[test]
    fn follows_get_time_scale_to_the_value() {
        let m = Fake::default();
        let func = 0x7FF1_0000_1000u64;
        let manager_getter = 0x7FF1_0000_2000u64;
        // get_timeScale: sub rsp,28h; call getter; movss xmm0,[rax+0C8h]; add rsp,28h; ret
        let call_rel = (manager_getter - (func + 9)) as i32;
        let mut code = vec![0x48, 0x83, 0xEC, 0x28, 0xE8];
        code.extend_from_slice(&call_rel.to_le_bytes());
        code.extend_from_slice(&[
            0xF3, 0x0F, 0x10, 0x80, 0xC8, 0x00, 0x00, 0x00, 0x48, 0x83, 0xC4, 0x28, 0xC3,
        ]);
        code.resize(160, 0xCC);
        m.put(func, &code);
        // getter: mov rax,[rip+0x1000] (-> manager_getter+7+0x1000); ret
        let mut g = vec![0x48, 0x8B, 0x05, 0x00, 0x10, 0x00, 0x00, 0xC3];
        g.resize(160, 0xCC);
        m.put(manager_getter, &g);
        let global = manager_getter + 7 + 0x1000;
        let manager = 0x5_0000_0000u64;
        m.p64(global, manager);
        m.put(manager + 0xC8, &1f32.to_le_bytes());
        let p = time_scale_path(&m, func).expect("found");
        assert_eq!(
            p,
            Path {
                base: global,
                offsets: vec![0xC8]
            }
        );
        assert_eq!(p.resolve(&m), Some(manager + 0xC8));
        let rt = Runtime::new(&m, jumpspace_layout(), GA);
        let found = find_unity(
            &rt,
            0,
            Some(("UnityPlayer.dll".into(), 0x7FF1_0000_0000, p)),
        )
        .unwrap();
        let speed = &found.cheats[0];
        assert_eq!(speed.id, "game_speed");
        assert_eq!(
            speed.base,
            Some(("UnityPlayer.dll".into(), global - 0x7FF1_0000_0000))
        );
        assert_eq!(speed.now, 1.0);
    }

    #[test]
    fn finds_a_native_function_by_name() {
        // Code at 0x1000 and 0x1008; in the data section (RVA 0x2000+): two function pointers,
        // an empty entry, two name pointers, then the names.
        let code = [0xC3u8; 0x10];
        let mut bytes = tiny_pe(&code, &[]);
        let data = bytes.len() - 0x1000 + 0x900; // file offset of RVA 0x2900
        let va = |rva: u64| (0x1_8000_0000u64 + rva).to_le_bytes();
        let names = [
            b"UnityEngine.Time::get_deltaTime ".as_slice(),
            b"UnityEngine.Time::get_timeScale ",
        ];
        let mut at = data + 0x40;
        let mut name_rvas = vec![];
        for n in names {
            bytes[at..at + n.len()].copy_from_slice(n);
            name_rvas.push(0x2900 + (at - data) as u64);
            at += n.len();
        }
        bytes[data..data + 8].copy_from_slice(&va(0x1000));
        bytes[data + 8..data + 16].copy_from_slice(&va(0x1008));
        bytes[data + 16..data + 24].copy_from_slice(&[0; 8]);
        bytes[data + 24..data + 32].copy_from_slice(&va(name_rvas[0]));
        bytes[data + 32..data + 40].copy_from_slice(&va(name_rvas[1]));
        let pe = Pe::parse(&bytes).unwrap();
        assert_eq!(
            native_function(&pe, "UnityEngine.Time::get_timeScale"),
            Some(0x1008)
        );
        assert_eq!(
            native_function(&pe, "UnityEngine.Time::get_deltaTime"),
            Some(0x1000)
        );
        assert_eq!(native_function(&pe, "UnityEngine.Time::nope"), None);
    }

    #[test]
    fn finds_the_time_scale_cache() {
        // 0x1000: lea rcx,[rip+X] (the name); call ...; test rax,rax; mov [rip+Y],rax
        let mut code = vec![
            0x48, 0x8D, 0x0D, 0, 0, 0, 0, 0xE8, 0, 0, 0, 0, 0x48, 0x85, 0xC0, 0x48, 0x89, 0x05, 0,
            0, 0, 0, 0xC3,
        ];
        // The name goes into the export section's spare room at 0x2800.
        let name_rva = 0x2800u32;
        let rel = name_rva as i32 - (0x1000 + 7);
        code[3..7].copy_from_slice(&rel.to_le_bytes());
        let cache_rva = 0x2900u32;
        let rel2 = cache_rva as i32 - (0x1000 + 22);
        code[18..22].copy_from_slice(&rel2.to_le_bytes());
        let mut bytes = tiny_pe(&code, &[]);
        let name_off = bytes.len() - 0x1000 + 0x800;
        let s = b"UnityEngine.Time::get_timeScale()\0";
        bytes[name_off..name_off + s.len()].copy_from_slice(s);
        let pe = Pe::parse(&bytes).unwrap();
        assert_eq!(time_scale_cache(&pe), Some(cache_rva));
    }
}
