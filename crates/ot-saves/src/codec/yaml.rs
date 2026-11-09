//! YAML saves, block style: `key: value` maps, `- item` lists, comments,
//! quoted text and several documents (`---`, as in Unity's YAML files). Flow
//! collections (`{a: 1}`), multi-line text and aliases are shown but not
//! edited. Files with anything else (tabs for indentation, plain text running
//! over several lines) aren't treated as YAML and open as hex.
//!
//! An edit replaces just the value's text, so comments, spacing and key order
//! stay exactly as they were. Nothing is ever loaded as objects (no tags
//! interpreted).

use super::json::{Spot, join_key};
use super::{Field, FieldKind, MAX_FIELDS};

#[derive(Debug)]
struct Frame {
    /// Column of this frame's lines; `None` until its first line is seen.
    indent: Option<usize>,
    /// Column of the key (or `-`) that opened it.
    parent: Option<usize>,
    path: String,
    seq: Option<bool>,
    next: usize,
}

impl Frame {
    fn new(path: String, parent: Option<usize>) -> Frame {
        Frame {
            indent: None,
            parent,
            path,
            seq: None,
            next: 0,
        }
    }
}

struct Parser<'a> {
    text: &'a str,
    out: Vec<Spot>,
    truncated: bool,
}

fn is_seq_item(c: &str) -> bool {
    c == "-" || c.starts_with("- ")
}

fn plain_kind(v: &str) -> FieldKind {
    if v.parse::<i64>().is_ok() {
        FieldKind::Int
    } else if matches!(v, "true" | "false" | "True" | "False" | "TRUE" | "FALSE") {
        FieldKind::Bool
    } else if matches!(v, "~" | "null" | "Null" | "NULL") {
        FieldKind::Null
    } else if (v.contains('.') || v.contains('e') || v.contains('E'))
        && v.parse::<f64>().is_ok_and(f64::is_finite)
    {
        FieldKind::Float
    } else {
        FieldKind::Text
    }
}

/// The key of a `key: rest` line and where `rest` starts, or `None`.
fn split_key(c: &str) -> Option<(String, usize)> {
    let (key, after) = if let Some(q) = c.strip_prefix('"') {
        let end = closing_double(q)?;
        let key: String = serde_json::from_str(&c[..end + 2]).ok()?;
        (key, end + 2)
    } else if let Some(q) = c.strip_prefix('\'') {
        let end = closing_single(q)?;
        (q[..end].replace("''", "'"), end + 2)
    } else {
        if c.starts_with(['[', '{', '?', '#', '&', '*', '!', '|', '>', '%', '@', '`']) {
            return None;
        }
        let bytes = c.as_bytes();
        let colon = (0..bytes.len())
            .find(|&i| bytes[i] == b':' && (i + 1 == bytes.len() || bytes[i + 1] == b' '))?;
        let key = c[..colon].trim_end();
        if key.is_empty() || key.contains(" #") {
            return None;
        }
        (key.to_string(), colon)
    };
    let rest = &c[after..];
    let rest = rest.strip_prefix(':')?;
    if !(rest.is_empty() || rest.starts_with(' ')) {
        return None;
    }
    Some((key, after + 1))
}

