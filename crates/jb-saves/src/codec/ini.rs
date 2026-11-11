//! INI-style text saves: `[section]` headers and `key = value` lines. Only
//! files where every line is one of those (or a comment) count as INI. Edits
//! replace just the value text.
//!
//! Also covers GameMaker's `ini_write_*` files (numbers in quotes,
//! `gold="250.000000"`) and Godot's `ConfigFile` saves (`[section]`,
//! `key=value` with Godot values; lists and maps over several lines are shown
//! but not edited).

use super::json::Spot;
use super::{Field, FieldKind, MAX_FIELDS};

fn valid_key(k: &str) -> bool {
    !k.is_empty()
        && k.len() <= 128
        && k.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ' ' | '/'))
}

fn valid_section(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.contains(['[', ']'])
}

fn kind_of(v: &str) -> FieldKind {
    if v.parse::<i64>().is_ok() {
        FieldKind::Int
    } else if v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("false") {
        FieldKind::Bool
    } else if v.contains('.') && v.parse::<f64>().is_ok_and(f64::is_finite) {
        FieldKind::Float
    } else {
        FieldKind::Text
    }
}

/// How many brackets are open after `text` (outside quotes), starting from
/// `depth`; `None` if it closes more than were open.
fn open_brackets(text: &str, mut depth: i32, quoted: &mut bool) -> Option<i32> {
    let mut escape = false;
    for c in text.chars() {
        if *quoted {
            match c {
                _ if escape => escape = false,
                '\\' => escape = true,
                '"' => *quoted = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => *quoted = true,
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return None;
        }
    }
    Some(depth)
}

/// A value over several lines is shown up to this many bytes.
const MAX_JOINED: usize = 1 << 20;

/// Every `key = value`, or `None` if this isn't an INI file.
pub(crate) fn parse(b: &[u8]) -> Option<(Vec<Spot>, bool)> {
    let text = std::str::from_utf8(b).ok()?;
    if text.contains('\0') {
        return None;
    }
    let mut section = String::new();
    let mut out = Vec::new();
    let mut truncated = false;
    let mut offset = 0;
    let mut lines = text.split_inclusive('\n');
    while let Some(raw) = lines.next() {
        let line_start = offset;
        offset += raw.len();
        let line = raw.trim_end_matches(['\n', '\r']);
        let t = line.trim();
        if t.is_empty() || t.starts_with(';') || t.starts_with('#') {
            continue;
        }
        if let Some(name) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if !valid_section(name.trim()) {
                return None;
            }
            section = name.trim().to_string();
            continue;
        }
        let eq = line.find('=')?;
        let key = line[..eq].trim();
        if !valid_key(key) {
            return None;
        }
        let after = &line[eq + 1..];
        let value = after.trim();
        let lead = after.len() - after.trim_start().len();
        let start = line_start + eq + 1 + lead;
        let path = if section.is_empty() {
            key.to_string()
        } else {
            format!("{section}.{key}")
        };
        // A list or map that goes on over the next lines (Godot).
        let mut quoted = false;
        let mut depth = if value.starts_with(['{', '[']) {
            open_brackets(value, 0, &mut quoted)
        } else {
            Some(0)
        };
        if depth.is_some_and(|d| d > 0) {
            let mut joined = value.to_string();
            while depth.is_some_and(|d| d > 0) {
                let next = lines.next()?;
                offset += next.len();
                let next = next.trim_end_matches(['\n', '\r']);
                depth = open_brackets(next, depth.unwrap_or(0), &mut quoted);
                if joined.len() < MAX_JOINED {
                    joined.push('\n');
                    joined.push_str(next);
                }
            }
            if depth != Some(0) {
                return None;
            }
            if out.len() >= MAX_FIELDS {
                truncated = true;
                continue;
            }
            out.push(Spot {
                field: Field {
                    path,
                    kind: FieldKind::Other,
                    value: joined,
                    editable: false,
                    note: Some("list or map over several lines".into()),
                },
                start: 0,
                end: 0,
            });
            continue;
        }
        if out.len() >= MAX_FIELDS {
            truncated = true;
            continue;
        }
        // `gold="250.000000"`: the value inside the quotes.
        let inner = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .filter(|v| !v.contains('"'));
        let (shown, start, note) = match inner {
            Some(v) => (v, start + 1, Some("in quotes".to_string())),
            None => (value, start, None),
        };
        out.push(Spot {
            field: Field {
                path,
                kind: kind_of(shown),
                value: shown.to_string(),
                editable: true,
                note,
            },
            start,
            end: start + shown.len(),
        });
    }
    (!out.is_empty()).then_some((out, truncated))
}

