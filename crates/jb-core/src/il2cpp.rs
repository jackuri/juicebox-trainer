//! Class and field names from a Unity IL2CPP game's
//! `<Game>_Data\il2cpp_data\Metadata\global-metadata.dat`. Only the file's
//! bytes are parsed; nothing is decompiled, run or saved.
//!
//! The metadata layout changes between Unity versions (format 24 to 39+). So
//! the reader doesn't trust one fixed layout: it finds where each class
//! record keeps its field count by checking which position adds up to the
//! file's total number of fields, and where it keeps its first field by
//! checking which position is consistent with those counts. Field *types*
//! live in GameAssembly.dll and aren't read here, so fields come back with
//! type `?`.

use std::path::{Path, PathBuf};

use crate::dotnet::{ClassInfo, FieldInfo};
use crate::error::{Error, Result};

const SANITY: u32 = 0xFAB1_1BAF;
/// Section numbers (the same order in formats 24–39).
const STRINGS: usize = 2;
const FIELDS: usize = 11;
const TYPE_DEFS: usize = 19;

pub const MAX_METADATA_BYTES: u64 = 512 << 20;

fn bad(why: impl Into<String>) -> Error {
    Error::Debugger(format!("IL2CPP metadata: {}", why.into()))
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

#[derive(Debug, Clone, Copy)]
struct Section {
    offset: usize,
    size: usize,
    /// Entries, when the header says (format 38+).
    count: Option<usize>,
}

/// The metadata format version, if `bytes` is IL2CPP metadata.
pub fn version(bytes: &[u8]) -> Option<i32> {
    (u32_at(bytes, 0)? == SANITY).then(|| u32_at(bytes, 4).map(|v| v as i32))?
}

fn sections(b: &[u8]) -> Result<(i32, Vec<Section>)> {
    if u32_at(b, 0) != Some(SANITY) {
        return Err(bad(
            "not IL2CPP metadata (it may be protected or encrypted by the game; JuiceBox Trainer doesn't decrypt it)",
        ));
    }
    let version = u32_at(b, 4).ok_or_else(|| bad("truncated"))? as i32;
    if !(24..=60).contains(&version) {
        return Err(bad(format!("format {version} isn't supported")));
    }
    let triples = version >= 38;
    let step = if triples { 12 } else { 8 };
    let mut out = Vec::new();
    let mut at = 8;
    // The first section starts where the header ends.
    let header_end = u32_at(b, 8).ok_or_else(|| bad("truncated"))? as usize;
    while at + step <= header_end.min(b.len()) && out.len() < 64 {
        let offset = u32_at(b, at).ok_or_else(|| bad("truncated"))? as usize;
        let size = u32_at(b, at + 4).ok_or_else(|| bad("truncated"))? as usize;
        let count = if triples {
            u32_at(b, at + 8).map(|c| c as usize)
        } else {
            None
        };
        if offset + size > b.len() {
            return Err(bad("a section goes past the end of the file"));
        }
        out.push(Section {
            offset,
            size,
            count,
        });
        at += step;
    }
    if out.len() <= TYPE_DEFS {
        return Err(bad("too few sections"));
    }
    Ok((version, out))
}

/// Where a class record keeps its name, field count and first field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Layout {
    size: usize,
    field_count: usize,
    field_start: usize,
}

/// Finds the record layout by checking which positions agree with the
/// file's totals.
fn calibrate(b: &[u8], types: Section, total_fields: usize, sizes: &[usize]) -> Option<Layout> {
    for &size in sizes {
        if size < 24 || !types.size.is_multiple_of(size) {
            continue;
        }
        let n = types.count.unwrap_or(types.size / size);
        if n == 0 || n * size != types.size {
            continue;
        }
        let rec = |i: usize| &b[types.offset + i * size..types.offset + (i + 1) * size];
        // u16 counts: the position whose sum is the number of fields.
        let count_pos: Vec<usize> = (8..size - 1)
            .step_by(2)
            .filter(|&p| {
                (0..n)
                    .map(|i| usize::from(u16_at(rec(i), p).unwrap_or(0)))
                    .sum::<usize>()
                    == total_fields
            })
            .collect();
        for &fc in &count_pos {
            // fieldStart: the u32 whose ranges (start .. start + count)
            // cover every field exactly once. Classes needn't be stored in
            // field order.
            for fs in (8..size - 3).step_by(2) {
                if fs == fc || fs + 4 > size {
                    continue;
                }
                let mut seen = vec![false; total_fields];
                let ok = (0..n).all(|i| {
                    let r = rec(i);
                    let count = usize::from(u16_at(r, fc).unwrap_or(0));
                    if count == 0 {
                        return true;
                    }
                    let start = u32_at(r, fs).unwrap_or(u32::MAX) as usize;
                    if start.saturating_add(count) > total_fields {
                        return false;
                    }
                    seen[start..start + count]
                        .iter_mut()
                        .all(|s| !std::mem::replace(s, true))
                });
                if ok && seen.iter().all(|&s| s) {
                    return Some(Layout {
                        size,
                        field_count: fc,
                        field_start: fs,
                    });
                }
            }
        }
    }
    None
}

