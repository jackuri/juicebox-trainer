//! INI-style text saves: `[section]` headers and `key = value` lines. Only
//! files where every line is one of those (or a comment) count as INI. Edits
//! replace just the value text.

use super::json::Spot;
use super::{Field, FieldKind, MAX_FIELDS};

fn valid_key(k: &str) -> bool {
    !k.is_empty()
        && k.len() <= 128
        && k.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '))
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
    for raw in text.split_inclusive('\n') {
        let line_start = offset;
        offset += raw.len();
        let line = raw.trim_end_matches(['\n', '\r']);
        let t = line.trim();
        if t.is_empty() || t.starts_with(';') || t.starts_with('#') {
            continue;
        }
        if let Some(name) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if !valid_key(name.trim()) {
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
        if out.len() >= MAX_FIELDS {
            truncated = true;
            continue;
        }
        let path = if section.is_empty() {
            key.to_string()
        } else {
            format!("{section}.{key}")
        };
        out.push(Spot {
            field: Field {
                path,
                kind: kind_of(value),
                value: value.to_string(),
                editable: true,
                note: None,
            },
            start,
            end: start + value.len(),
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
    let v = value.trim();
    if v.contains(['\n', '\r']) {
        return Err("a value can't contain line breaks".into());
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
    Ok([&b[..spot.start], v.as_bytes(), &b[spot.end..]].concat())
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
    fn other_text_is_not_ini() {
        assert!(parse(b"<save gold=\"1\"/>").is_none());
        assert!(parse(b"just some words").is_none());
        assert!(parse(b"gold: 100\nhp: 5").is_none());
        assert!(parse(b"").is_none());
    }
}
