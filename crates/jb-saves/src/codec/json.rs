//! JSON saves. The file is scanned once to find every value and where its
//! text is; an edit replaces just that text, so the rest of the file (spacing,
//! key order, number formatting) stays exactly as it was.

use super::{Field, FieldKind, MAX_FIELDS};

const MAX_DEPTH: usize = 256;

#[derive(Debug, Clone)]
pub(crate) struct Spot {
    pub field: Field,
    pub start: usize,
    pub end: usize,
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
    out: Vec<Spot>,
    truncated: bool,
}

/// `player.gold`, `items[2]`, `["odd key"]`.
pub(crate) fn join_key(path: &str, key: &str) -> String {
    let simple = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == ' ' || c == '$');
    let part = if simple {
        key.to_string()
    } else {
        format!("[{}]", serde_json::to_string(key).unwrap_or_default())
    };
    if path.is_empty() || part.starts_with('[') {
        format!("{path}{part}")
    } else {
        format!("{path}.{part}")
    }
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn err(&self, what: &str) -> String {
        format!("{what} at byte {}", self.i)
    }

    fn eat(&mut self, c: u8) -> Result<(), String> {
        if self.b.get(self.i) == Some(&c) {
            self.i += 1;
            Ok(())
        } else {
            Err(self.err(&format!("expected '{}'", c as char)))
        }
    }

    fn push(&mut self, path: &str, kind: FieldKind, start: usize, value: String) {
        if self.out.len() >= MAX_FIELDS {
            self.truncated = true;
            return;
        }
        self.out.push(Spot {
            field: Field {
                path: path.to_string(),
                kind,
                value,
                editable: true,
                note: None,
            },
            start,
            end: self.i,
        });
    }

    fn string(&mut self) -> Result<String, String> {
        self.eat(b'"')?;
        let start = self.i;
        loop {
            match self.b.get(self.i) {
                None => return Err(self.err("unterminated string")),
                Some(b'"') => break,
                Some(b'\\') => self.i += 2,
                Some(c) if *c < 0x20 => return Err(self.err("control character in string")),
                Some(_) => self.i += 1,
            }
        }
        let raw = &self.b[start - 1..=self.i];
        self.i += 1;
        let raw = std::str::from_utf8(raw).map_err(|_| self.err("invalid UTF-8"))?;
        serde_json::from_str::<String>(raw).map_err(|e| self.err(&format!("bad string ({e})")))
    }

    fn number(&mut self) -> Result<FieldKind, String> {
        let start = self.i;
        let mut float = false;
        while let Some(&c) = self.b.get(self.i) {
            match c {
                b'0'..=b'9' | b'-' | b'+' => {}
                b'.' | b'e' | b'E' => float = true,
                _ => break,
            }
            self.i += 1;
        }
        let text = std::str::from_utf8(&self.b[start..self.i]).unwrap_or("");
        serde_json::from_str::<serde_json::Number>(text).map_err(|_| self.err("bad number"))?;
        Ok(if float {
            FieldKind::Float
        } else {
            FieldKind::Int
        })
    }

    fn literal(&mut self, word: &[u8]) -> Result<(), String> {
        if self.b[self.i..].starts_with(word) {
            self.i += word.len();
            Ok(())
        } else {
            Err(self.err("unexpected character"))
        }
    }

    fn value(&mut self, path: &str, depth: usize) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err(self.err("nested too deeply"));
        }
        self.ws();
        let start = self.i;
        match self.b.get(self.i) {
            Some(b'{') => {
                self.i += 1;
                self.ws();
                if self.b.get(self.i) == Some(&b'}') {
                    self.i += 1;
                    return Ok(());
                }
                loop {
                    self.ws();
                    let key = self.string()?;
                    self.ws();
                    self.eat(b':')?;
                    self.value(&join_key(path, &key), depth + 1)?;
                    self.ws();
                    match self.b.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            return Ok(());
                        }
                        _ => return Err(self.err("expected ',' or '}'")),
                    }
                }
            }
            Some(b'[') => {
                self.i += 1;
                self.ws();
                if self.b.get(self.i) == Some(&b']') {
                    self.i += 1;
                    return Ok(());
                }
                let mut n = 0;
                loop {
                    self.value(&format!("{path}[{n}]"), depth + 1)?;
                    n += 1;
                    self.ws();
                    match self.b.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            return Ok(());
                        }
                        _ => return Err(self.err("expected ',' or ']'")),
                    }
                }
            }
            Some(b'"') => {
                let s = self.string()?;
                self.push(path, FieldKind::Text, start, s);
                Ok(())
            }
            Some(b't') => {
                self.literal(b"true")?;
                self.push(path, FieldKind::Bool, start, "true".into());
                Ok(())
            }
            Some(b'f') => {
                self.literal(b"false")?;
                self.push(path, FieldKind::Bool, start, "false".into());
                Ok(())
            }
            Some(b'n') => {
                self.literal(b"null")?;
                self.push(path, FieldKind::Null, start, "null".into());
                Ok(())
            }
            Some(b'-' | b'0'..=b'9') => {
                let kind = self.number()?;
                let text = String::from_utf8_lossy(&self.b[start..self.i]).into_owned();
                self.push(path, kind, start, text);
                Ok(())
            }
            _ => Err(self.err("unexpected character")),
        }
    }
}

/// Skips a UTF-8 byte order mark.
fn bom(b: &[u8]) -> usize {
    if b.starts_with(b"\xEF\xBB\xBF") { 3 } else { 0 }
}