/// How many classes (type definitions) the metadata lists: the length of IL2CPP's table of
/// loaded classes in the running game.
pub fn type_count(b: &[u8]) -> Result<usize> {
    let (_, secs) = sections(b)?;
    if let Some(c) = secs[TYPE_DEFS].count.filter(|c| *c > 0) {
        return Ok(c);
    }
    // Older formats don't say: the number of class records read.
    Ok(read_classes_raw(b)?.1)
}

/// Reads class and field names from metadata bytes.
pub fn read_classes(b: &[u8]) -> Result<Vec<ClassInfo>> {
    Ok(read_classes_raw(b)?.0)
}

/// The classes, and how many records there are (some are skipped, like `<Module>`).
fn read_classes_raw(b: &[u8]) -> Result<(Vec<ClassInfo>, usize)> {
    let (version, secs) = sections(b)?;
    let strings = secs[STRINGS];
    let fields = secs[FIELDS];
    let types = secs[TYPE_DEFS];
    // Field records: name, type, token (12 bytes; 16 in format 24.0).
    let field_size = match fields.count {
        Some(c) if c > 0 => fields.size / c,
        _ if version == 24 && fields.size.is_multiple_of(16) && !fields.size.is_multiple_of(12) => {
            16
        }
        _ => 12,
    };
    if field_size < 8 || !fields.size.is_multiple_of(field_size) {
        return Err(bad("unexpected field records"));
    }
    let total_fields = fields.size / field_size;
    let sizes: Vec<usize> = match types.count {
        Some(c) if c > 0 => vec![types.size / c],
        _ => (64..=160).step_by(2).collect(),
    };
    let layout = calibrate(b, types, total_fields, &sizes).ok_or_else(|| {
        bad(format!(
            "couldn't make sense of format {version}'s class records"
        ))
    })?;
    let string = |i: u32| -> String {
        let at = strings.offset + i as usize;
        let end = strings.offset + strings.size;
        if at >= end {
            return String::new();
        }
        let s = &b[at..end];
        let len = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        String::from_utf8_lossy(&s[..len]).into_owned()
    };
    let n = types.size / layout.size;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let r = &b[types.offset + i * layout.size..types.offset + (i + 1) * layout.size];
        let name = string(u32_at(r, 0).unwrap_or(u32::MAX));
        let namespace = string(u32_at(r, 4).unwrap_or(u32::MAX));
        if name.is_empty() || name == "<Module>" {
            continue;
        }
        let count = usize::from(u16_at(r, layout.field_count).unwrap_or(0));
        let start = u32_at(r, layout.field_start).unwrap_or(0) as usize;
        let mut list = Vec::with_capacity(count);
        for f in start..(start + count).min(total_fields) {
            let at = fields.offset + f * field_size;
            let fname = string(u32_at(b, at).unwrap_or(u32::MAX));
            list.push(FieldInfo {
                name: fname,
                ty: "?".into(),
                is_static: false,
                is_const: false,
            });
        }
        out.push(ClassInfo {
            namespace,
            name,
            base: None,
            fields: list,
        });
    }
    Ok((out, n))
}

/// The game's metadata file, next to its exe.
pub fn metadata_path(exe: &Path) -> Option<PathBuf> {
    let stem = exe.file_stem()?.to_string_lossy().into_owned();
    let p = exe
        .parent()?
        .join(format!("{stem}_Data"))
        .join("il2cpp_data")
        .join("Metadata")
        .join("global-metadata.dat");
    p.is_file().then_some(p)
}

pub fn read_file(path: &Path) -> Result<Vec<ClassInfo>> {
    let len = std::fs::metadata(path)
        .map_err(|e| bad(format!("{}: {e}", path.display())))?
        .len();
    if len > MAX_METADATA_BYTES {
        return Err(bad("the file is too big"));
    }
    let bytes = std::fs::read(path).map_err(|e| bad(format!("{}: {e}", path.display())))?;
    read_classes(&bytes)
}

