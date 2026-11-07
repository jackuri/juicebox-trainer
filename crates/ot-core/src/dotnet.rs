//! Reads class and field names from a .NET assembly's metadata (ECMA-335),
//! for Unity (Mono) games (`<Game>_Data\Managed\Assembly-CSharp.dll`) and
//! XNA / FNA / MonoGame games. Only the metadata tables are parsed from the
//! file's bytes: no code is decompiled, loaded or run, and nothing is saved.
//!
//! The result tells the AI helper what a game calls its values and their
//! types ("Player.currentHealth: float"), so it knows what to scan for.

use std::path::Path;

use serde::Serialize;

use crate::error::{Error, Result};

/// Files bigger than this aren't read.
pub const MAX_ASSEMBLY_BYTES: u64 = 256 << 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldInfo {
    pub name: String,
    /// `int`, `float`, `bool`, `string`, a class name, `int[]`, …
    pub ty: String,
    pub is_static: bool,
    /// A constant (`const`), not stored per object.
    pub is_const: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassInfo {
    pub namespace: String,
    pub name: String,
    pub base: Option<String>,
    pub fields: Vec<FieldInfo>,
}

impl ClassInfo {
    pub fn full_name(&self) -> String {
        if self.namespace.is_empty() {
            self.name.clone()
        } else {
            format!("{}.{}", self.namespace, self.name)
        }
    }
}

fn bad(why: impl Into<String>) -> Error {
    Error::Debugger(format!("not a readable .NET assembly: {}", why.into()))
}

struct Rd<'a> {
    b: &'a [u8],
}

impl Rd<'_> {
    fn u16(&self, at: usize) -> Result<u16> {
        let s = self.b.get(at..at + 2).ok_or_else(|| bad("truncated"))?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&self, at: usize) -> Result<u32> {
        let s = self.b.get(at..at + 4).ok_or_else(|| bad("truncated"))?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn u64(&self, at: usize) -> Result<u64> {
        Ok(u64::from(self.u32(at)?) | (u64::from(self.u32(at + 4)?) << 32))
    }
    fn idx(&self, at: usize, wide: bool) -> Result<u32> {
        if wide {
            self.u32(at)
        } else {
            self.u16(at).map(u32::from)
        }
    }
}

/// (virtual address, size, raw data offset) of a section.
type Section = (u32, u32, u32);

/// The sections, and the CLI header's address.
fn sections(r: &Rd<'_>) -> Result<(Vec<Section>, usize)> {
    if r.b.get(..2) != Some(b"MZ") {
        return Err(bad("no MZ header"));
    }
    let pe = r.u32(0x3C)? as usize;
    if r.b.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err(bad("no PE header"));
    }
    let coff = pe + 4;
    let count = r.u16(coff + 2)? as usize;
    let opt_size = r.u16(coff + 16)? as usize;
    let opt = coff + 20;
    let magic = r.u16(opt)?;
    let dirs = match magic {
        0x10B => opt + 96,
        0x20B => opt + 112,
        _ => return Err(bad("unknown optional header")),
    };
    // Data directory 14: the CLI header.
    let cli_rva = r.u32(dirs + 14 * 8)?;
    if cli_rva == 0 {
        return Err(bad("no .NET (CLI) header; not a managed assembly"));
    }
    let table = opt + opt_size;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let s = table + i * 40;
        out.push((
            r.u32(s + 12)?,
            r.u32(s + 8)?.max(r.u32(s + 16)?),
            r.u32(s + 20)?,
        ));
    }
    Ok((out, cli_rva as usize))
}

fn file_offset(secs: &[Section], rva: usize) -> Result<usize> {
    secs.iter()
        .find(|(va, size, _)| rva >= *va as usize && rva < (*va + *size) as usize)
        .map(|(va, _, raw)| rva - *va as usize + *raw as usize)
        .ok_or_else(|| bad("address outside every section"))
}

