//! Reads an Unreal Engine game's names, objects and properties from its
//! memory (UE 4.23 and newer, including UE5). Read-only: nothing in the game
//! is changed.
//!
//! Unreal keeps every name in a pool (`FNamePool`) and every object in an
//! array (`FUObjectArray`); classes describe their properties (`FProperty`)
//! with names, types and offsets. Their exact layout differs between engine
//! versions and builds, so instead of hard-coding it this reader finds and
//! checks each piece against things every Unreal game has:
//! - the name pool starts with `None`, then `ByteProperty`;
//! - the first object is the package `/Script/CoreUObject`;
//! - the engine struct `Vector` has the properties `X`, `Y`, `Z` at offsets
//!   0, n and 2n (n = 4 in UE4, 8 in UE5);
//! - the class `Actor` derives from `Object`.
//!
//! With that, it can list a game's classes and properties ("PlayerCharacter
//! .Health: FloatProperty at +0x2A8") and find live objects of a class, so
//! values can be read by name.

use std::collections::HashMap;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::session::AttachedProcess;

/// Objects listed at most.
const MAX_OBJECTS: usize = 4_000_000;

fn bad(why: impl Into<String>) -> Error {
    Error::Debugger(format!("Unreal reader: {}", why.into()))
}

/// What the reader needs from a process (a trait so tests can fake it).
pub trait Mem {
    fn read(&self, address: u64, len: usize) -> Option<Vec<u8>>;
    fn u16(&self, a: u64) -> Option<u16> {
        self.read(a, 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&self, a: u64) -> Option<u32> {
        self.read(a, 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i32(&self, a: u64) -> Option<i32> {
        self.u32(a).map(|v| v as i32)
    }
    /// Writable private memory (base, size), where the engine allocates
    /// objects. Empty if the reader can't search memory.
    fn heap_regions(&self) -> Vec<(u64, u64)> {
        Vec::new()
    }
    fn ptr(&self, a: u64) -> Option<u64> {
        self.read(a, 8)
            .map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
            .filter(|p| (0x10000..0x7FFF_FFFF_FFFF).contains(p))
    }
}

impl Mem for AttachedProcess {
    fn read(&self, address: u64, len: usize) -> Option<Vec<u8>> {
        self.read_bytes(address, len).ok()
    }
    fn heap_regions(&self) -> Vec<(u64, u64)> {
        self.regions()
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.readable && r.writable && !r.image && r.size <= 4 << 30)
            .map(|r| (r.base, r.size))
            .collect()
    }
}

/// Reads an object header (0x28 bytes) from a buffer.
fn parse_head(address: u64, h: &[u8]) -> ObjHead {
    let q = |o: usize| u64::from_le_bytes(h[o..o + 8].try_into().unwrap_or([0; 8]));
    let d = |o: usize| u32::from_le_bytes(h[o..o + 4].try_into().unwrap_or([0; 4]));
    ObjHead {
        address,
        class: q(0x10),
        name: d(0x18),
        number: d(0x1C),
        outer: q(0x20),
    }
}

/// Searches writable memory for objects whose class pointer (at +0x10)
/// satisfies `want`. This is a plain memory scan, like the value scanner's.
fn scan_objects<M: Mem>(mem: &M, want: impl Fn(u64, u64) -> bool, limit: usize) -> Vec<ObjHead> {
    let mut out = Vec::new();
    for (base, size) in mem.heap_regions() {
        let mut at = base;
        while at < base + size && out.len() < limit {
            let len = (base + size - at).min(16 << 20) as usize;
            if let Some(buf) = mem.read(at, len) {
                for i in (0..len.saturating_sub(0x28)).step_by(8) {
                    let class =
                        u64::from_le_bytes(buf[i + 0x10..i + 0x18].try_into().unwrap_or([0; 8]));
                    let address = at + i as u64;
                    if class != 0 && want(address, class) {
                        out.push(parse_head(address, &buf[i..i + 0x28]));
                        if out.len() >= limit {
                            break;
                        }
                    }
                }
            }
            at += len as u64;
        }
    }
    out
}

/// Name -> index for `wanted`, walking name block 0 entry by entry.
fn name_indices<M: Mem>(mem: &M, block0: u64, wanted: &[&str]) -> HashMap<String, u32> {
    let mut out = HashMap::new();
    let Some(buf) = mem
        .read(block0, 1 << 20)
        .or_else(|| mem.read(block0, 1 << 16))
    else {
        return out;
    };
    let mut off = 0usize;
    while off + 2 < buf.len() && out.len() < wanted.len() {
        let h = u16::from_le_bytes([buf[off], buf[off + 1]]);
        let len = usize::from(h >> 6);
        if len == 0 {
            break;
        }
        let bytes = if h & 1 != 0 { len * 2 } else { len };
        if let Some(name) = buf
            .get(off + 2..off + 2 + bytes)
            .and_then(|c| decode_entry(h, c))
            && wanted.contains(&name.as_str())
        {
            out.insert(name, (off / 2) as u32);
        }
        off += 2 + bytes;
        off += off % 2;
    }
    out
}

/// A name pool entry: 2-byte header (wide flag in bit 0, length in the top
/// 10 bits) then the characters.
pub(crate) fn decode_entry(header: u16, chars: &[u8]) -> Option<String> {
    let wide = header & 1 != 0;
    let len = usize::from(header >> 6);
    if wide {
        let units: Vec<u16> = chars
            .get(..len * 2)?
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        Some(String::from_utf16_lossy(&units))
    } else {
        Some(String::from_utf8_lossy(chars.get(..len)?).into_owned())
    }
}

/// Where `None` + `ByteProperty` (the start of block 0) are in `hay`.
pub(crate) fn find_block0(hay: &[u8]) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = hay[from..].windows(4).position(|w| w == b"None") {
        let at = from + i;
        let ok = at >= 2
            && hay.get(at + 6..at + 18) == Some(b"ByteProperty".as_slice())
            && u16::from_le_bytes([hay[at - 2], hay[at - 1]]) >> 6 == 4
            && u16::from_le_bytes([hay[at + 4], hay[at + 5]]) >> 6 == 12;
        if ok {
            return Some(at - 2);
        }
        from = at + 4;
    }
    None
}

/// Field offsets, found by checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub item_size: u64,
    pub super_struct: u64,
    pub child_properties: u64,
    pub field_next: u64,
    pub field_name: u64,
    pub property_element_size: u64,
    pub property_offset: u64,
}

