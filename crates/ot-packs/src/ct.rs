//! Cheat Engine tables (`.CT`) to packs.
//!
//! A table is XML. Only plain value entries come over: a number type, an
//! address inside a module (`game.exe+1234`, `[[game.exe+10]+20]+8`) and
//! optional pointer offsets. Everything else is listed as skipped with the
//! reason, never run: Auto Assembler and Lua scripts, fixed addresses, names
//! that only a table script defines, text and byte-array values. The pack
//! that comes out goes through the normal validator like any other pack.
//!
//! The XML is read with DTDs refused, from at most [`MAX_TABLE_BYTES`].

use std::collections::{BTreeMap, HashSet};

use ot_core::ValueType;
use serde::Serialize;

use crate::schema::{
    Address, Cheat, Game, Impl, Meta, NumberConfig, Pack, SCHEMA_VERSION, Ui, ValueImpl,
};
use crate::validate::{Severity, validate};

/// File extension, without the dot.
pub const EXTENSION: &str = "ct";
/// Tables refuse to open above this size.
pub const MAX_TABLE_BYTES: u64 = 16 << 20;
/// At most this many cheats come over from one table.
pub const MAX_CHEATS: usize = 500;
const MAX_ENTRIES: usize = 20_000;
const MAX_DEPTH: usize = 32;
/// XML nodes read from one table at most (a few hundred bytes of memory each).
const MAX_NODES: u32 = 300_000;
/// Stands in for the exe until the player names it (see [`Converted::exe_known`]).
pub const UNKNOWN_EXE: &str = "unknown.exe";

/// A table entry that didn't come over, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub name: String,
    pub why: String,
}

#[derive(Debug, Clone)]
pub struct Converted {
    pub pack: Pack,
    pub skipped: Vec<Skipped>,
    /// The game's exe is known (named by the player, or used by the table's
    /// addresses). If not, `pack.game.exe` is [`UNKNOWN_EXE`].
    pub exe_known: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum CtError {
    #[error("this isn't a Cheat Engine table ({0})")]
    NotTable(String),
    #[error("the file is {0} bytes, too big to be a cheat table")]
    TooBig(u64),
}

const SCRIPT: &str = "runs its own code (a script); OpenTrainer never runs code from a table";

/// Converts table bytes. `file_stem` names the game ("Hollow_Knight" →
/// "Hollow Knight"); `exe` is the game's exe file name if the player chose it.
/// `today` is `YYYY-MM-DD`.
pub fn convert(
    bytes: &[u8],
    file_stem: &str,
    exe: Option<&str>,
    today: &str,
) -> Result<Converted, CtError> {
    if bytes.len() as u64 > MAX_TABLE_BYTES {
        return Err(CtError::TooBig(bytes.len() as u64));
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| CtError::NotTable("it isn't text".into()))?;
    let doc = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_NODES,
            ..Default::default()
        },
    )
    .map_err(|e| CtError::NotTable(e.to_string()))?;
    let root = doc.root_element();
    if root.tag_name().name() != "CheatTable" {
        return Err(CtError::NotTable(format!(
            "it starts with <{}>",
            root.tag_name().name()
        )));
    }

    let mut walk = Walk::default();
    if let Some(entries) = child(root, "CheatEntries") {
        walk.entries(entries, None, None, 0);
    }
    if child(root, "LuaScript").is_some_and(|n| !text_of(n).trim().is_empty()) {
        walk.skipped.push(Skipped {
            name: "The table's own script (Lua)".into(),
            why: "OpenTrainer never runs code from a table".into(),
        });
    }

    let name = game_name(file_stem);
    let guessed = exe.map(str::to_string).or_else(|| walk.likely_exe());
    let exe_known = guessed.is_some();
    let mut pack = Pack {
        schema: SCHEMA_VERSION,
        game: Game {
            id: slug(&name, "imported-table"),
            name,
            engine: None,
            exe: guessed.unwrap_or_else(|| UNKNOWN_EXE.into()),
            arch: None,
            builds: Vec::new(),
        },
        meta: Meta {
            author: "Cheat Engine table (imported)".into(),
            created: Some(today.into()),
            pack_version: "1.0.0".into(),
            notes: Some(format!(
                "Converted from the Cheat Engine table \"{file_stem}\". Credit goes to the table's author (see where you \
                 downloaded it). Tables are made for one game version: if a value shows ?? the game was probably updated."
            )),
        },
        cheats: walk.cheats,
        save_codes: Vec::new(),
    };
    let mut skipped = walk.skipped;
    drop_invalid(&mut pack, &mut skipped);
    Ok(Converted {
        pack,
        skipped,
        exe_known,
    })
}