pub(crate) fn set(b: &[u8], path: &str, value: &str) -> Result<Vec<u8>, String> {
    let (spots, _) = parse(b).ok_or("not an INI file")?;
    let spot = spots
        .iter()
        .find(|s| s.field.path == path)
        .ok_or_else(|| format!("no value at {path}"))?;
    if !spot.field.editable {
        return Err("this value can't be edited".into());
    }
    let v = value.trim();
    if v.contains(['\n', '\r']) {
        return Err("a value can't contain line breaks".into());
    }
    if spot.field.note.is_some() && v.contains('"') {
        return Err("a value in quotes can't contain quotes".into());
    }
    match spot.field.kind {
        FieldKind::Int if v.parse::<i64>().is_err() => {
            return Err(format!("{value:?} isn't a whole number"));
        }
        FieldKind::Float if v.parse::<f64>().is_err() => {
            return Err(format!("{value:?} isn't a number"));
        }
        _ => {}
    }
    let out = [&b[..spot.start], v.as_bytes(), &b[spot.end..]].concat();
    if parse(&out).is_none() {
        return Err("that value would break the file".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INI: &str = "; settings\r\n[Player]\r\nGold = 250\r\nName=Tester\r\nSpeed = 1.5\r\n\r\n[Audio]\r\nMuted = false\r\n";

    #[test]
    fn reads_and_edits_ini() {
        let (spots, _) = parse(INI.as_bytes()).unwrap();
        let paths: Vec<_> = spots
            .iter()
            .map(|s| (s.field.path.as_str(), s.field.kind))
            .collect();
        assert_eq!(
            paths,
            vec![
                ("Player.Gold", FieldKind::Int),
                ("Player.Name", FieldKind::Text),
                ("Player.Speed", FieldKind::Float),
                ("Audio.Muted", FieldKind::Bool)
            ]
        );
        let out = set(INI.as_bytes(), "Player.Gold", "9999").unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            INI.replace("Gold = 250", "Gold = 9999")
        );
        assert!(set(INI.as_bytes(), "Player.Gold", "x").is_err());
        assert!(set(INI.as_bytes(), "Player.Name", "a\nb").is_err());
    }

    #[test]
    fn reads_game_maker_quoted_numbers() {
        let gm = "[Player]\r\ngold=\"250.000000\"\r\nname=\"Hero\"\r\n";
        let (spots, _) = parse(gm.as_bytes()).unwrap();
        assert_eq!(spots[0].field.value, "250.000000");
        assert_eq!(spots[0].field.kind, FieldKind::Float);
        assert_eq!(spots[1].field.value, "Hero");
        let out = set(gm.as_bytes(), "Player.gold", "99999").unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            gm.replace("\"250.000000\"", "\"99999\"")
        );
        assert!(set(gm.as_bytes(), "Player.name", "a\"b").is_err());
    }

    #[test]
    fn reads_godot_config_files() {
        let godot = "[player]\n\ngold=250\nname=\"Hero\"\npos=Vector2(1.5, 2)\ninventory={\n\"potion\": 3,\n\"note\": \"a } b\"\n}\nlevels=[1, 2]\n\n[world]\nday=12\n";
        let (spots, _) = parse(godot.as_bytes()).unwrap();
        let get = |p: &str| {
            spots
                .iter()
                .find(|s| s.field.path == p)
                .unwrap()
                .field
                .clone()
        };
        assert_eq!(get("player.gold").kind, FieldKind::Int);
        assert_eq!(get("player.name").value, "Hero");
        assert_eq!(get("player.pos").value, "Vector2(1.5, 2)");
        let inv = get("player.inventory");
        assert!(!inv.editable && inv.value.contains("potion"));
        assert_eq!(get("player.levels").value, "[1, 2]");
        assert_eq!(get("world.day").value, "12");
        let out = set(godot.as_bytes(), "world.day", "13").unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            godot.replace("day=12", "day=13")
        );
        assert!(set(godot.as_bytes(), "player.inventory", "{}").is_err());
        // A map that never closes isn't INI.
        assert!(parse(b"[a]\nx={\n\"b\": 1\n").is_none());
    }

    #[test]
    fn other_text_is_not_ini() {
        assert!(parse(b"<save gold=\"1\"/>").is_none());
        assert!(parse(b"just some words").is_none());
        assert!(parse(b"gold: 100\nhp: 5").is_none());
        assert!(parse(b"").is_none());
    }
}