/// Writes small metadata files for tests (our own writer; layouts follow
/// format 29 and the newer compact format 39).
#[cfg(test)]
pub(crate) mod writer {
    pub struct Class<'a> {
        pub namespace: &'a str,
        pub name: &'a str,
        pub fields: &'a [&'a str],
    }

    /// `triples`: format 38+ header with counts. `record`: class record
    /// size; field count is a u16 at `fc`, first field a u32 at `fs`.
    pub fn build(
        version: i32,
        triples: bool,
        record: usize,
        fc: usize,
        fs: usize,
        classes: &[Class<'_>],
    ) -> Vec<u8> {
        let mut strings = vec![0u8];
        let add = |s: &str, strings: &mut Vec<u8>| -> u32 {
            let at = strings.len() as u32;
            strings.extend_from_slice(s.as_bytes());
            strings.push(0);
            at
        };
        let mut fields = Vec::new();
        let mut types = Vec::new();
        let mut next_field = 0u32;
        for c in classes {
            let mut r = vec![0xFFu8; record];
            let name = add(c.name, &mut strings);
            let ns = add(c.namespace, &mut strings);
            r[0..4].copy_from_slice(&name.to_le_bytes());
            r[4..8].copy_from_slice(&ns.to_le_bytes());
            // Other u16 counts (methods etc.) are zero.
            for p in (fc - 4..fc + 12).step_by(2) {
                r[p..p + 2].copy_from_slice(&0u16.to_le_bytes());
            }
            r[fc..fc + 2].copy_from_slice(&(c.fields.len() as u16).to_le_bytes());
            let start = if c.fields.is_empty() {
                u32::MAX
            } else {
                next_field
            };
            r[fs..fs + 4].copy_from_slice(&start.to_le_bytes());
            for f in c.fields {
                let n = add(f, &mut strings);
                fields.extend_from_slice(&n.to_le_bytes());
                fields.extend_from_slice(&7u32.to_le_bytes());
                fields.extend_from_slice(&0x0400_0001u32.to_le_bytes());
                next_field += 1;
            }
            types.extend_from_slice(&r);
        }
        let step = if triples { 12 } else { 8 };
        let header = 8 + 31 * step;
        let mut out = Vec::new();
        out.extend_from_slice(&super::SANITY.to_le_bytes());
        out.extend_from_slice(&version.to_le_bytes());
        let mut body = Vec::new();
        let mut entries = Vec::new();
        for i in 0..31 {
            let (data, count): (&[u8], usize) = match i {
                super::STRINGS => (&strings, 0),
                super::FIELDS => (&fields, fields.len() / 12),
                super::TYPE_DEFS => (&types, classes.len()),
                _ => (&[], 0),
            };
            entries.push((
                (header + body.len()) as u32,
                data.len() as u32,
                count as u32,
            ));
            body.extend_from_slice(data);
        }
        for (off, size, count) in entries {
            out.extend_from_slice(&off.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            if triples {
                out.extend_from_slice(&count.to_le_bytes());
            }
        }
        out.extend_from_slice(&body);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::writer::{Class, build};
    use super::*;

    fn sample() -> Vec<Class<'static>> {
        vec![
            Class {
                namespace: "",
                name: "<Module>",
                fields: &[],
            },
            Class {
                namespace: "Game",
                name: "Player",
                fields: &["health", "maxHealth", "gold"],
            },
            Class {
                namespace: "Game",
                name: "Empty",
                fields: &[],
            },
            Class {
                namespace: "Game.Items",
                name: "Inventory",
                fields: &["slots", "coins"],
            },
        ]
    }

    #[test]
    fn reads_compact_format_39() {
        let b = build(39, true, 82, 62, 26, &sample());
        assert_eq!(version(&b), Some(39));
        let classes = read_classes(&b).unwrap();
        let player = classes.iter().find(|c| c.name == "Player").unwrap();
        assert_eq!(player.full_name(), "Game.Player");
        let names: Vec<_> = player.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["health", "maxHealth", "gold"]);
        let inv = classes.iter().find(|c| c.name == "Inventory").unwrap();
        assert_eq!(inv.fields[1].name, "coins");
        assert!(classes.iter().all(|c| c.name != "<Module>"));
        // Every record counts, <Module> too (the running game's table has a slot for each).
        assert_eq!(type_count(&b).unwrap(), sample().len());
        let old = build(29, false, 88, 64, 32, &sample());
        assert_eq!(type_count(&old).unwrap(), sample().len());
    }

    #[test]
    fn reads_older_format_29_without_counts() {
        // Format 29: 88-byte records, counts at 64, first field at 32.
        let b = build(29, false, 88, 64, 32, &sample());
        let classes = read_classes(&b).unwrap();
        let inv = classes.iter().find(|c| c.name == "Inventory").unwrap();
        assert_eq!(inv.fields.len(), 2);
        assert_eq!(inv.fields[0].name, "slots");
    }

    #[test]
    fn refuses_protected_or_odd_files() {
        assert!(
            read_classes(b"not metadata at all")
                .unwrap_err()
                .to_string()
                .contains("encrypted")
        );
        let mut b = build(39, true, 82, 62, 26, &sample());
        b[4..8].copy_from_slice(&7i32.to_le_bytes());
        assert!(read_classes(&b).is_err());
    }
}

/// Manual check against a real game's metadata on this PC (never in CI):
/// `JB_IL2CPP_PROBE=<path to global-metadata.dat> cargo test -p jb-core probe -- --ignored --nocapture`
#[cfg(test)]
mod probe {
    #[test]
    #[ignore = "needs a real game's metadata file"]
    fn probe() {
        let Some(path) = std::env::var_os("JB_IL2CPP_PROBE") else {
            return;
        };
        let bytes = std::fs::read(&path).unwrap();
        println!("format {:?}", super::version(&bytes));
        let classes = super::read_classes(&bytes).unwrap();
        let fields: usize = classes.iter().map(|c| c.fields.len()).sum();
        println!("{} classes, {fields} fields", classes.len());
        let hits = crate::dotnet::search(&classes, Some("health"), 12);
        for h in hits {
            println!("  {}.{}", h.class, h.field);
        }
    }
}
