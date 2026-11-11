//! Helpers for the v1.21.0 tools that make the AI helper better at finding
//! cheats (PLAN.md §5.6): the values around one it found (`read_struct`),
//! what's already known for a game (`known_cheats`) and short notes kept
//! between sessions (`note_save`).

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// The key a tool result uses to hand the protocol a PNG (base64).
pub const IMAGE_KEY: &str = "__png_base64";

/// Standard base64 (for pictures in tool results).
pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

// ----- the values around a value ----------------------------------------------------------------

fn plausible_int(v: i32) -> bool {
    v != 0 && v.unsigned_abs() <= 10_000_000
}

fn plausible_float(v: f64) -> bool {
    v.is_finite() && v != 0.0 && (1e-3..=1e7).contains(&v.abs())
}

/// The values around `at` (read from `start`): one row per 4 bytes that
/// holds something, as a whole number, a decimal and (8-byte aligned) a
/// double or a pointer, with a hint for what it may be. `found` is the
/// value at `at` (as a whole number), so "same as" rows stand out (often the
/// maximum, e.g. max health next to health).
pub fn struct_rows(
    bytes: &[u8],
    ok: &[bool],
    start: u64,
    at: u64,
    ptr64: bool,
    is_pointer: impl Fn(u64) -> bool,
) -> Vec<Value> {
    let found = at
        .checked_sub(start)
        .and_then(|o| bytes.get(o as usize..o as usize + 4))
        .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let mut rows = Vec::new();
    let mut i = 0usize;
    while i + 4 <= bytes.len() {
        if !ok[i..i + 4].iter().all(|x| *x) {
            i += 4;
            continue;
        }
        let addr = start + i as u64;
        let b4 = [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]];
        let whole = i32::from_le_bytes(b4);
        let dec = f64::from(f32::from_le_bytes(b4));
        let wide =
            (addr.is_multiple_of(8) && i + 8 <= bytes.len() && ok[i..i + 8].iter().all(|x| *x))
                .then(|| {
                    let mut b8 = [0u8; 8];
                    b8.copy_from_slice(&bytes[i..i + 8]);
                    b8
                });
        let ptr = wide
            .filter(|_| ptr64)
            .map(u64::from_le_bytes)
            .or_else(|| (!ptr64).then_some(u64::from(u32::from_le_bytes(b4))))
            .filter(|p| *p > 0x10000 && is_pointer(*p));
        let dbl = wide.map(f64::from_le_bytes).filter(|d| plausible_float(*d));
        let offset = addr as i64 - at as i64;
        let mut row = serde_json::Map::new();
        row.insert(
            "offset".into(),
            json!(if offset < 0 {
                format!("-0x{:X}", -offset)
            } else {
                format!("+0x{offset:X}")
            }),
        );
        row.insert("address".into(), json!(format!("0x{addr:X}")));
        let mut hint = Vec::new();
        if let Some(p) = ptr {
            row.insert("pointer".into(), json!(format!("0x{p:X}")));
            hint.push("a pointer (to another object)");
        } else {
            if plausible_int(whole) {
                row.insert("i32".into(), json!(whole));
            }
            if plausible_float(dec) && !plausible_int(whole) {
                row.insert("f32".into(), json!((dec * 1000.0).round() / 1000.0));
                hint.push("a decimal number");
            }
            if let Some(d) = dbl {
                row.insert("f64".into(), json!((d * 1000.0).round() / 1000.0));
            }
        }
        if offset == 0 {
            hint.push("the value you found");
        } else if found.is_some_and(|f| f == whole && plausible_int(whole)) {
            hint.push("same as the value you found (a maximum?)");
        }
        if row.len() > 2 {
            if !hint.is_empty() {
                row.insert("hint".into(), json!(hint.join("; ")));
            }
            rows.push(Value::Object(row));
        }
        i += 4;
    }
    rows
}

// ----- what's known for a game ------------------------------------------------------------------