/// A compressed unsigned integer (ECMA-335 II.23.2).
fn compressed(b: &[u8], at: &mut usize) -> Option<u32> {
    let first = *b.get(*at)?;
    let (len, v) = if first & 0x80 == 0 {
        (1, u32::from(first))
    } else if first & 0xC0 == 0x80 {
        (
            2,
            (u32::from(first & 0x3F) << 8) | u32::from(*b.get(*at + 1)?),
        )
    } else if first & 0xE0 == 0xC0 {
        let s = b.get(*at + 1..*at + 4)?;
        (
            4,
            (u32::from(first & 0x1F) << 24)
                | (u32::from(s[0]) << 16)
                | (u32::from(s[1]) << 8)
                | u32::from(s[2]),
        )
    } else {
        return None;
    };
    *at += len;
    Some(v)
}

const MODULE: usize = 0x00;
const TYPE_REF: usize = 0x01;
const TYPE_DEF: usize = 0x02;
const FIELD_PTR: usize = 0x03;
const FIELD: usize = 0x04;
const METHOD_DEF: usize = 0x06;
const MODULE_REF: usize = 0x1A;
const TYPE_SPEC: usize = 0x1B;
const ASSEMBLY_REF: usize = 0x23;

struct Tables<'a> {
    r: Rd<'a>,
    rows: [u32; 64],
    strings: usize,
    strings_len: usize,
    blob: usize,
    blob_len: usize,
    wide_str: bool,
    wide_guid: bool,
    wide_blob: bool,
    /// Start of each table's rows (file offset) and its row size.
    start: [usize; 64],
    size: [usize; 64],
}

impl Tables<'_> {
    fn simple(&self, t: usize) -> usize {
        if self.rows[t] < 1 << 16 { 2 } else { 4 }
    }
    fn coded(&self, tables: &[usize], bits: u32) -> usize {
        let max = tables.iter().map(|&t| self.rows[t]).max().unwrap_or(0);
        if max < 1 << (16 - bits) { 2 } else { 4 }
    }
    fn s(&self) -> usize {
        if self.wide_str { 4 } else { 2 }
    }
    fn g(&self) -> usize {
        if self.wide_guid { 4 } else { 2 }
    }
    fn bl(&self) -> usize {
        if self.wide_blob { 4 } else { 2 }
    }
    fn string(&self, at: u32) -> String {
        let at = at as usize;
        if at >= self.strings_len {
            return String::new();
        }
        let b = &self.r.b[self.strings + at..self.strings + self.strings_len];
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        String::from_utf8_lossy(&b[..end]).into_owned()
    }
    fn blob(&self, at: u32) -> &[u8] {
        let mut p = at as usize;
        if p >= self.blob_len {
            return &[];
        }
        let heap = &self.r.b[self.blob..self.blob + self.blob_len];
        let Some(len) = compressed(heap, &mut p) else {
            return &[];
        };
        heap.get(p..p + len as usize).unwrap_or(&[])
    }
    fn row(&self, t: usize, i: u32) -> usize {
        self.start[t] + (i as usize - 1) * self.size[t]
    }
}

fn type_name(t: &Tables<'_>, coded: u32) -> Option<String> {
    // TypeDefOrRef: 2 tag bits (TypeDef, TypeRef, TypeSpec).
    let (tag, i) = (coded & 3, coded >> 2);
    let s = t.s();
    match tag {
        0 if i >= 1 && i <= t.rows[TYPE_DEF] => {
            let row = t.row(TYPE_DEF, i);
            Some(t.string(t.r.idx(row + 4, s == 4).ok()?))
        }
        1 if i >= 1 && i <= t.rows[TYPE_REF] => {
            let row = t.row(TYPE_REF, i);
            let scope = t.coded(&[MODULE, MODULE_REF, ASSEMBLY_REF, TYPE_REF], 2);
            Some(t.string(t.r.idx(row + scope, s == 4).ok()?))
        }
        2 => Some("(generic)".into()),
        _ => None,
    }
}