/// Removes cheats the validator rejects (listing them as skipped), so one
/// odd entry doesn't stop the rest.
fn drop_invalid(pack: &mut Pack, skipped: &mut Vec<Skipped>) {
    let mut bad: BTreeMap<usize, String> = BTreeMap::new();
    for issue in validate(pack) {
        if issue.severity != Severity::Error {
            continue;
        }
        let Some(i) = issue
            .path
            .strip_prefix("cheats[")
            .and_then(|r| r.split(']').next())
            .and_then(|n| n.parse::<usize>().ok())
        else {
            continue;
        };
        bad.entry(i).or_insert(issue.message);
    }
    for (i, why) in bad.into_iter().rev() {
        if i < pack.cheats.len() {
            let c = pack.cheats.remove(i);
            skipped.push(Skipped { name: c.name, why });
        }
    }
}

#[derive(Default)]
struct Walk {
    cheats: Vec<Cheat>,
    skipped: Vec<Skipped>,
    ids: HashSet<String>,
    seen: usize,
}

impl Walk {
    fn entries(
        &mut self,
        list: roxmltree::Node,
        category: Option<&str>,
        parent: Option<&Chain>,
        depth: usize,
    ) {
        for e in list.children().filter(|n| n.has_tag_name("CheatEntry")) {
            self.seen += 1;
            if self.seen > MAX_ENTRIES || depth > MAX_DEPTH {
                return;
            }
            self.entry(e, category, parent, depth);
        }
    }

    fn entry(
        &mut self,
        e: roxmltree::Node,
        category: Option<&str>,
        parent: Option<&Chain>,
        depth: usize,
    ) {
        let id = child(e, "ID").map(text_of).unwrap_or_default();
        let mut name = child(e, "Description")
            .map(|n| unquote(&text_of(n)))
            .unwrap_or_default();
        if name.is_empty() {
            name = format!("Entry {}", id.trim());
        }
        let ty = child(e, "VariableType").map(|n| text_of(n).trim().to_string());
        let address = child(e, "Address").map(|n| text_of(n).trim().to_string());
        let group = child(e, "GroupHeader").is_some_and(|n| text_of(n).trim() == "1")
            || (ty.is_none() && address.is_none());

        let mut chain = None;
        if group {
            // A group's own address (if any) is what its children's `+X` addresses add to.
            chain = address
                .as_deref()
                .filter(|a| !a.is_empty() && !a.starts_with(['+', '-']))
                .and_then(|a| parse_address(a).ok());
        } else {
            match self.value(e, ty.as_deref(), address.as_deref(), parent) {
                Ok((c, vt)) => {
                    self.add(&name, category, &c, vt);
                    chain = Some(c);
                }
                Err(why) => self.skipped.push(Skipped {
                    name: name.clone(),
                    why: why.to_string(),
                }),
            }
        }
        if let Some(kids) = child(e, "CheatEntries") {
            let cat = if group || category.is_none() {
                name.as_str()
            } else {
                category.unwrap_or_default()
            };
            self.entries(kids, Some(cat), chain.as_ref(), depth + 1);
        }
    }