#[derive(Debug, Clone, Copy)]
struct ObjHead {
    address: u64,
    class: u64,
    name: u32,
    number: u32,
    outer: u64,
}

/// A property of a class or struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Property {
    pub name: String,
    /// `FloatProperty`, `IntProperty`, `ObjectProperty`, …
    pub ty: String,
    pub offset: u64,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassView {
    /// `/Script/Game.PlayerCharacter`.
    pub path: String,
    pub name: String,
    /// Declared in the game rather than the engine.
    pub game: bool,
    pub properties: Vec<Property>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub address: u64,
    pub name: String,
    pub class: String,
    /// Numeric properties: (name, type, address, value now).
    pub values: Vec<(String, String, u64, String)>,
}

/// A connected reader.
pub struct Reader<M: Mem> {
    mem: M,
    blocks: u64,
    objects: u64,
    count: usize,
    layout: Layout,
    heads: Vec<ObjHead>,
    names: std::cell::RefCell<HashMap<u32, String>>,
    by_address: HashMap<u64, usize>,
    class_class: u64,
    struct_classes: Vec<u64>,
    /// No object array was found; objects are found by searching memory.
    scan_mode: bool,
    /// The exe's address range. Every real object starts with a pointer
    /// into it (its virtual function table).
    image: (u64, u64),
}

impl<M: Mem> std::fmt::Debug for Reader<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reader")
            .field("objects", &self.count)
            .field("layout", &self.layout)
            .finish()
    }
}

fn name_at<M: Mem>(mem: &M, blocks: u64, index: u32) -> Option<String> {
    let block = mem.ptr(blocks + u64::from(index >> 16) * 8)?;
    let entry = block + u64::from(index & 0xFFFF) * 2;
    let header = mem.u16(entry)?;
    let len = usize::from(header >> 6);
    if len == 0 || len > 1024 {
        return None;
    }
    let bytes = mem.read(entry + 2, if header & 1 != 0 { len * 2 } else { len })?;
    decode_entry(header, &bytes)
}

impl<T: Mem> Mem for std::sync::Arc<T> {
    fn read(&self, address: u64, len: usize) -> Option<Vec<u8>> {
        (**self).read(address, len)
    }
    fn heap_regions(&self) -> Vec<(u64, u64)> {
        (**self).heap_regions()
    }
}