/// Decodes a type from a signature.
fn sig_type(t: &Tables<'_>, b: &[u8], at: &mut usize, depth: u32) -> String {
    if depth > 8 {
        return "?".into();
    }
    let Some(&e) = b.get(*at) else {
        return "?".into();
    };
    *at += 1;
    let simple = match e {
        0x02 => "bool",
        0x03 => "char",
        0x04 => "sbyte",
        0x05 => "byte",
        0x06 => "short",
        0x07 => "ushort",
        0x08 => "int",
        0x09 => "uint",
        0x0A => "long",
        0x0B => "ulong",
        0x0C => "float",
        0x0D => "double",
        0x0E => "string",
        0x18 => "nint",
        0x19 => "nuint",
        0x1C => "object",
        _ => "",
    };
    if !simple.is_empty() {
        return simple.into();
    }
    match e {
        0x11 | 0x12 => compressed(b, at)
            .and_then(|c| type_name(t, c))
            .unwrap_or_else(|| "?".into()),
        0x1D => format!("{}[]", sig_type(t, b, at, depth + 1)),
        0x0F => format!("{}*", sig_type(t, b, at, depth + 1)),
        0x13 | 0x1E => {
            compressed(b, at);
            "T".into()
        }
        0x15 => {
            let base = sig_type(t, b, at, depth + 1);
            let n = compressed(b, at).unwrap_or(0).min(8);
            let args: Vec<String> = (0..n).map(|_| sig_type(t, b, at, depth + 1)).collect();
            let base = base.split('`').next().unwrap_or(&base).to_string();
            format!("{base}<{}>", args.join(", "))
        }
        0x14 => {
            let inner = sig_type(t, b, at, depth + 1);
            format!("{inner}[,]")
        }
        // Custom modifiers: skip the type token and read on.
        0x1F | 0x20 => {
            compressed(b, at);
            sig_type(t, b, at, depth + 1)
        }
        _ => "?".into(),
    }
}