/// A short summary of one installed pack.
fn pack_summary(pack: &jb_packs::Pack, folder: &str) -> Value {
    let cheats: Vec<Value> = pack
        .cheats
        .iter()
        .take(40)
        .map(|c| match &c.implementation {
            jb_packs::Impl::Value(v) => json!({
                "name": c.name,
                "kind": "value",
                "type": serde_json::to_value(v.ty).unwrap_or_default(),
                "address": serde_json::to_value(&v.address).unwrap_or_default(),
            }),
            jb_packs::Impl::Patch(p) => json!({ "name": c.name, "kind": "patch", "aob": p.aob }),
        })
        .collect();
    json!({
        "folder": folder,
        "game": pack.game.name,
        "madeByAi": pack.meta.is_ai(),
        "builds": pack.game.builds.len(),
        "cheats": cheats,
    })
}

/// Installed packs (in the player's pack folder) for this exe.
pub fn installed_packs(packs_dir: Option<&Path>, exe: &str) -> Vec<Value> {
    let Some(dir) = packs_dir else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let path = e.path().join("pack.json");
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if meta.len() > 2_000_000 {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Ok((pack, _)) = jb_packs::parse(&text)
            && pack.game.exe.eq_ignore_ascii_case(exe)
        {
            out.push(pack_summary(&pack, &e.file_name().to_string_lossy()));
        }
        if out.len() >= 5 {
            break;
        }
    }
    out
}

/// The last downloaded community list, if any.
fn community() -> Option<jb_packs::community::Index> {
    let path = jb_packs::community::cache_path()?;
    let bytes = std::fs::read(path).ok()?;
    jb_packs::community::parse_index(&bytes)
        .ok()
        .map(|(i, _)| i)
}

/// What the community list has for this exe: packs and table links.
pub fn community_for_exe(exe: &str) -> Value {
    let Some(index) = community() else {
        return json!({ "note": "the community list hasn't been downloaded on this PC yet" });
    };
    let packs: Vec<Value> = index
        .packs
        .iter()
        .filter(|p| p.exe.eq_ignore_ascii_case(exe))
        .map(|p| json!({ "name": p.name, "cheats": p.cheats, "author": p.author, "id": p.id }))
        .collect();
    let tables: Vec<Value> = index
        .tables
        .iter()
        .filter(|t| {
            t.exe
                .as_deref()
                .is_some_and(|e| e.eq_ignore_ascii_case(exe))
        })
        .map(|t| json!({ "title": t.title, "by": t.by, "cheats": t.cheats }))
        .collect();
    json!({ "packs": packs, "tableLinks": tables })
}

/// What the community list has for a web game's site.
pub fn community_for_site(host: &str) -> Value {
    let Some(index) = community() else {
        return json!({ "note": "the community list hasn't been downloaded on this PC yet" });
    };
    let web: Vec<Value> = index
        .web
        .iter()
        .filter(|w| jb_packs::web::host(&w.url).is_some_and(|h| same_site(&h, host)))
        .map(|w| json!({ "game": w.game, "by": w.by, "cheats": w.cheats }))
        .collect();
    json!({ "web": web })
}

fn same_site(a: &str, b: &str) -> bool {
    a.trim_start_matches("www.") == b.trim_start_matches("www.")
}

// ----- notes between sessions -------------------------------------------------------------------

/// Longest note kept, in characters.
pub const MAX_NOTE: usize = 600;
/// Notes kept per game or engine (the newest).
const KEEP: usize = 10;

/// Where the helper keeps its notes (`%LOCALAPPDATA%\JuiceBox Trainer\ai-notes`).
pub fn notes_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| {
        PathBuf::from(d)
            .join(jb_core::names::FOLDER)
            .join("ai-notes")
    })
}

/// A safe file name for a note key (`exe:game.exe`, `engine:unity-mono`,
/// `site:example.com`).
fn note_file(dir: &Path, key: &str) -> PathBuf {
    let slug: String = key
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(100)
        .collect();
    dir.join(format!("{slug}.json"))
}

/// The notes kept for a key, oldest first.
pub fn read_notes(dir: Option<&Path>, key: &str) -> Vec<Value> {
    let Some(dir) = dir else {
        return Vec::new();
    };
    std::fs::read_to_string(note_file(dir, key))
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<Value>>(&t).ok())
        .unwrap_or_default()
}