    fn value(
        &self,
        e: roxmltree::Node,
        ty: Option<&str>,
        address: Option<&str>,
        parent: Option<&Chain>,
    ) -> Result<(Chain, ValueType), String> {
        let signed = child(e, "ShowAsSigned").is_some_and(|n| text_of(n).trim() == "1");
        let vt = match ty {
            Some("Auto Assembler Script") => return Err(SCRIPT.into()),
            Some(t) => value_type(t, signed)
                .ok_or_else(|| format!("its type ({t}) isn't a number OpenTrainer can set"))?,
            None => return Err("it has no value type".into()),
        };
        let address = address
            .filter(|a| !a.is_empty())
            .ok_or("it has no address")?;
        let mut chain = if let Some(rel) = address
            .strip_prefix('+')
            .or_else(|| address.strip_prefix('-').map(|_| address))
        {
            // Relative to the entry it sits under.
            let n = if address.starts_with('-') {
                parse_hex(address)
            } else {
                parse_hex(rel)
            }
            .ok_or("its address isn't a plain hex offset")?;
            let mut c = parent
                .cloned()
                .ok_or("its address is relative to an entry that couldn't be imported")?;
            c.add(n)?;
            c
        } else {
            parse_address(address)?
        };
        if let Some(offs) = child(e, "Offsets") {
            let list: Vec<String> = offs
                .children()
                .filter(|n| n.has_tag_name("Offset"))
                .map(text_of)
                .collect();
            // Cheat Engine stores them last-applied first.
            for o in list.iter().rev() {
                let n = parse_hex(o).ok_or_else(|| {
                    format!(
                        "its pointer offset \"{}\" isn't a plain hex number",
                        o.trim()
                    )
                })?;
                chain.offsets.push(n);
            }
        }
        Ok((chain, vt))
    }

    fn add(&mut self, name: &str, category: Option<&str>, chain: &Chain, ty: ValueType) {
        if self.cheats.len() >= MAX_CHEATS {
            self.skipped.push(Skipped {
                name: name.into(),
                why: format!("only the first {MAX_CHEATS} entries of a table are imported"),
            });
            return;
        }
        let base = slug(name, "value");
        let mut id = base.chars().take(56).collect::<String>();
        let mut n = 2;
        while self.ids.contains(&id) {
            id = format!("{}-{n}", base.chars().take(56).collect::<String>());
            n += 1;
        }
        self.ids.insert(id.clone());
        self.cheats.push(Cheat {
            id,
            name: name.chars().take(80).collect(),
            category: category.map(|c| c.chars().take(60).collect()),
            description: None,
            ui: Ui::Number,
            hotkey: None,
            toggle: None,
            slider: None,
            number: Some(NumberConfig {
                min: None,
                max: None,
                hold: true,
            }),
            button: None,
            implementation: Impl::Value(ValueImpl {
                ty,
                address: Address {
                    base: Some(chain.base()),
                    aob: None,
                    module: None,
                    offset: None,
                    operand: None,
                    next: None,
                    offsets: chain.offsets.iter().map(|&o| hex(o)).collect(),
                },
            }),
        });
    }

    /// The exe the table's addresses use most.
    fn likely_exe(&self) -> Option<String> {
        let mut count: BTreeMap<String, (usize, String)> = BTreeMap::new();
        for c in &self.cheats {
            if let Impl::Value(v) = &c.implementation
                && let Some(base) = &v.address.base
                && let Some((m, _)) = base.rsplit_once('+')
                && m.to_ascii_lowercase().ends_with(".exe")
            {
                count
                    .entry(m.to_ascii_lowercase())
                    .or_insert((0, m.to_string()))
                    .0 += 1;
            }
        }
        count.into_values().max_by_key(|(n, _)| *n).map(|(_, m)| m)
    }
}

/// A pointer path: `module+base`, then for each offset read a pointer and add it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Chain {
    module: String,
    base: i64,
    offsets: Vec<i64>,
}