/// Every value in the document, or why it isn't JSON.
pub(crate) fn parse(b: &[u8]) -> Result<(Vec<Spot>, bool), String> {
    let mut p = Parser {
        b,
        i: bom(b),
        out: Vec::new(),
        truncated: false,
    };
    p.ws();
    if !matches!(p.b.get(p.i), Some(b'{' | b'[')) {
        return Err("not a JSON object or array".into());
    }
    p.value("", 0)?;
    p.ws();
    if p.i != b.len() {
        return Err(p.err("extra data after the JSON"));
    }
    Ok((p.out, p.truncated))
}

/// The JSON text for `value` in a field of `kind`.
pub(crate) fn token(kind: FieldKind, value: &str) -> Result<String, String> {
    let v = value.trim();
    match kind {
        FieldKind::Text => serde_json::to_string(value).map_err(|e| e.to_string()),
        FieldKind::Int => v
            .parse::<i64>()
            .map(|n| n.to_string())
            .or_else(|_| v.parse::<u64>().map(|n| n.to_string()))
            .map_err(|_| format!("{value:?} isn't a whole number")),
        FieldKind::Float => match serde_json::from_str::<serde_json::Number>(v) {
            Ok(_) => Ok(v.to_string()),
            Err(_) => Err(format!("{value:?} isn't a number")),
        },
        FieldKind::Bool => match v.to_ascii_lowercase().as_str() {
            "true" => Ok("true".into()),
            "false" => Ok("false".into()),
            _ => Err(format!("{value:?} isn't true or false")),
        },
        FieldKind::Null => match serde_json::from_str::<serde_json::Value>(v) {
            Ok(j) if !j.is_object() && !j.is_array() => Ok(v.to_string()),
            _ => serde_json::to_string(value).map_err(|e| e.to_string()),
        },
        FieldKind::Other => Err("this value can't be edited".into()),
    }
}

/// Sets the value at `path`.
pub(crate) fn set(b: &[u8], path: &str, value: &str) -> Result<Vec<u8>, String> {
    let (spots, _) = parse(b)?;
    let spot = spots
        .iter()
        .find(|s| s.field.path == path)
        .ok_or_else(|| format!("no value at {path}"))?;
    let text = token(spot.field.kind, value)?;
    Ok([&b[..spot.start], text.as_bytes(), &b[spot.end..]].concat())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "\u{feff}{\n  \"version\": 1,\n  \"player\": { \"name\": \"Tester\", \"gold\": 250, \"speed\": 1.5,\n    \"alive\": true, \"pet\": null, \"bag\": [3, {\"id\": \"potion\"}], \"odd.key\": \"x\\\"y\" }\n}\n";

    fn fields() -> Vec<(String, FieldKind, String)> {
        parse(DOC.as_bytes())
            .unwrap()
            .0
            .into_iter()
            .map(|s| (s.field.path, s.field.kind, s.field.value))
            .collect()
    }

    #[test]
    fn finds_every_value_with_paths() {
        let f = fields();
        let get = |p: &str| f.iter().find(|x| x.0 == p).cloned().unwrap();
        assert_eq!(get("version").1, FieldKind::Int);
        assert_eq!(get("player.name").2, "Tester");
        assert_eq!(get("player.gold").2, "250");
        assert_eq!(get("player.speed").1, FieldKind::Float);
        assert_eq!(get("player.alive").1, FieldKind::Bool);
        assert_eq!(get("player.pet").1, FieldKind::Null);
        assert_eq!(get("player.bag[0]").2, "3");
        assert_eq!(get("player.bag[1].id").2, "potion");
        assert_eq!(get("player[\"odd.key\"]").2, "x\"y");
        assert_eq!(f.len(), 9);
    }

    #[test]
    fn edits_change_only_that_value() {
        let out = set(DOC.as_bytes(), "player.gold", " 999999 ").unwrap();
        let expected = DOC.replace("\"gold\": 250", "\"gold\": 999999");
        assert_eq!(String::from_utf8(out).unwrap(), expected);
        let out = set(DOC.as_bytes(), "player.name", "Max \"the\" Hero").unwrap();
        assert!(
            String::from_utf8(out)
                .unwrap()
                .contains(r#""name": "Max \"the\" Hero""#)
        );
        let out = set(DOC.as_bytes(), "player.alive", "FALSE").unwrap();
        assert!(String::from_utf8(out).unwrap().contains("\"alive\": false"));
        let out = set(DOC.as_bytes(), "player.pet", "\"Rex\"").unwrap();
        assert!(String::from_utf8(out).unwrap().contains("\"pet\": \"Rex\""));
    }

    #[test]
    fn bad_edits_and_bad_json_are_refused() {
        assert!(set(DOC.as_bytes(), "player.gold", "1.5").is_err());
        assert!(set(DOC.as_bytes(), "player.gold", "lots").is_err());
        assert!(set(DOC.as_bytes(), "player.speed", "fast").is_err());
        assert!(set(DOC.as_bytes(), "nope", "1").is_err());
        assert!(parse(b"{\"a\": 1} x").is_err());
        assert!(parse(b"{\"a\": }").is_err());
        assert!(parse(b"42").is_err());
        assert!(parse(b"{\"a\": \"unterminated}").is_err());
        let deep = "[".repeat(300) + &"]".repeat(300);
        assert!(parse(deep.as_bytes()).is_err());
    }
}