/// Reads every class with its fields from an assembly's bytes.
pub fn read_classes(bytes: &[u8]) -> Result<Vec<ClassInfo>> {
    let r = Rd { b: bytes };
    let (secs, cli_rva) = sections(&r)?;
    let cli = file_offset(&secs, cli_rva)?;
    let md_rva = r.u32(cli + 8)? as usize;
    let md = file_offset(&secs, md_rva)?;
    if r.u32(md)? != 0x424A_5342 {
        return Err(bad("no metadata signature"));
    }
    let ver_len = r.u32(md + 12)? as usize;
    let mut p = md + 16 + ver_len;
    let streams = r.u16(p + 2)? as usize;
    p += 4;
    let (mut tilde, mut strings, mut blob) = (None, None, None);
    for _ in 0..streams {
        let off = r.u32(p)? as usize;
        let size = r.u32(p + 4)? as usize;
        let name_at = p + 8;
        let name_end = bytes[name_at..]
            .iter()
            .position(|&c| c == 0)
            .ok_or_else(|| bad("stream name"))?;
        let name = &bytes[name_at..name_at + name_end];
        match name {
            b"#~" | b"#-" => tilde = Some((md + off, size)),
            b"#Strings" => strings = Some((md + off, size)),
            b"#Blob" => blob = Some((md + off, size)),
            _ => {}
        }
        p = name_at + (name_end + 4) / 4 * 4;
    }
    let (tilde, _) = tilde.ok_or_else(|| bad("no tables stream"))?;
    let (strings, strings_len) = strings.ok_or_else(|| bad("no strings heap"))?;
    let (blob, blob_len) = blob.unwrap_or((0, 0));
    if strings + strings_len > bytes.len() || blob + blob_len > bytes.len() {
        return Err(bad("heap outside the file"));
    }
    let heap_sizes = bytes
        .get(tilde + 6)
        .copied()
        .ok_or_else(|| bad("truncated"))?;
    let valid = r.u64(tilde + 8)?;
    let mut rows = [0u32; 64];
    let mut q = tilde + 24;
    for (i, row) in rows.iter_mut().enumerate() {
        if valid & (1 << i) != 0 {
            *row = r.u32(q)?;
            q += 4;
        }
    }
    let mut t = Tables {
        r,
        rows,
        strings,
        strings_len,
        blob,
        blob_len,
        wide_str: heap_sizes & 1 != 0,
        wide_guid: heap_sizes & 2 != 0,
        wide_blob: heap_sizes & 4 != 0,
        start: [0; 64],
        size: [0; 64],
    };
    let (s, g, bl) = (t.s(), t.g(), t.bl());
    let size_of = |t: &Tables<'_>, table: usize| -> usize {
        match table {
            MODULE => 2 + s + 3 * g,
            TYPE_REF => t.coded(&[MODULE, MODULE_REF, ASSEMBLY_REF, TYPE_REF], 2) + 2 * s,
            TYPE_DEF => {
                4 + 2 * s
                    + t.coded(&[TYPE_DEF, TYPE_REF, TYPE_SPEC], 2)
                    + t.simple(FIELD)
                    + t.simple(METHOD_DEF)
            }
            FIELD_PTR => t.simple(FIELD),
            FIELD => 2 + s + bl,
            _ => 0,
        }
    };
    // Only the first five tables are needed, and they come first.
    let mut at = q;
    for table in [MODULE, TYPE_REF, TYPE_DEF, FIELD_PTR, FIELD] {
        t.size[table] = size_of(&t, table);
        t.start[table] = at;
        at += t.size[table] * t.rows[table] as usize;
    }
    if at > bytes.len() {
        return Err(bad("tables outside the file"));
    }
    let field_wide = t.simple(FIELD) == 4;
    let extends_size = t.coded(&[TYPE_DEF, TYPE_REF, TYPE_SPEC], 2);
    let mut out = Vec::new();
    let types = t.rows[TYPE_DEF];
    for i in 1..=types {
        let row = t.row(TYPE_DEF, i);
        let name = t.string(t.r.idx(row + 4, s == 4)?);
        let namespace = t.string(t.r.idx(row + 4 + s, s == 4)?);
        let extends = t.r.idx(row + 4 + 2 * s, extends_size == 4)?;
        let first = t.r.idx(row + 4 + 2 * s + extends_size, field_wide)?;
        let next = if i < types {
            t.r.idx(
                t.row(TYPE_DEF, i + 1) + 4 + 2 * s + extends_size,
                field_wide,
            )?
        } else {
            t.rows[FIELD] + 1
        };
        let mut fields = Vec::new();
        for f in first..next.min(t.rows[FIELD] + 1) {
            let fi = if t.rows[FIELD_PTR] > 0 {
                t.r.idx(t.row(FIELD_PTR, f), field_wide)?
            } else {
                f
            };
            if fi == 0 || fi > t.rows[FIELD] {
                continue;
            }
            let frow = t.row(FIELD, fi);
            let flags = t.r.u16(frow)?;
            let fname = t.string(t.r.idx(frow + 2, s == 4)?);
            let sig = t.blob(t.r.idx(frow + 2 + s, bl == 4)?);
            let ty = if sig.first() == Some(&0x06) {
                let mut at = 1;
                sig_type(&t, sig, &mut at, 0)
            } else {
                "?".into()
            };
            fields.push(FieldInfo {
                name: fname,
                ty,
                is_static: flags & 0x10 != 0,
                is_const: flags & 0x40 != 0,
            });
        }
        if name == "<Module>" {
            continue;
        }
        out.push(ClassInfo {
            namespace,
            name,
            base: if extends == 0 {
                None
            } else {
                type_name(&t, extends)
            },
            fields,
        });
    }
    Ok(out)
}