impl Chain {
    fn add(&mut self, n: i64) -> Result<(), String> {
        let slot = match self.offsets.last_mut() {
            Some(o) => o,
            None => &mut self.base,
        };
        *slot = slot.checked_add(n).ok_or("its address is out of range")?;
        Ok(())
    }

    fn base(&self) -> String {
        format!("{}+{}", self.module, hex(self.base))
    }
}

fn hex(n: i64) -> String {
    if n < 0 {
        format!("-0x{:X}", n.unsigned_abs())
    } else {
        format!("0x{n:X}")
    }
}

fn value_type(t: &str, signed: bool) -> Option<ValueType> {
    Some(match (t, signed) {
        ("Byte", false) => ValueType::U8,
        ("Byte", true) => ValueType::I8,
        ("2 Bytes", false) => ValueType::U16,
        ("2 Bytes", true) => ValueType::I16,
        ("4 Bytes", false) => ValueType::U32,
        ("4 Bytes", true) => ValueType::I32,
        ("8 Bytes", false) => ValueType::U64,
        ("8 Bytes", true) => ValueType::I64,
        ("Float", _) => ValueType::F32,
        ("Double", _) => ValueType::F64,
        _ => return None,
    })
}

/// `0x1A`, `1A`, `-8` (hex, as Cheat Engine writes them).
fn parse_hex(s: &str) -> Option<i64> {
    let t = s.trim();
    if t.is_empty() || t.len() > 18 {
        return None;
    }
    crate::schema::parse_offset(t)
}

fn is_module(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    (l.ends_with(".exe") || l.ends_with(".dll")) && l.len() > 4 && !s.contains(['/', '\\', ':'])
}

/// Parses `"game.exe"+1234`, `game.exe+10+8`, `[[game.exe+10]+20]+8`.
fn parse_address(s: &str) -> Result<Chain, String> {
    let mut p = Parser {
        s: s.as_bytes(),
        i: 0,
        depth: 0,
    };
    let chain = p.expr()?;
    p.skip_ws();
    if p.i != p.s.len() {
        return Err(format!(
            "its address ({s}) isn't a module address OpenTrainer understands"
        ));
    }
    let module = chain.module.ok_or(
        "its address is a fixed number, which usually changes every time the game starts"
            .to_string(),
    )?;
    if chain.base < 0 {
        return Err("its address is before the start of the module".into());
    }
    Ok(Chain {
        module,
        base: chain.base,
        offsets: chain.offsets,
    })
}

