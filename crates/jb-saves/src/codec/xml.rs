//! XML saves (e.g. .NET `XmlSerializer` saves of Unity and XNA games). Every
//! element that holds only text, and every attribute, is a value; its path is
//! made of element names (`Save.Player.Gold`, `Save.Items.Item[2].Count`,
//! `Save.Player@level`). An edit replaces just that text, so the rest of the
//! file stays exactly as it was.
//!
//! The file is only read as XML text: no DTDs (refused), no external
//! entities, nothing loaded or run.

use super::json::Spot;
use super::{Field, FieldKind, MAX_FIELDS};

const MAX_NODES: u32 = 2_000_000;

fn kind_of(v: &str) -> FieldKind {
    let t = v.trim();
    if t.parse::<i64>().is_ok() {
        FieldKind::Int
    } else if t == "true" || t == "false" {
        FieldKind::Bool
    } else if (t.contains('.') || t.contains('e') || t.contains('E'))
        && t.parse::<f64>().is_ok_and(f64::is_finite)
    {
        FieldKind::Float
    } else {
        FieldKind::Text
    }
}

fn bom(b: &[u8]) -> usize {
    if b.starts_with(b"\xEF\xBB\xBF") { 3 } else { 0 }
}

/// Every value, or `None` if this isn't an XML document we can read.
pub(crate) fn parse(b: &[u8]) -> Option<(Vec<Spot>, bool)> {
    let skip = bom(b);
    let text = std::str::from_utf8(&b[skip..]).ok()?;
    if !text.trim_start().starts_with('<') {
        return None;
    }
    let opts = roxmltree::ParsingOptions {
        allow_dtd: false,
        nodes_limit: MAX_NODES,
        ..roxmltree::ParsingOptions::default()
    };
    let doc = roxmltree::Document::parse_with_options(text, opts).ok()?;
    let mut out = Vec::new();
    let mut truncated = false;
    let mut push = |field: Field, start: usize, end: usize| {
        if out.len() >= MAX_FIELDS {
            truncated = true;
            return;
        }
        out.push(Spot {
            field,
            start: start + skip,
            end: end + skip,
        });
    };
    let root = doc.root_element();
    let mut stack = vec![(root, root.tag_name().name().to_string())];
    while let Some((node, path)) = stack.pop() {
        for a in node.attributes() {
            let r = a.range_value();
            push(
                Field {
                    path: format!("{path}@{}", a.name()),
                    kind: kind_of(a.value()),
                    value: a.value().to_string(),
                    editable: true,
                    note: Some("attribute".into()),
                },
                r.start,
                r.end,
            );
        }
        let elements: Vec<_> = node.children().filter(|c| c.is_element()).collect();
        if elements.is_empty() {
            // A leaf with text: exactly one text child (plain or CDATA).
            let mut children = node.children().filter(|c| !c.is_comment() && !c.is_pi());
            if let (Some(t), None) = (children.next(), children.next())
                && t.is_text()
                && let Some(value) = t.text()
                && !value.trim().is_empty()
            {
                let r = t.range();
                let raw = &text[r.clone()];
                let cdata = raw.starts_with("<![CDATA[") && raw.ends_with("]]>");
                if cdata || !raw.contains('<') {
                    push(
                        Field {
                            path: path.clone(),
                            kind: kind_of(value),
                            value: value.to_string(),
                            editable: true,
                            note: None,
                        },
                        r.start,
                        r.end,
                    );
                }
            }
            continue;
        }
        let mut seen = std::collections::HashMap::<&str, usize>::new();
        let mut named = Vec::new();
        for e in &elements {
            let name = e.tag_name().name();
            let n = seen.entry(name).or_default();
            named.push((*e, name, *n));
            *n += 1;
        }
        // Reversed so the stack hands them out in file order.
        for (e, name, i) in named.into_iter().rev() {
            let child = if seen[name] > 1 {
                format!("{path}.{name}[{i}]")
            } else {
                format!("{path}.{name}")
            };
            stack.push((e, child));
        }
    }
    out.sort_by_key(|s| s.start);
    Some((out, truncated))
}

fn escape(v: &str, attribute: bool) -> String {
    let mut s = String::with_capacity(v.len());
    for c in v.chars() {
        match c {
            '&' => s.push_str("&amp;"),
            '<' => s.push_str("&lt;"),
            '>' => s.push_str("&gt;"),
            '"' if attribute => s.push_str("&quot;"),
            '\'' if attribute => s.push_str("&apos;"),
            _ => s.push(c),
        }
    }
    s
}