/// Adds a note for each key (keeping the newest few). The text is cut to
/// `MAX_NOTE` characters.
pub fn save_note(
    dir: Option<&Path>,
    keys: &[String],
    text: &str,
    date: &str,
) -> Result<usize, String> {
    let dir = dir.ok_or("no place for notes on this PC")?;
    let text: String = text
        .trim()
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(MAX_NOTE)
        .collect();
    if text.is_empty() {
        return Err("the note is empty".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for key in keys {
        let mut notes = read_notes(Some(dir), key);
        notes.push(json!({ "date": date, "text": text }));
        let skip = notes.len().saturating_sub(KEEP);
        let notes: Vec<Value> = notes.into_iter().skip(skip).collect();
        let path = note_file(dir, key);
        let tmp = path.with_extension("tmp");
        std::fs::write(
            &tmp,
            serde_json::to_string_pretty(&notes).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    }
    Ok(keys.len())
}

/// The notes for these keys, labelled as earlier sessions' notes.
pub fn notes_for(dir: Option<&Path>, keys: &[String]) -> Value {
    let mut out = serde_json::Map::new();
    for key in keys {
        let notes = read_notes(dir, key);
        if !notes.is_empty() {
            out.insert(key.clone(), json!(notes));
        }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn shows_the_values_around_a_value() {
        // health 75, max health 100, ammo 30, gold 250, speed 1.5 (like the testbed), then a pointer.
        let mut b = Vec::new();
        for v in [75i32, 100, 30, 250] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&1.5f32.to_le_bytes());
        b.extend_from_slice(&[0; 4]);
        b.extend_from_slice(&0x7FF6_1234_0000u64.to_le_bytes());
        let ok = vec![true; b.len()];
        let rows = struct_rows(&b, &ok, 0x1000, 0x1000, true, |p| p == 0x7FF6_1234_0000);
        let at = |o: &str| {
            rows.iter()
                .find(|r| r["offset"] == o)
                .cloned()
                .unwrap_or_default()
        };
        assert_eq!(at("+0x0")["i32"], 75);
        assert_eq!(at("+0x0")["hint"], "the value you found");
        assert_eq!(at("+0x4")["i32"], 100);
        assert_eq!(at("+0xC")["i32"], 250);
        assert_eq!(at("+0x10")["f32"], 1.5);
        assert!(at("+0x18")["pointer"].is_string());
        assert!(
            rows.iter().all(|r| r["offset"] != "+0x14"),
            "zeros are left out"
        );
        // A maximum right next to it stands out.
        let mut m = Vec::new();
        for v in [100i32, 100] {
            m.extend_from_slice(&v.to_le_bytes());
        }
        let rows = struct_rows(&m, &[true; 8], 0x2000, 0x2000, true, |_| false);
        assert!(rows[1]["hint"].as_str().unwrap().contains("same as"));
    }

    #[test]
    fn keeps_short_notes_per_game_and_engine() {
        let dir = std::env::temp_dir().join(format!("ot-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let keys = vec!["exe:Game.exe".to_string(), "engine:unity-mono".to_string()];
        for n in 0..12 {
            save_note(Some(&dir), &keys, &format!("note {n}"), "2026-11-10").unwrap();
        }
        let notes = read_notes(Some(&dir), "exe:game.exe");
        assert_eq!(notes.len(), 10);
        assert_eq!(notes[9]["text"], "note 11");
        let long = "x".repeat(5000);
        save_note(Some(&dir), &keys[..1], &long, "2026-11-10").unwrap();
        assert_eq!(
            read_notes(Some(&dir), "exe:game.exe")[9]["text"]
                .as_str()
                .unwrap()
                .len(),
            MAX_NOTE
        );
        assert!(save_note(Some(&dir), &keys, "  ", "2026-11-10").is_err());
        // Keys can't leave the folder.
        let file = note_file(&dir, "exe:..\\..\\evil");
        assert_eq!(file.parent(), Some(dir.as_path()));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