struct Partial {
    module: Option<String>,
    base: i64,
    offsets: Vec<i64>,
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    depth: usize,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while self.s.get(self.i).is_some_and(u8::is_ascii_whitespace) {
            self.i += 1;
        }
    }

    fn expr(&mut self) -> Result<Partial, String> {
        let mut c = self.term()?;
        loop {
            self.skip_ws();
            let sign = match self.s.get(self.i) {
                Some(b'+') => 1,
                Some(b'-') => -1,
                _ => return Ok(c),
            };
            self.i += 1;
            self.skip_ws();
            let start = self.i;
            while self.s.get(self.i).is_some_and(u8::is_ascii_alphanumeric) {
                self.i += 1;
            }
            let word = std::str::from_utf8(&self.s[start..self.i]).unwrap_or_default();
            let n = parse_hex(word).ok_or_else(|| {
                format!(
                    "its address uses \"{word}\", a name only one of the table's scripts defines"
                )
            })?;
            let n = if sign < 0 { n.checked_neg() } else { Some(n) }
                .ok_or("its address is out of range")?;
            let slot = match c.offsets.last_mut() {
                Some(o) => o,
                None => &mut c.base,
            };
            *slot = slot.checked_add(n).ok_or("its address is out of range")?;
        }
    }

    fn term(&mut self) -> Result<Partial, String> {
        self.skip_ws();
        match self.s.get(self.i) {
            Some(b'[') => {
                self.depth += 1;
                if self.depth > 16 {
                    return Err("its address is nested too deeply".into());
                }
                self.i += 1;
                let mut inner = self.expr()?;
                self.skip_ws();
                if self.s.get(self.i) != Some(&b']') {
                    return Err("its address has an unclosed [".into());
                }
                self.i += 1;
                self.depth -= 1;
                if inner.module.is_none() {
                    return Err("its address starts from a fixed number, which usually changes every time the game starts".into());
                }
                inner.offsets.push(0);
                Ok(inner)
            }
            Some(b'"') => {
                self.i += 1;
                let start = self.i;
                while self.s.get(self.i).is_some_and(|&b| b != b'"') {
                    self.i += 1;
                }
                let word = std::str::from_utf8(&self.s[start..self.i])
                    .unwrap_or_default()
                    .to_string();
                if self.s.get(self.i) != Some(&b'"') {
                    return Err("its address has an unclosed quote".into());
                }
                self.i += 1;
                self.word(&word)
            }
            _ => {
                let start = self.i;
                while self
                    .s
                    .get(self.i)
                    .is_some_and(|&b| !matches!(b, b'+' | b'[' | b']'))
                {
                    self.i += 1;
                }
                let raw = std::str::from_utf8(&self.s[start..self.i])
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                // `Game-Win64-Shipping.exe` has dashes; `game.exe-10` subtracts.
                if !is_module(&raw)
                    && let Some((left, right)) = raw.rsplit_once('-')
                    && let Some(n) = parse_hex(right)
                {
                    let mut c = self.word(left.trim())?;
                    c.base = c.base.checked_sub(n).ok_or("its address is out of range")?;
                    return Ok(c);
                }
                self.word(&raw)
            }
        }
    }

    fn word(&self, w: &str) -> Result<Partial, String> {
        if is_module(w) {
            return Ok(Partial {
                module: Some(w.to_string()),
                base: 0,
                offsets: Vec::new(),
            });
        }
        if let Some(n) = parse_hex(w) {
            return Ok(Partial {
                module: None,
                base: n,
                offsets: Vec::new(),
            });
        }
        if w.is_empty() {
            return Err("its address is empty".into());
        }
        Err(format!(
            "its address uses \"{w}\", a name only one of the table's scripts defines"
        ))
    }
}

fn child<'a, 'i>(n: roxmltree::Node<'a, 'i>, tag: &str) -> Option<roxmltree::Node<'a, 'i>> {
    n.children().find(|c| c.has_tag_name(tag))
}

fn text_of(n: roxmltree::Node) -> String {
    n.text().unwrap_or_default().to_string()
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    t.strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .unwrap_or(t)
        .trim()
        .to_string()
}

/// "Hollow_Knight v1.5" → "Hollow Knight v1.5".
fn game_name(stem: &str) -> String {
    let n = stem.replace('_', " ");
    let n = n.split_whitespace().collect::<Vec<_>>().join(" ");
    if n.is_empty() {
        "Imported table".into()
    } else {
        n.chars().take(80).collect()
    }
}