pub(crate) fn set(b: &[u8], path: &str, value: &str) -> Result<Vec<u8>, String> {
    let (spots, _) = parse(b).ok_or("not an XML file")?;
    let spot = spots
        .iter()
        .find(|s| s.field.path == path)
        .ok_or_else(|| format!("no value at {path}"))?;
    let v = match spot.field.kind {
        FieldKind::Int => {
            let t = value.trim();
            t.parse::<i64>()
                .map_err(|_| format!("{value:?} isn't a whole number"))?;
            t.to_string()
        }
        FieldKind::Float => {
            let t = value.trim();
            if !t.parse::<f64>().is_ok_and(f64::is_finite) {
                return Err(format!("{value:?} isn't a number"));
            }
            t.to_string()
        }
        FieldKind::Bool => match value.trim().to_ascii_lowercase().as_str() {
            "true" => "true".into(),
            "false" => "false".into(),
            _ => return Err(format!("{value:?} isn't true or false")),
        },
        _ => value.to_string(),
    };
    let attribute = path.contains('@');
    let text = escape(&v, attribute);
    let out = [&b[..spot.start], text.as_bytes(), &b[spot.end..]].concat();
    if parse(&out).is_none() {
        return Err("that value would break the file".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<SaveData xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\r\n  <!-- slot 1 -->\r\n  <Player level=\"7\">\r\n    <Name>Tom &amp; Jerry</Name>\r\n    <Gold>250</Gold>\r\n    <Speed>1.5</Speed>\r\n    <Alive>true</Alive>\r\n    <Note><![CDATA[a <b> c]]></Note>\r\n    <Empty />\r\n  </Player>\r\n  <Items>\r\n    <Item><Id>potion</Id><Count>3</Count></Item>\r\n    <Item><Id>ether</Id><Count>1</Count></Item>\r\n  </Items>\r\n</SaveData>\r\n";

    fn get(path: &str) -> Field {
        parse(DOC.as_bytes())
            .unwrap()
            .0
            .into_iter()
            .find(|s| s.field.path == path)
            .unwrap_or_else(|| panic!("{path}"))
            .field
    }

    #[test]
    fn finds_text_and_attributes() {
        assert_eq!(get("SaveData.Player.Name").value, "Tom & Jerry");
        assert_eq!(get("SaveData.Player.Gold").kind, FieldKind::Int);
        assert_eq!(get("SaveData.Player.Speed").kind, FieldKind::Float);
        assert_eq!(get("SaveData.Player.Alive").kind, FieldKind::Bool);
        assert_eq!(get("SaveData.Player.Note").value, "a <b> c");
        assert_eq!(get("SaveData.Player@level").value, "7");
        assert_eq!(get("SaveData.Items.Item[1].Id").value, "ether");
        assert_eq!(get("SaveData.Items.Item[0].Count").value, "3");
        let (spots, _) = parse(DOC.as_bytes()).unwrap();
        assert!(spots.iter().all(|s| !s.field.path.contains("Empty")));
        assert_eq!(spots.len(), 10);
    }

    #[test]
    fn edits_change_only_that_text() {
        let out = set(DOC.as_bytes(), "SaveData.Player.Gold", " 99999 ").unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            DOC.replace("<Gold>250</Gold>", "<Gold>99999</Gold>")
        );
        let out = set(DOC.as_bytes(), "SaveData.Player@level", "50").unwrap();
        assert!(String::from_utf8(out).unwrap().contains("level=\"50\""));
        let out = set(DOC.as_bytes(), "SaveData.Player.Name", "A<B> & \"C\"").unwrap();
        assert!(
            String::from_utf8(out)
                .unwrap()
                .contains("<Name>A&lt;B&gt; &amp; \"C\"</Name>")
        );
        let out = set(DOC.as_bytes(), "SaveData.Player.Note", "x & y").unwrap();
        assert!(
            String::from_utf8(out.clone())
                .unwrap()
                .contains("<Note>x &amp; y</Note>")
        );
        assert_eq!(
            parse(&out)
                .unwrap()
                .0
                .iter()
                .find(|s| s.field.path == "SaveData.Player.Note")
                .unwrap()
                .field
                .value,
            "x & y"
        );
        assert!(set(DOC.as_bytes(), "SaveData.Player.Gold", "lots").is_err());
        assert!(set(DOC.as_bytes(), "SaveData.Player.Alive", "maybe").is_err());
        assert!(set(DOC.as_bytes(), "SaveData.Nope", "1").is_err());
    }

    #[test]
    fn other_data_is_not_xml() {
        assert!(parse(b"{\"a\": 1}").is_none());
        assert!(parse(b"<a><b></a>").is_none());
        assert!(parse(b"plain text").is_none());
        // DTDs are refused.
        assert!(
            parse(b"<?xml version=\"1.0\"?><!DOCTYPE a [<!ENTITY x \"y\">]><a>&x;</a>").is_none()
        );
    }
}