impl Reader<std::sync::Arc<AttachedProcess>> {
    /// Finds the name pool and object array of the attached game.
    pub fn connect(process: std::sync::Arc<AttachedProcess>) -> Result<Self> {
        let main = process.main_module()?;
        // 1. Block 0 of the name pool, somewhere in writable private memory.
        let mut block0 = None;
        'regions: for r in process.regions()? {
            if !r.readable || r.image || !r.writable || r.size > 512 << 20 {
                continue;
            }
            let mut at = r.base;
            while at < r.base + r.size {
                let len = (r.base + r.size - at).min(4 << 20) as usize;
                if let Ok(buf) = process.read_bytes(at, len)
                    && let Some(i) = find_block0(&buf)
                {
                    block0 = Some(at + i as u64);
                    break 'regions;
                }
                at += len as u64 - 32;
                if len < 64 {
                    break;
                }
            }
        }
        let block0 = block0.ok_or_else(|| {
            bad("no Unreal name pool found (the game may use UE 4.22 or older, or not be an Unreal game)")
        })?;
        let image = process.module_image(&main)?;
        Self::from_parts(process, &image, block0)
    }
}

impl<M: Mem> Reader<M> {
    /// Finds the rest from the exe image and name block 0.
    pub(crate) fn from_parts(mem: M, image: &[(u64, Vec<u8>)], block0: u64) -> Result<Self> {
        // 2. The pool's block array: a pointer to block 0 inside the exe.
        let target = block0.to_le_bytes();
        let mut blocks = None;
        for (start, bytes) in image {
            for i in (0..bytes.len().saturating_sub(8)).step_by(8) {
                if bytes[i..i + 8] == target {
                    let at = start + i as u64;
                    if name_at(&mem, at, 0).as_deref() == Some("None") {
                        blocks = Some(at);
                        break;
                    }
                }
            }
            if blocks.is_some() {
                break;
            }
        }
        let blocks = blocks.ok_or_else(|| bad("found names but not where the game keeps them"))?;
        let image_range = (
            image.iter().map(|c| c.0).min().unwrap_or(0),
            image
                .iter()
                .map(|c| c.0 + c.1.len() as u64)
                .max()
                .unwrap_or(0),
        );

        // 3. The object array: Objects**, PreAllocated*, Max, Num, MaxChunks, NumChunks.
        let mut found = None;
        'scan: for (start, bytes) in image {
            for i in (0..bytes.len().saturating_sub(32)).step_by(4) {
                let w = |o: usize| {
                    u32::from_le_bytes(bytes[i + o..i + o + 4].try_into().unwrap_or([0; 4]))
                };
                let (max, num, max_chunks, num_chunks) = (w(16), w(20), w(24), w(28));
                if !(1000..=MAX_OBJECTS as u32).contains(&num)
                    || max < num
                    || num_chunks != num.div_ceil(65536)
                    || max_chunks < num_chunks
                    || u64::from(max_chunks) * 65536 < u64::from(max)
                {
                    continue;
                }
                let objects = u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap_or([0; 8]));
                let Some(chunk0) = mem.ptr(objects) else {
                    continue;
                };
                for item_size in [24u64, 16, 32] {
                    let (Some(o0), Some(o1)) = (mem.ptr(chunk0), mem.ptr(chunk0 + item_size))
                    else {
                        continue;
                    };
                    let first = mem.u32(o0 + 0x18).and_then(|n| name_at(&mem, blocks, n));
                    if first.as_deref() == Some("/Script/CoreUObject")
                        && mem.u32(o1 + 0xC) == Some(1)
                    {
                        found = Some((start + i as u64, objects, num as usize, item_size));
                        break 'scan;
                    }
                }
            }
        }
        let Some((_, objects, count, item_size)) = found else {
            // Some builds don't keep the object array where it can be read
            // directly; find classes by searching memory instead.
            return Self::from_scan(mem, blocks, block0, image_range);
        };

        // 4. Read every object's header.
        let mut heads = Vec::with_capacity(count);
        let mut chunk = 0u64;
        for i in 0..count {
            if i % 65536 == 0 {
                chunk = mem.ptr(objects + (i as u64 / 65536) * 8).unwrap_or(0);
            }
            if chunk == 0 {
                continue;
            }
            let Some(obj) = mem.ptr(chunk + (i as u64 % 65536) * item_size) else {
                continue;
            };
            let Some(h) = mem.read(obj, 0x28) else {
                continue;
            };
            let q = |o: usize| u64::from_le_bytes(h[o..o + 8].try_into().unwrap_or([0; 8]));
            let d = |o: usize| u32::from_le_bytes(h[o..o + 4].try_into().unwrap_or([0; 4]));
            heads.push(ObjHead {
                address: obj,
                class: q(0x10),
                name: d(0x18),
                number: d(0x1C),
                outer: q(0x20),
            });
        }
        let by_address: HashMap<u64, usize> = heads
            .iter()
            .enumerate()
            .map(|(i, h)| (h.address, i))
            .collect();
        let mut r = Self {
            mem,
            blocks,
            objects,
            count,
            layout: Layout {
                item_size,
                super_struct: 0,
                child_properties: 0,
                field_next: 0,
                field_name: 0,
                property_element_size: 0,
                property_offset: 0,
            },
            heads,
            names: Default::default(),
            by_address,
            class_class: 0,
            struct_classes: Vec::new(),
            scan_mode: false,
            image: image_range,
        };
        r.calibrate()?;
        Ok(r)
    }

    /// Finds classes and structs by searching memory (slower, about a minute
    /// per search on a big game).
    fn from_scan(mem: M, blocks: u64, block0: u64, image: (u64, u64)) -> Result<Self> {
        let idx = name_indices(
            &mem,
            block0,
            &[
                "Class",
                "ScriptStruct",
                "BlueprintGeneratedClass",
                "WidgetBlueprintGeneratedClass",
                "AnimBlueprintGeneratedClass",
            ],
        );
        let class_name = *idx.get("Class").ok_or_else(|| bad("no Class name"))?;
        // 1. The class of classes: it is its own class.
        let class_obj = scan_objects(
            &mem,
            |address, class| {
                class == address
                    && mem.u32(address + 0x18) == Some(class_name)
                    && mem.u32(address + 0x1C) == Some(0)
            },
            1,
        )
        .first()
        .map(|h| h.address)
        .ok_or_else(|| bad("couldn't find the game's classes in memory"))?;
        // 2. Every native class.
        let mut heads = scan_objects(&mem, |_, class| class == class_obj, MAX_OBJECTS);
        // 3. Structs and blueprint classes, whose classes are among those.
        let kinds: Vec<u64> = heads
            .iter()
            .filter(|h| {
                [
                    "ScriptStruct",
                    "BlueprintGeneratedClass",
                    "WidgetBlueprintGeneratedClass",
                    "AnimBlueprintGeneratedClass",
                ]
                .iter()
                .any(|k| idx.get(*k) == Some(&h.name))
            })
            .map(|h| h.address)
            .collect();
        heads.extend(scan_objects(
            &mem,
            |_, class| kinds.contains(&class),
            MAX_OBJECTS,
        ));
        heads.sort_by_key(|h| h.address);
        heads.dedup_by_key(|h| h.address);
        let by_address: HashMap<u64, usize> = heads
            .iter()
            .enumerate()
            .map(|(i, h)| (h.address, i))
            .collect();
        let count = heads.len();
        let mut r = Self {
            mem,
            blocks,
            objects: 0,
            count,
            layout: Layout {
                item_size: 0,
                super_struct: 0,
                child_properties: 0,
                field_next: 0,
                field_name: 0,
                property_element_size: 0,
                property_offset: 0,
            },
            heads,
            names: Default::default(),
            by_address,
            class_class: 0,
            struct_classes: Vec::new(),
            scan_mode: true,
            image,
        };
        r.calibrate()?;
        r.struct_classes.extend(kinds);
        r.struct_classes.sort_unstable();
        r.struct_classes.dedup();
        Ok(r)
    }

    /// Whether `address` looks like a live object: function table in the
    /// exe, a name that decodes, and an outer object.
    fn is_object_at(&self, address: u64) -> bool {
        let (lo, hi) = self.image;
        let in_image = |p: Option<u64>| p.is_some_and(|p| p >= lo && p < hi);
        in_image(self.mem.ptr(address))
            && self
                .mem
                .u32(address + 0x18)
                .and_then(|n| name_at(&self.mem, self.blocks, n))
                .is_some_and(|n| !n.is_empty())
            && self.mem.ptr(address + 0x20).is_some()
    }

    /// True when objects are found by searching memory.
    pub fn scan_mode(&self) -> bool {
        self.scan_mode
    }

    fn name(&self, index: u32) -> String {
        if let Some(n) = self.names.borrow().get(&index) {
            return n.clone();
        }
        let n = name_at(&self.mem, self.blocks, index).unwrap_or_default();
        self.names.borrow_mut().insert(index, n.clone());
        n
    }

    fn obj_name(&self, h: &ObjHead) -> String {
        let base = self.name(h.name);
        if h.number > 0 {
            format!("{base}_{}", h.number - 1)
        } else {
            base
        }
    }

    fn head(&self, address: u64) -> Option<ObjHead> {
        if let Some(&i) = self.by_address.get(&address) {
            return Some(self.heads[i]);
        }
        // Packages and other objects outside the list: read them directly.
        let h = self.mem.read(address, 0x28)?;
        Some(parse_head(address, &h))
    }

    /// `/Script/Engine.Actor`.
    fn path(&self, h: &ObjHead) -> String {
        let mut parts = vec![self.obj_name(h)];
        let mut outer = h.outer;
        for _ in 0..8 {
            let Some(o) = self.head(outer) else { break };
            parts.push(self.obj_name(&o));
            outer = o.outer;
        }
        parts.reverse();
        parts.join(".")
    }

    fn find(&self, name: &str, package: &str) -> Option<&ObjHead> {
        self.heads.iter().find(|h| {
            self.name(h.name) == name
                && h.number == 0
                && self
                    .head(h.outer)
                    .is_some_and(|o| self.name(o.name) == package)
        })
    }

    fn field_name(&self, field: u64) -> Option<String> {
        self.mem
            .u32(field + self.layout.field_name)
            .map(|n| self.name(n))
    }

    fn calibrate(&mut self) -> Result<()> {
        // The class of classes: an object named Class whose class is itself.
        self.class_class = self
            .heads
            .iter()
            .find(|h| h.class == h.address && self.name(h.name) == "Class")
            .map(|h| h.address)
            .ok_or_else(|| bad("no Class object"))?;
        let vector = *self
            .find("Vector", "/Script/CoreUObject")
            .ok_or_else(|| bad("no Vector struct"))?;
        self.struct_classes = [self.class_class, vector.class]
            .into_iter()
            .chain(
                self.heads
                    .iter()
                    .filter(|h| {
                        h.class == self.class_class
                            && self.name(h.name) == "BlueprintGeneratedClass"
                    })
                    .map(|h| h.address),
            )
            .collect();

        // Properties of Vector: X, Y, Z through ChildProperties and Next.
        let mut found = None;
        'co: for co in (0x28u64..0xA0).step_by(8) {
            let Some(first) = self.mem.ptr(vector.address + co) else {
                continue;
            };
            for (next, name) in [
                (0x20u64, 0x28u64),
                (0x18, 0x20),
                (0x18, 0x28),
                (0x20, 0x30),
                (0x28, 0x30),
            ] {
                let n = |f: u64| self.mem.u32(f + name).map(|i| self.name(i));
                let second = self.mem.ptr(first + next);
                let third = second.and_then(|s| self.mem.ptr(s + next));
                if n(first).as_deref() == Some("X")
                    && second.and_then(n).as_deref() == Some("Y")
                    && third.and_then(n).as_deref() == Some("Z")
                {
                    found = Some((
                        co,
                        next,
                        name,
                        [first, second.unwrap_or(0), third.unwrap_or(0)],
                    ));
                    break 'co;
                }
            }
        }
        let (co, next, name, xyz) =
            found.ok_or_else(|| bad("couldn't find the properties of Vector"))?;
        self.layout.child_properties = co;
        self.layout.field_next = next;
        self.layout.field_name = name;

        // Element size and offset: X, Y, Z at 0, n, 2n with size n.
        let mut sized = None;
        'n: for n in [4i32, 8] {
            for eo in (name + 8..name + 0x60).step_by(4) {
                let offs: Vec<_> = xyz.iter().map(|&f| self.mem.i32(f + eo)).collect();
                if offs != [Some(0), Some(n), Some(2 * n)] {
                    continue;
                }
                for es in (name + 8..name + 0x60).step_by(4) {
                    if es != eo && xyz.iter().all(|&f| self.mem.i32(f + es) == Some(n)) {
                        sized = Some((es, eo));
                        break 'n;
                    }
                }
            }
        }
        let (es, eo) = sized.ok_or_else(|| bad("couldn't find property offsets"))?;
        self.layout.property_element_size = es;
        self.layout.property_offset = eo;

        // SuperStruct: Actor's points to Object.
        let actor = *self
            .find("Actor", "/Script/Engine")
            .ok_or_else(|| bad("no Actor class"))?;
        let object = self
            .find("Object", "/Script/CoreUObject")
            .map(|h| h.address)
            .ok_or_else(|| bad("no Object class"))?;
        self.layout.super_struct = (0x28u64..0x60)
            .step_by(8)
            .find(|&so| self.mem.ptr(actor.address + so) == Some(object))
            .ok_or_else(|| bad("couldn't find the parent class link"))?;
        Ok(())
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    pub fn object_count(&self) -> usize {
        self.count
    }

    fn type_name(&self, field: u64) -> String {
        // FField::ClassPrivate is an FFieldClass whose first member is its FName.
        self.mem
            .ptr(field + 0x8)
            .and_then(|c| self.mem.u32(c))
            .map(|n| self.name(n))
            .unwrap_or_else(|| "?".into())
    }

    /// The properties a struct or class declares itself (not inherited).
    fn own_properties(&self, strukt: u64) -> Vec<Property> {
        let mut out = Vec::new();
        let mut field = self.mem.ptr(strukt + self.layout.child_properties);
        let mut guard = 0;
        while let Some(f) = field {
            guard += 1;
            if guard > 4096 {
                break;
            }
            let name = self.field_name(f).unwrap_or_default();
            out.push(Property {
                name,
                ty: self.type_name(f),
                offset: self
                    .mem
                    .i32(f + self.layout.property_offset)
                    .map_or(0, |v| v.max(0) as u64),
                size: self
                    .mem
                    .i32(f + self.layout.property_element_size)
                    .map_or(0, |v| v.max(0) as u64),
            });
            field = self.mem.ptr(f + self.layout.field_next);
        }
        out
    }

    /// All properties including inherited ones, by offset.
    fn all_properties(&self, strukt: u64) -> Vec<Property> {
        let mut out = Vec::new();
        let mut s = Some(strukt);
        for _ in 0..32 {
            let Some(cur) = s else { break };
            out.extend(self.own_properties(cur));
            s = self.mem.ptr(cur + self.layout.super_struct);
        }
        out.sort_by_key(|p| p.offset);
        out
    }

    fn is_engine(path: &str) -> bool {
        [
            "/Script/CoreUObject",
            "/Script/Engine",
            "/Script/UMG",
            "/Script/Slate",
            "/Script/Niagara",
        ]
        .iter()
        .any(|p| path.starts_with(p))
            || path.starts_with("/Script/") && path.contains("Engine")
    }

    /// Classes and structs whose name, or one of whose own properties,
    /// contains `query`; game classes first.
    pub fn search(&self, query: &str, limit: usize) -> Vec<ClassView> {
        let q = query.to_lowercase();
        let mut out = Vec::new();
        for h in &self.heads {
            if !self.struct_classes.contains(&h.class) {
                continue;
            }
            let name = self.obj_name(h);
            if name.starts_with("Default__")
                || name.starts_with("SKEL_")
                || name.starts_with("REINST_")
            {
                continue;
            }
            let props = self.own_properties(h.address);
            let name_hit = name.to_lowercase().contains(&q);
            let matching: Vec<Property> = props
                .iter()
                .filter(|p| name_hit || p.name.to_lowercase().contains(&q))
                .cloned()
                .collect();
            if matching.is_empty() && !name_hit {
                continue;
            }
            let path = self.path(h);
            out.push(ClassView {
                game: !Self::is_engine(&path),
                path,
                name,
                properties: matching,
            });
        }
        out.sort_by(|a, b| b.game.cmp(&a.game).then(a.path.cmp(&b.path)));
        out.truncate(limit);
        out
    }

    fn class_named(&self, name: &str) -> Option<&ObjHead> {
        self.heads
            .iter()
            .filter(|h| self.struct_classes.contains(&h.class))
            .find(|h| self.obj_name(h) == name)
    }

    fn derives_from(&self, mut class: u64, base: u64) -> bool {
        for _ in 0..32 {
            if class == base {
                return true;
            }
            match self.mem.ptr(class + self.layout.super_struct) {
                Some(s) => class = s,
                None => return false,
            }
        }
        false
    }

    /// Live objects of a class (and its subclasses) with their numeric
    /// property values and addresses.
    /// `only`: keep properties whose name contains this (case-insensitive).
    pub fn instances(
        &self,
        class_name: &str,
        only: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Instance>> {
        let only = only.map(str::to_lowercase);
        let class = *self
            .class_named(class_name)
            .ok_or_else(|| bad(format!("no class named {class_name}")))?;
        let props: Vec<Property> = self
            .all_properties(class.address)
            .into_iter()
            .filter(|p| {
                matches!(
                    p.ty.as_str(),
                    "FloatProperty"
                        | "DoubleProperty"
                        | "IntProperty"
                        | "Int64Property"
                        | "ByteProperty"
                        | "UInt32Property"
                        | "Int16Property"
                        | "BoolProperty"
                ) && only
                    .as_ref()
                    .is_none_or(|q| p.name.to_lowercase().contains(q))
            })
            .collect();
        let mut out = Vec::new();
        let candidates = self.live_heads(class.address, limit * 4);
        for h in &candidates {
            if out.len() >= limit {
                break;
            }
            let name = self.obj_name(h);
            let values = props
                .iter()
                .filter_map(|p| {
                    let at = h.address + p.offset;
                    let v = match p.ty.as_str() {
                        "FloatProperty" => self.mem.read(at, 4).and_then(|b| {
                            Some(f32::from_le_bytes(b.try_into().ok()?).to_string())
                        })?,
                        "DoubleProperty" => self.mem.read(at, 8).and_then(|b| {
                            Some(f64::from_le_bytes(b.try_into().ok()?).to_string())
                        })?,
                        "IntProperty" => self.mem.i32(at)?.to_string(),
                        "UInt32Property" => self.mem.u32(at)?.to_string(),
                        "Int64Property" => self.mem.read(at, 8).and_then(|b| {
                            Some(i64::from_le_bytes(b.try_into().ok()?).to_string())
                        })?,
                        "Int16Property" => (self.mem.u16(at)? as i16).to_string(),
                        "ByteProperty" | "BoolProperty" => self.mem.read(at, 1)?[0].to_string(),
                        _ => return None,
                    };
                    Some((p.name.clone(), p.ty.clone(), at, v))
                })
                .collect();
            out.push(Instance {
                address: h.address,
                name,
                class: self.obj_name(&self.head(h.class).unwrap_or(*h)),
                values,
            });
        }
        Ok(out)
    }

    pub fn objects_address(&self) -> u64 {
        self.objects
    }

    /// Live objects (not class defaults) whose class is `class` or derives from it.
    fn live_heads(&self, class: u64, limit: usize) -> Vec<ObjHead> {
        let found;
        let candidates: &[ObjHead] = if self.scan_mode {
            let targets: Vec<u64> = self
                .heads
                .iter()
                .filter(|h| {
                    self.struct_classes.contains(&h.class) && self.derives_from(h.address, class)
                })
                .map(|h| h.address)
                .collect();
            // A class pointer alone matches stale memory and copies too, so
            // also require what every live object has: a function table in
            // the exe, a readable name and an outer object.
            found = scan_objects(
                &self.mem,
                |address, c| targets.contains(&c) && self.is_object_at(address),
                limit,
            );
            &found
        } else {
            &self.heads
        };
        candidates
            .iter()
            .filter(|h| {
                self.derives_from(h.class, class) && !self.obj_name(h).starts_with("Default__")
            })
            .take(limit)
            .copied()
            .collect()
    }
}