/// Reads an assembly file.
pub fn read_assembly(path: &Path) -> Result<Vec<ClassInfo>> {
    let len = std::fs::metadata(path)
        .map_err(|e| bad(format!("{}: {e}", path.display())))?
        .len();
    if len > MAX_ASSEMBLY_BYTES {
        return Err(bad("the file is too big"));
    }
    let bytes = std::fs::read(path).map_err(|e| bad(format!("{}: {e}", path.display())))?;
    read_classes(&bytes)
}

/// The game's own .NET assemblies: Unity's `Assembly-CSharp*.dll`, or for
/// XNA-style games the exe and the game's other managed DLLs.
pub fn game_assemblies(exe: &Path) -> Vec<std::path::PathBuf> {
    let Some(dir) = exe.parent() else {
        return Vec::new();
    };
    let stem = exe
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let managed = dir.join(format!("{stem}_Data")).join("Managed");
    let unity: Vec<_> = ["Assembly-CSharp.dll", "Assembly-CSharp-firstpass.dll"]
        .iter()
        .map(|f| managed.join(f))
        .filter(|p| p.is_file())
        .collect();
    if !unity.is_empty() {
        return unity;
    }
    let mut out = vec![exe.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut dlls: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dll"))
                    && !p.file_name().is_some_and(|n| {
                        let n = n.to_string_lossy().to_ascii_lowercase();
                        n.starts_with("system.")
                            || n.starts_with("microsoft.")
                            || n.starts_with("monogame")
                            || n.starts_with("fna")
                            || n.starts_with("mscorlib")
                    })
            })
            .collect();
        dlls.sort();
        out.extend(dlls.into_iter().take(20));
    }
    out
}

/// A field match for a search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldHit {
    pub class: String,
    pub field: String,
    pub ty: String,
    pub is_static: bool,
}

/// Words that usually name values players want to change.
pub const INTERESTING: &[&str] = &[
    "health", "hp", "life", "lives", "gold", "money", "coin", "cash", "ammo", "bullet", "xp",
    "exp", "level", "stamina", "mana", "energy", "speed", "score", "damage", "armor", "shield",
    "potion", "item",
];

fn numeric(ty: &str) -> bool {
    matches!(
        ty,
        "int"
            | "uint"
            | "long"
            | "ulong"
            | "short"
            | "ushort"
            | "byte"
            | "sbyte"
            | "float"
            | "double"
            | "bool"
    )
}

/// `<Health>k__BackingField` (a C# property's storage) → `Health`; other
/// compiler-made names → `None`.
pub fn plain_name(name: &str) -> Option<&str> {
    match name.strip_prefix('<') {
        None => Some(name),
        Some(rest) => rest
            .strip_suffix(">k__BackingField")
            .filter(|n| !n.contains('<')),
    }
}