/// Index of the closing `"` in `s` (which starts after the opening one).
fn closing_double(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

fn closing_single(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\'' {
            if b.get(i + 1) == Some(&b'\'') {
                i += 2;
                continue;
            }
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Strips a trailing ` # comment` from plain text.
fn without_comment(v: &str) -> &str {
    match v.find(" #") {
        Some(i) => v[..i].trim_end(),
        None => v.trim_end(),
    }
}

enum Value {
    /// A value on following lines.
    Nested,
    /// Multi-line text (`|` or `>`): its lines are skipped.
    Block,
    Done,
}

impl Parser<'_> {
    fn push(&mut self, field: Field, start: usize, end: usize) {
        if self.out.len() >= MAX_FIELDS {
            self.truncated = true;
            return;
        }
        self.out.push(Spot { field, start, end });
    }

    fn read_only(&mut self, path: &str, value: &str, note: &str) {
        self.push(
            Field {
                path: path.to_string(),
                kind: FieldKind::Other,
                value: value.to_string(),
                editable: false,
                note: Some(note.into()),
            },
            0,
            0,
        );
    }

    /// The value text `v` starting at byte `at` of the file.
    fn value(&mut self, path: &str, v: &str, at: usize) -> Option<Value> {
        let mut v = v;
        let mut at = at;
        // Anchors and tags in front of a value: `&a 5`, `!!int 5`.
        while v.starts_with('&') || v.starts_with('!') {
            let end = v.find(' ').unwrap_or(v.len());
            let rest = &v[end..];
            let trimmed = rest.trim_start();
            at += end + (rest.len() - trimmed.len());
            v = trimmed;
        }
        let v_end = v.trim_end();
        if v_end.is_empty() || v_end.starts_with('#') {
            return Some(Value::Nested);
        }
        match v.as_bytes()[0] {
            b'|' | b'>' => {
                self.read_only(path, "(text on several lines)", "multi-line text");
                Some(Value::Block)
            }
            b'*' => {
                self.read_only(path, without_comment(v), "alias");
                Some(Value::Done)
            }
            b'[' | b'{' => {
                let t = without_comment(v);
                if !(t.ends_with(']') || t.ends_with('}')) {
                    return None;
                }
                self.read_only(path, t, "list or map on one line");
                Some(Value::Done)
            }
            b'"' => {
                let end = closing_double(&v[1..])? + 2;
                let text: String = serde_json::from_str(&v[..end]).ok().or_else(|| {
                    // YAML escapes JSON doesn't know; show it as written.
                    Some(v[1..end - 1].to_string())
                })?;
                let tail = v[end..].trim();
                if !(tail.is_empty() || tail.starts_with('#')) {
                    return None;
                }
                self.push(
                    Field {
                        path: path.to_string(),
                        kind: FieldKind::Text,
                        value: text,
                        editable: true,
                        note: Some("quoted".into()),
                    },
                    at,
                    at + end,
                );
                Some(Value::Done)
            }
            b'\'' => {
                let end = closing_single(&v[1..])? + 2;
                let tail = v[end..].trim();
                if !(tail.is_empty() || tail.starts_with('#')) {
                    return None;
                }
                self.push(
                    Field {
                        path: path.to_string(),
                        kind: FieldKind::Text,
                        value: v[1..end - 1].replace("''", "'"),
                        editable: true,
                        note: Some("quoted".into()),
                    },
                    at,
                    at + end,
                );
                Some(Value::Done)
            }
            _ => {
                let t = without_comment(v);
                self.push(
                    Field {
                        path: path.to_string(),
                        kind: plain_kind(t),
                        value: t.to_string(),
                        editable: true,
                        note: None,
                    },
                    at,
                    at + t.len(),
                );
                Some(Value::Done)
            }
        }
    }

    fn run(&mut self) -> Option<()> {
        let text = self.text;
        let mut stack = vec![Frame::new(String::new(), None)];
        let mut doc = 0usize;
        let mut seen = false;
        // While skipping a `|` / `>` block: the column of its key.
        let mut block: Option<usize> = None;
        let mut offset = 0;
        for raw in text.split_inclusive('\n') {
            let line_start = offset;
            offset += raw.len();
            let line = raw.trim_end_matches(['\n', '\r']);
            let content = line.trim_start_matches(' ');
            let indent = line.len() - content.len();
            if let Some(col) = block {
                if content.trim().is_empty() || indent > col {
                    continue;
                }
                block = None;
            }
            let trimmed = content.trim_end();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if content.starts_with('\t') {
                return None;
            }
            if indent == 0 && (trimmed.starts_with('%') || trimmed == "...") {
                continue;
            }
            if indent == 0 && (trimmed == "---" || trimmed.starts_with("--- ")) {
                if seen {
                    doc += 1;
                }
                let prefix = if doc == 0 {
                    String::new()
                } else {
                    format!("doc{doc}")
                };
                stack = vec![Frame::new(prefix, None)];
                // `--- value` is a document that is just one value.
                let rest = trimmed[3..].trim_start();
                if !rest.is_empty() && !rest.starts_with('!') && !rest.starts_with('&') {
                    return None;
                }
                continue;
            }
            seen = true;
            // Find the frame this line belongs to.
            loop {
                let top = stack.last_mut()?;
                match top.indent {
                    None => {
                        let child = match top.parent {
                            None => true,
                            Some(p) => indent > p || (indent == p && is_seq_item(content)),
                        };
                        if child {
                            top.indent = Some(indent);
                            break;
                        }
                        // `key:` with nothing under it.
                        stack.pop();
                        if stack.is_empty() {
                            return None;
                        }
                    }
                    Some(i) if indent < i => {
                        stack.pop();
                        if stack.is_empty() {
                            return None;
                        }
                    }
                    // A list at its key's column ends at the next key there.
                    Some(i)
                        if indent == i
                            && top.seq == Some(true)
                            && top.parent == Some(indent)
                            && !is_seq_item(content) =>
                    {
                        stack.pop();
                        if stack.is_empty() {
                            return None;
                        }
                    }
                    Some(i) if indent == i => break,
                    // More indented than expected: text over several lines.
                    Some(_) => return None,
                }
            }
            let mut col = indent;
            let mut c = content;
            let mut at = line_start + indent;
            // `- item`, possibly `- key: value`.
            if is_seq_item(c) {
                let top = stack.last_mut()?;
                if top.seq == Some(false) {
                    return None;
                }
                top.seq = Some(true);
                let path = format!("{}[{}]", top.path, top.next);
                top.next += 1;
                let rest = &c[1..];
                let after = rest.trim_start_matches(' ');
                let skip = 1 + (rest.len() - after.len());
                let after_t = after.trim_end();
                if after_t.is_empty() || after_t.starts_with('#') {
                    stack.push(Frame::new(path, Some(col)));
                    continue;
                }
                if is_seq_item(after) {
                    return None;
                }
                if split_key(after).is_none() {
                    match self.value(&path, after, at + skip)? {
                        Value::Nested => stack.push(Frame::new(path, Some(col))),
                        Value::Block => block = Some(col),
                        Value::Done => {}
                    }
                    continue;
                }
                // A map inside the list item, its first key on this line.
                let mut frame = Frame::new(path, Some(col));
                frame.indent = Some(col + skip);
                stack.push(frame);
                col += skip;
                c = after;
                at += skip;
            }
            let (key, rest_at) = split_key(c)?;
            let top = stack.last_mut()?;
            if top.seq == Some(true) {
                return None;
            }
            top.seq = Some(false);
            let path = join_key(&top.path, &key);
            let rest = &c[rest_at..];
            let v = rest.trim_start_matches(' ');
            let v_at = at + rest_at + (rest.len() - v.len());
            match self.value(&path, v, v_at)? {
                Value::Nested => stack.push(Frame::new(path, Some(col))),
                Value::Block => block = Some(col),
                Value::Done => {}
            }
        }
        Some(())
    }
}

/// Every value, or `None` if this isn't YAML we can read.
pub(crate) fn parse(b: &[u8]) -> Option<(Vec<Spot>, bool)> {
    let skip = if b.starts_with(b"\xEF\xBB\xBF") { 3 } else { 0 };
    let text = std::str::from_utf8(&b[skip..]).ok()?;
    if text.contains('\0') {
        return None;
    }
    let mut p = Parser {
        text,
        out: Vec::new(),
        truncated: false,
    };
    p.run()?;
    if !p.out.iter().any(|s| s.field.editable) {
        return None;
    }
    for s in &mut p.out {
        if s.field.editable {
            s.start += skip;
            s.end += skip;
        }
    }
    Some((p.out, p.truncated))
}

/// Can `v` be written without quotes and still read back as text?
fn plain_safe(v: &str) -> bool {
    !v.is_empty()
        && v.trim() == v
        && !v.starts_with([
            '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%',
            '@', '`',
        ])
        && !v.contains(": ")
        && !v.contains(" #")
        && !v.ends_with(':')
        && !v.contains(['\n', '\r', '\t'])
        && plain_kind(v) == FieldKind::Text
}

pub(crate) fn set(b: &[u8], path: &str, value: &str) -> Result<Vec<u8>, String> {
    let (spots, _) = parse(b).ok_or("not a YAML file")?;
    let spot = spots
        .iter()
        .find(|s| s.field.path == path)
        .ok_or_else(|| format!("no value at {path}"))?;
    if !spot.field.editable {
        return Err("this value can't be edited".into());
    }
    let raw = &b[spot.start..spot.end];
    let t = value.trim();
    let text = match spot.field.kind {
        FieldKind::Int => {
            t.parse::<i64>()
                .map_err(|_| format!("{value:?} isn't a whole number"))?;
            t.to_string()
        }
        FieldKind::Float => {
            if !t.parse::<f64>().is_ok_and(f64::is_finite) {
                return Err(format!("{value:?} isn't a number"));
            }
            t.to_string()
        }
        FieldKind::Bool => match t.to_ascii_lowercase().as_str() {
            "true" => "true".into(),
            "false" => "false".into(),
            _ => return Err(format!("{value:?} isn't true or false")),
        },
        FieldKind::Null if plain_kind(t) != FieldKind::Text => t.to_string(),
        _ if raw.starts_with(b"'") => {
            if value.contains(['\n', '\r']) {
                return Err("a value can't contain line breaks".into());
            }
            format!("'{}'", value.replace('\'', "''"))
        }
        _ if !raw.starts_with(b"\"") && plain_safe(value) => value.to_string(),
        _ => serde_json::to_string(value).map_err(|e| e.to_string())?,
    };
    let out = [&b[..spot.start], text.as_bytes(), &b[spot.end..]].concat();
    let ok = parse(&out).is_some_and(|(s, _)| s.iter().any(|s| s.field.path == path));
    if !ok {
        return Err("that value would break the file".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# save file\nversion: 3\nplayer:\n  name: Tester   # who\n  gold: 250\n  speed: 1.5\n  alive: true\n  pet: ~\n  title: \"The \\\"Brave\\\"\"\n  motto: 'it''s fine'\n  bag:\n  - potion\n  - id: sword\n    level: 2\n  -\n    id: shield\n  stats: {str: 5, dex: 3}\n  bio: |\n    line one\n    line two\n  notes:\nworld:\n  day: 12\n";

    fn get(doc: &str, path: &str) -> Field {
        parse(doc.as_bytes())
            .unwrap()
            .0
            .into_iter()
            .find(|s| s.field.path == path)
            .unwrap_or_else(|| panic!("{path}"))
            .field
    }

    #[test]
    fn reads_block_yaml() {
        assert_eq!(get(DOC, "version").kind, FieldKind::Int);
        assert_eq!(get(DOC, "player.name").value, "Tester");
        assert_eq!(get(DOC, "player.gold").value, "250");
        assert_eq!(get(DOC, "player.speed").kind, FieldKind::Float);
        assert_eq!(get(DOC, "player.alive").kind, FieldKind::Bool);
        assert_eq!(get(DOC, "player.pet").kind, FieldKind::Null);
        assert_eq!(get(DOC, "player.title").value, "The \"Brave\"");
        assert_eq!(get(DOC, "player.motto").value, "it's fine");
        assert_eq!(get(DOC, "player.bag[0]").value, "potion");
        assert_eq!(get(DOC, "player.bag[1].id").value, "sword");
        assert_eq!(get(DOC, "player.bag[1].level").value, "2");
        assert_eq!(get(DOC, "player.bag[2].id").value, "shield");
        assert!(!get(DOC, "player.stats").editable);
        assert!(!get(DOC, "player.bio").editable);
        assert_eq!(get(DOC, "world.day").value, "12");
    }

    #[test]
    fn edits_change_only_that_value() {
        let out = set(DOC.as_bytes(), "player.gold", "99999").unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            DOC.replace("gold: 250", "gold: 99999")
        );
        let out = set(DOC.as_bytes(), "player.name", "Max").unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            DOC.replace("name: Tester", "name: Max")
        );
        // Text that needs quotes gets them.
        let out = set(DOC.as_bytes(), "player.name", "a: b # c").unwrap();
        assert_eq!(
            get(std::str::from_utf8(&out).unwrap(), "player.name").value,
            "a: b # c"
        );
        let out = set(DOC.as_bytes(), "player.name", "123").unwrap();
        let f = get(std::str::from_utf8(&out).unwrap(), "player.name");
        assert_eq!((f.kind, f.value.as_str()), (FieldKind::Text, "123"));
        let out = set(DOC.as_bytes(), "player.motto", "don't").unwrap();
        assert!(String::from_utf8(out).unwrap().contains("motto: 'don''t'"));
        let out = set(DOC.as_bytes(), "player.title", "Hero").unwrap();
        assert!(String::from_utf8(out).unwrap().contains("title: \"Hero\""));
        let out = set(DOC.as_bytes(), "player.bag[1].level", "9").unwrap();
        assert!(String::from_utf8(out).unwrap().contains("    level: 9\n"));
        assert!(set(DOC.as_bytes(), "player.gold", "lots").is_err());
        assert!(set(DOC.as_bytes(), "player.stats", "1").is_err());
        assert!(set(DOC.as_bytes(), "nope", "1").is_err());
    }

    #[test]
    fn reads_unity_style_documents() {
        let unity = "%YAML 1.1\n%TAG !u! tag:unity3d.com,2011:\n--- !u!114 &1\nMonoBehaviour:\n  m_Name: Save\n  gold: 10\n--- !u!114 &2\nMonoBehaviour:\n  m_Name: Other\n  gold: 20\n";
        assert_eq!(get(unity, "MonoBehaviour.gold").value, "10");
        assert_eq!(get(unity, "doc1.MonoBehaviour.gold").value, "20");
    }

    #[test]
    fn other_text_is_not_yaml() {
        assert!(parse(b"just some words").is_none());
        assert!(parse(b"{\"a\": 1}").is_none());
        assert!(parse(b"a: 1\n\tb: 2\n").is_none());
        assert!(parse(b"a: one\n  two\n").is_none());
        assert!(parse(b"<a/>").is_none());
        assert!(parse(b"").is_none());
        assert!(parse(b"[Section]\nkey=value\n").is_none());
    }
}