/// What instant engine cheats need (v1.26.0): the reader answers by name.
impl<M: Mem> crate::engine_cheats::Graph for Reader<M> {
    fn ptr(&self, address: u64) -> Option<u64> {
        self.mem.ptr(address)
    }
    fn read(&self, address: u64, len: usize) -> Option<Vec<u8>> {
        self.mem.read(address, len)
    }
    fn properties_of(&self, object: u64) -> Vec<Property> {
        self.mem
            .ptr(object + 0x10)
            .filter(|c| {
                self.struct_classes
                    .contains(&self.head(*c).map_or(0, |h| h.class))
            })
            .map(|c| self.all_properties(c))
            .unwrap_or_default()
    }
    fn class_name_of(&self, object: u64) -> String {
        self.mem
            .ptr(object + 0x10)
            .and_then(|c| self.head(c))
            .map(|h| self.obj_name(&h))
            .unwrap_or_else(|| "?".into())
    }
    fn is_a(&self, object: u64, class: &str) -> bool {
        let (Some(base), Some(c)) = (self.class_named(class), self.mem.ptr(object + 0x10)) else {
            return false;
        };
        self.derives_from(c, base.address)
    }
    fn outer_of(&self, object: u64) -> Option<u64> {
        self.mem.ptr(object + 0x20)
    }
    fn find_live(&self, class: &str) -> Option<u64> {
        let class = self.class_named(class)?.address;
        self.live_heads(class, 1).first().map(|h| h.address)
    }
    fn global_pointing_to(&self, target: u64) -> Option<u64> {
        let (lo, hi) = self.image;
        let want = target.to_le_bytes();
        let mut at = lo & !7;
        while at < hi {
            let len = (hi - at).min(1 << 20) as usize;
            if let Some(buf) = self.mem.read(at, len)
                && let Some(i) = (0..len.saturating_sub(7))
                    .step_by(8)
                    .find(|&i| buf[i..i + 8] == want)
            {
                return Some(at + i as u64);
            }
            at += len as u64;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(s: &str) -> Vec<u8> {
        let header = (s.len() as u16) << 6;
        [&header.to_le_bytes()[..], s.as_bytes()].concat()
    }

    #[test]
    fn decodes_name_entries() {
        assert_eq!(decode_entry(4 << 6, b"None").as_deref(), Some("None"));
        let wide: Vec<u8> = "Ab".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(decode_entry((2 << 6) | 1, &wide).as_deref(), Some("Ab"));
        assert_eq!(decode_entry(9 << 6, b"short"), None);
    }

    #[test]
    fn finds_the_first_name_block() {
        let mut hay = vec![0u8; 37];
        let at = hay.len();
        hay.extend(entry("None"));
        hay.extend(entry("ByteProperty"));
        hay.extend(entry("IntProperty"));
        assert_eq!(find_block0(&hay), Some(at));
        assert_eq!(find_block0(b"..None..ByteProperty"), None);
    }
}

/// Manual check against a running single-player Unreal game on this PC (never
/// in CI). Attaches through the anti-cheat guard and only reads memory:
/// `JB_UNREAL_PID=<pid> cargo test -p jb-core unreal_probe -- --ignored --nocapture`
/// (`JB_UNREAL_CHEATS=1` also lists the instant engine cheats it would make).
#[cfg(test)]
mod probe {
    #[test]
    #[ignore = "needs a running Unreal game"]
    fn unreal_probe() {
        let Some(pid) = std::env::var("JB_UNREAL_PID")
            .ok()
            .and_then(|p| p.parse().ok())
        else {
            return;
        };
        let p = crate::AttachedProcess::attach(pid).expect("attach (guard)");
        let started = std::time::Instant::now();
        let r = super::Reader::connect(std::sync::Arc::new(p)).expect("connect");
        println!(
            "objects {} in {:?}, layout {:?}",
            r.object_count(),
            started.elapsed(),
            r.layout()
        );
        // Instant engine cheats (v1.26.0): what would go in the pack.
        if std::env::var("JB_UNREAL_CHEATS").is_ok() {
            match crate::engine_cheats::find_unreal(&r) {
                Ok(found) => {
                    println!("engine global at {:#X}", found.global);
                    for c in &found.cheats {
                        println!(
                            "  {} ({:?}) = {} from {}: {:?} {:?}",
                            c.name, c.ty, c.now, c.found_in, c.offsets, c.control
                        );
                    }
                    println!("  not found: {:?}", found.not_found);
                }
                Err(e) => println!("instant cheats: {e}"),
            }
        }
        let q = std::env::var("JB_UNREAL_QUERY").unwrap_or_else(|_| "Health".into());
        for c in r.search(&q, 15) {
            let props: Vec<String> = c
                .properties
                .iter()
                .take(6)
                .map(|p| format!("{}: {} +{:#X}", p.name, p.ty, p.offset))
                .collect();
            println!(
                "{}{} | {}",
                if c.game { "[game] " } else { "" },
                c.path,
                props.join(", ")
            );
        }
        if let Ok(class) = std::env::var("JB_UNREAL_CLASS") {
            let only = std::env::var("JB_UNREAL_ONLY").ok();
            for i in r.instances(&class, only.as_deref(), 3).expect("instances") {
                println!("instance {} ({}) at {:#X}", i.name, i.class, i.address);
                for (n, t, a, v) in i.values.iter().take(25) {
                    println!("    {n}: {t} at {a:#X} = {v}");
                }
            }
        }
    }
}