/// Instance fields whose class or field name contains `query` (or, with no
/// query, an interesting word), numbers first.
pub fn search(classes: &[ClassInfo], query: Option<&str>, limit: usize) -> Vec<FieldHit> {
    let q = query.map(str::to_lowercase);
    // (field name matched, is a number, hit)
    let mut hits: Vec<(bool, bool, FieldHit)> = Vec::new();
    for c in classes {
        // Compiler-made helper classes (closures, iterators, async state).
        if c.name.contains('<') {
            continue;
        }
        let cname = c.full_name();
        let lc = cname.to_lowercase();
        for f in &c.fields {
            let Some(fname) = plain_name(&f.name) else {
                continue;
            };
            if f.is_const {
                continue;
            }
            let lf = fname.to_lowercase();
            let in_field = match &q {
                Some(q) => lf.contains(q),
                None => INTERESTING.iter().any(|w| lf.contains(w)),
            };
            let matched = in_field || q.as_ref().is_some_and(|q| lc.contains(q));
            if matched {
                hits.push((
                    in_field,
                    numeric(&f.ty),
                    FieldHit {
                        class: cname.clone(),
                        field: fname.to_string(),
                        ty: f.ty.clone(),
                        is_static: f.is_static,
                    },
                ));
            }
        }
    }
    hits.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(b.1.cmp(&a.1))
            .then(a.2.class.cmp(&b.2.class))
    });
    hits.into_iter().take(limit).map(|(_, _, h)| h).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_names() {
        assert_eq!(plain_name("gold"), Some("gold"));
        assert_eq!(plain_name("<Health>k__BackingField"), Some("Health"));
        assert_eq!(plain_name("<>4__this"), None);
        let classes = vec![
            ClassInfo {
                namespace: String::new(),
                name: "<>c__DisplayClass1_0".into(),
                base: None,
                fields: vec![FieldInfo {
                    name: "health".into(),
                    ty: "int".into(),
                    is_static: false,
                    is_const: false,
                }],
            },
            ClassInfo {
                namespace: "Game".into(),
                name: "Player".into(),
                base: None,
                fields: vec![FieldInfo {
                    name: "<Health>k__BackingField".into(),
                    ty: "float".into(),
                    is_static: false,
                    is_const: false,
                }],
            },
        ];
        let hits = search(&classes, Some("health"), 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(
            (hits[0].class.as_str(), hits[0].field.as_str()),
            ("Game.Player", "Health")
        );
    }

    #[test]
    fn compressed_ints() {
        let b = [0x03, 0x80, 0x80, 0xC0, 0x00, 0x40, 0x00];
        let mut at = 0;
        assert_eq!(compressed(&b, &mut at), Some(3));
        assert_eq!(compressed(&b, &mut at), Some(0x80));
        assert_eq!(compressed(&b, &mut at), Some(0x4000));
        assert_eq!(at, 7);
    }

    #[test]
    fn refuses_non_dotnet_files() {
        assert!(read_classes(b"not a pe file").is_err());
        let exe = std::env::current_exe().unwrap();
        let e = read_assembly(&exe).unwrap_err().to_string();
        assert!(e.contains("not a managed assembly"), "{e}");
    }

    /// Windows ships .NET Framework assemblies; read one (a system file, not
    /// a game) to check the reader on real metadata.
    #[test]
    fn reads_a_real_assembly() {
        let path = Path::new(r"C:\Windows\Microsoft.NET\Framework64\v4.0.30319\System.dll");
        if !path.is_file() {
            return;
        }
        let classes = read_assembly(path).unwrap();
        assert!(classes.len() > 1000, "{}", classes.len());
        let uri = classes
            .iter()
            .find(|c| c.full_name() == "System.Uri")
            .expect("System.Uri");
        let s = uri
            .fields
            .iter()
            .find(|f| f.name == "m_String")
            .expect("m_String");
        assert_eq!(s.ty, "string");
        assert!(!s.is_static);
        assert!(uri.fields.iter().any(|f| f.is_static));
        let hits = search(&classes, Some("m_String"), 5);
        assert!(hits.iter().any(|h| h.class == "System.Uri"));
        // Some int fields exist and come first in a broad search.
        let hits = search(&classes, Some("count"), 50);
        assert!(hits.first().is_some_and(|h| numeric(&h.ty)), "{hits:?}");
    }
}

/// Manual check against a real game's assembly on this PC (never in CI):
/// `OT_DOTNET_PROBE=<path to Assembly-CSharp.dll> cargo test -p ot-core dotnet_probe -- --ignored --nocapture`
#[cfg(test)]
mod dotnet_probe {
    #[test]
    #[ignore = "needs a real game's assembly"]
    fn dotnet_probe() {
        let Some(path) = std::env::var_os("OT_DOTNET_PROBE") else {
            return;
        };
        let classes = super::read_assembly(std::path::Path::new(&path)).unwrap();
        let fields: usize = classes.iter().map(|c| c.fields.len()).sum();
        println!("{} classes, {fields} fields", classes.len());
        let queries = std::env::var("OT_DOTNET_QUERY").unwrap_or_else(|_| "health,stamina".into());
        for q in queries.split(',') {
            for h in super::search(&classes, Some(q), 6) {
                println!("  {}.{}: {}", h.class, h.field, h.ty);
            }
        }
    }
}