/// Lowercase slug for ids: letters and digits, words joined by '-'.
fn slug(s: &str, fallback: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out: String = out.trim_end_matches('-').chars().take(60).collect();
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { fallback.into() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<CheatTable CheatEngineTableVersion="45">
  <CheatEntries>
    <CheatEntry>
      <ID>1</ID>
      <Description>"Player"</Description>
      <GroupHeader>1</GroupHeader>
      <CheatEntries>
        <CheatEntry>
          <ID>2</ID>
          <Description>"Health"</Description>
          <ShowAsSigned>1</ShowAsSigned>
          <VariableType>4 Bytes</VariableType>
          <Address>"Game-Win64-Shipping.exe"+01A2B3C0</Address>
          <Offsets>
            <Offset>18</Offset>
            <Offset>B8</Offset>
          </Offsets>
        </CheatEntry>
        <CheatEntry>
          <ID>3</ID>
          <Description>"Stamina"</Description>
          <VariableType>Float</VariableType>
          <Address>[[Game-Win64-Shipping.exe+10]+20]+8</Address>
          <CheatEntries>
            <CheatEntry>
              <ID>4</ID>
              <Description>"Max stamina"</Description>
              <VariableType>Float</VariableType>
              <Address>+4</Address>
            </CheatEntry>
          </CheatEntries>
        </CheatEntry>
      </CheatEntries>
    </CheatEntry>
    <CheatEntry>
      <ID>5</ID>
      <Description>"Infinite ammo"</Description>
      <VariableType>Auto Assembler Script</VariableType>
      <AssemblerScript>[ENABLE]
alloc(newmem,2048)
[DISABLE]
dealloc(newmem)</AssemblerScript>
    </CheatEntry>
    <CheatEntry>
      <ID>6</ID>
      <Description>"Gold"</Description>
      <VariableType>4 Bytes</VariableType>
      <Address>pGold</Address>
    </CheatEntry>
    <CheatEntry>
      <ID>7</ID>
      <Description>"Fixed"</Description>
      <VariableType>2 Bytes</VariableType>
      <Address>1A2B3C4D</Address>
    </CheatEntry>
    <CheatEntry>
      <ID>8</ID>
      <Description>"Name"</Description>
      <VariableType>String</VariableType>
      <Address>game.exe+10</Address>
    </CheatEntry>
    <CheatEntry>
      <ID>9</ID>
      <Description>"Health"</Description>
      <VariableType>Double</VariableType>
      <Address>UnityPlayer.dll+0x40</Address>
    </CheatEntry>
  </CheatEntries>
  <LuaScript>print("hi")</LuaScript>
</CheatTable>
"#;

    fn value(c: &Cheat) -> (&ValueImpl, &str) {
        match &c.implementation {
            Impl::Value(v) => (v, c.category.as_deref().unwrap_or("")),
            Impl::Patch(_) => panic!("patch"),
        }
    }

    #[test]
    fn converts_value_entries_and_lists_the_rest() {
        let got = convert(TABLE.as_bytes(), "My_Game  v1.2", None, "2026-11-08").unwrap();
        let p = &got.pack;
        assert_eq!(p.game.name, "My Game v1.2");
        assert_eq!(p.game.id, "my-game-v1-2");
        assert_eq!(p.game.exe, "Game-Win64-Shipping.exe");
        assert!(got.exe_known);
        let names: Vec<&str> = p.cheats.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Health", "Stamina", "Max stamina", "Health"]);

        let (health, cat) = value(&p.cheats[0]);
        assert_eq!(cat, "Player");
        assert_eq!(health.ty, ValueType::I32);
        assert_eq!(
            health.address.base.as_deref(),
            Some("Game-Win64-Shipping.exe+0x1A2B3C0")
        );
        // Cheat Engine lists offsets last-applied first.
        assert_eq!(health.address.offsets, ["0xB8", "0x18"]);

        let (stamina, _) = value(&p.cheats[1]);
        assert_eq!(stamina.ty, ValueType::F32);
        assert_eq!(
            stamina.address.base.as_deref(),
            Some("Game-Win64-Shipping.exe+0x10")
        );
        assert_eq!(stamina.address.offsets, ["0x20", "0x8"]);
        let (max, cat) = value(&p.cheats[2]);
        assert_eq!(cat, "Player");
        assert_eq!(max.address.offsets, ["0x20", "0xC"]);

        let (h2, cat) = value(&p.cheats[3]);
        assert_eq!(cat, "");
        assert_eq!(h2.ty, ValueType::F64);
        assert_eq!(h2.address.base.as_deref(), Some("UnityPlayer.dll+0x40"));
        assert_eq!(p.cheats[0].id, "health");
        assert_eq!(p.cheats[3].id, "health-2");
        assert!(
            p.cheats
                .iter()
                .all(|c| c.ui == Ui::Number && c.number.as_ref().is_some_and(|n| n.hold))
        );

        let skipped: Vec<(&str, &str)> = got
            .skipped
            .iter()
            .map(|s| (s.name.as_str(), s.why.as_str()))
            .collect();
        assert_eq!(skipped.len(), 5, "{skipped:?}");
        assert!(skipped[0].0 == "Infinite ammo" && skipped[0].1.contains("never runs code"));
        assert!(skipped[1].0 == "Gold" && skipped[1].1.contains("pGold"));
        assert!(skipped[2].0 == "Fixed" && skipped[2].1.contains("fixed number"));
        assert!(skipped[3].0 == "Name" && skipped[3].1.contains("String"));
        assert!(skipped[4].1.contains("never runs code"));

        // The pack is valid as is.
        let text = serde_json::to_string_pretty(p).unwrap();
        crate::parse(&text).unwrap();
    }

    #[test]
    fn the_player_names_the_exe_when_the_table_does_not() {
        let t = r#"<CheatTable><CheatEntries><CheatEntry><Description>"Coins"</Description>
            <VariableType>4 Bytes</VariableType><Address>"GameAssembly.dll"+0123</Address>
            <Offsets><Offset>5C</Offset><Offset>-8</Offset></Offsets></CheatEntry></CheatEntries></CheatTable>"#;
        let got = convert(t.as_bytes(), "Coins", None, "2026-11-08").unwrap();
        assert!(!got.exe_known);
        assert_eq!(got.pack.game.exe, UNKNOWN_EXE);
        let (v, _) = value(&got.pack.cheats[0]);
        assert_eq!(v.address.offsets, ["-0x8", "0x5C"]);
        let got = convert(t.as_bytes(), "Coins", Some("Coins.exe"), "2026-11-08").unwrap();
        assert!(got.exe_known);
        assert_eq!(got.pack.game.exe, "Coins.exe");
    }

    #[test]
    fn refuses_what_isnt_a_table() {
        assert!(matches!(
            convert(b"<html/>", "x", None, "2026-11-08"),
            Err(CtError::NotTable(_))
        ));
        assert!(matches!(
            convert(b"\xff\xfe", "x", None, "2026-11-08"),
            Err(CtError::NotTable(_))
        ));
        let dtd = r#"<?xml version="1.0"?><!DOCTYPE CheatTable [<!ENTITY a "aaaa">]><CheatTable/>"#;
        assert!(matches!(
            convert(dtd.as_bytes(), "x", None, "2026-11-08"),
            Err(CtError::NotTable(_))
        ));
        let big = vec![b' '; (MAX_TABLE_BYTES + 1) as usize];
        assert!(matches!(
            convert(&big, "x", None, "2026-11-08"),
            Err(CtError::TooBig(_))
        ));
    }

    #[test]
    fn addresses_parse_or_explain() {
        let ok = |s: &str| {
            parse_address(s).map(|c| {
                (
                    c.base(),
                    c.offsets.iter().map(|&o| hex(o)).collect::<Vec<_>>(),
                )
            })
        };
        assert_eq!(ok("game.exe").unwrap(), ("game.exe+0x0".into(), vec![]));
        assert_eq!(
            ok("\"My Game.exe\" + 10 + 8").unwrap(),
            ("My Game.exe+0x18".into(), vec![])
        );
        assert_eq!(
            ok("game.exe-10+30").unwrap(),
            ("game.exe+0x20".into(), vec![])
        );
        assert_eq!(
            ok("[game.exe+10]").unwrap(),
            ("game.exe+0x10".into(), vec!["0x0".into()])
        );
        assert_eq!(
            ok("[[game.exe+10]+20]-8").unwrap(),
            ("game.exe+0x10".into(), vec!["0x20".into(), "-0x8".into()])
        );
        assert!(ok("[1234]+8").unwrap_err().contains("fixed number"));
        assert!(ok("[game.exe+10").unwrap_err().contains("unclosed"));
        assert!(ok("game.exe+player").unwrap_err().contains("player"));
        assert!(ok("game.exe-20").unwrap_err().contains("before the start"));
        assert!(ok(&"[".repeat(40)).is_err());
    }

    #[test]
    fn huge_numbers_are_refused_not_wrapped() {
        let ok = |s: &str| parse_address(s).map(|c| c.base());
        assert!(
            ok("game.exe+7FFFFFFFFFFFFFFF+7FFFFFFFFFFFFFFF")
                .unwrap_err()
                .contains("out of range")
        );
        assert!(
            ok("[game.exe+10]+7FFFFFFFFFFFFFFF+7FFFFFFFFFFFFFFF")
                .unwrap_err()
                .contains("out of range")
        );
        assert!(ok("game.exe-7FFFFFFFFFFFFFFF-7FFFFFFFFFFFFFFF").is_err());
        let t = r#"<CheatTable><CheatEntries><CheatEntry><Description>"P"</Description><VariableType>4 Bytes</VariableType>
            <Address>game.exe+7FFFFFFFFFFFFFFF</Address><CheatEntries><CheatEntry><Description>"C"</Description>
            <VariableType>4 Bytes</VariableType><Address>+7FFFFFFFFFFFFFFF</Address></CheatEntry></CheatEntries>
            </CheatEntry></CheatEntries></CheatTable>"#;
        let got = convert(t.as_bytes(), "x", None, "2026-11-08").unwrap();
        assert_eq!(got.pack.cheats.len(), 1);
        assert!(
            got.skipped[0].why.contains("out of range"),
            "{:?}",
            got.skipped
        );
    }

    #[test]
    fn a_group_with_an_address_is_the_base_of_its_children() {
        let t = r#"<CheatTable><CheatEntries><CheatEntry><Description>"Player"</Description><GroupHeader>1</GroupHeader>
            <Address>[game.exe+100]</Address><CheatEntries>
            <CheatEntry><Description>"Health"</Description><VariableType>4 Bytes</VariableType><Address>+10</Address></CheatEntry>
            </CheatEntries></CheatEntry></CheatEntries></CheatTable>"#;
        let got = convert(t.as_bytes(), "x", None, "2026-11-08").unwrap();
        let (v, cat) = value(&got.pack.cheats[0]);
        assert_eq!(cat, "Player");
        assert_eq!(v.address.base.as_deref(), Some("game.exe+0x100"));
        assert_eq!(v.address.offsets, ["0x10"]);
    }

    #[test]
    fn too_many_xml_nodes_are_refused() {
        let mut t = String::from("<CheatTable>");
        t.push_str(&"<a/>".repeat(MAX_NODES as usize));
        t.push_str("</CheatTable>");
        assert!(matches!(
            convert(t.as_bytes(), "x", None, "2026-11-08"),
            Err(CtError::NotTable(_))
        ));
    }

    #[test]
    fn deep_or_huge_tables_stop_safely() {
        let mut t = String::from("<CheatTable><CheatEntries>");
        for _ in 0..100 {
            t.push_str("<CheatEntry><Description>\"g\"</Description><GroupHeader>1</GroupHeader><CheatEntries>");
        }
        for _ in 0..100 {
            t.push_str("</CheatEntries></CheatEntry>");
        }
        t.push_str("</CheatEntries></CheatTable>");
        let got = convert(t.as_bytes(), "x", None, "2026-11-08").unwrap();
        assert!(got.pack.cheats.is_empty());

        let mut t = String::from("<CheatTable><CheatEntries>");
        for i in 0..(MAX_CHEATS + 3) {
            t.push_str(&format!(
                "<CheatEntry><Description>\"v{i}\"</Description><VariableType>4 Bytes</VariableType><Address>g.exe+{i:X}</Address></CheatEntry>"
            ));
        }
        t.push_str("</CheatEntries></CheatTable>");
        let got = convert(t.as_bytes(), "x", None, "2026-11-08").unwrap();
        assert_eq!(got.pack.cheats.len(), MAX_CHEATS);
        assert_eq!(got.skipped.len(), 3);
    }
}
