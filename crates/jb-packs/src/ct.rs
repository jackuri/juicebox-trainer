//! Cheat Engine tables (`.CT`) to packs.
//!
//! A table is XML. Value entries come over: a number type, an address inside
//! a module (`game.exe+1234`, `[[game.exe+10]+20]+8`) and optional pointer
//! offsets, also when they start from a name a table script defines and
//! JuiceBox Trainer can find itself (see [`crate::ct_script`]), and offsets with
//! simple math (`slot*6180+2F90`). Scripts that only swap a few bytes become
//! checked patch cheats. Everything else is listed as skipped with the reason,
//! never run: other Auto Assembler and Lua scripts, fixed addresses, text and
//! byte-array values. The pack that comes out goes through the normal
//! validator like any other pack.
//!
//! The XML is read with DTDs refused, from at most [`MAX_TABLE_BYTES`].

use std::collections::{BTreeMap, HashMap, HashSet};

use jb_core::ValueType;
use serde::Serialize;

use crate::ct_script::{self, Sym};
use crate::schema::{
    Address, Capture, Cheat, Game, Impl, Meta, NumberConfig, Operand, Pack, PackArch, PatchImpl,
    SCHEMA_VERSION, Source, Ui, ValueImpl, When,
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

const SCRIPT: &str = "runs its own code (a script); JuiceBox Trainer never runs code from a table";

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
    walk.read_scripts(root);
    if let Some(entries) = child(root, "CheatEntries") {
        walk.entries(entries, None, None, 0);
    }
    if child(root, "LuaScript").is_some_and(|n| !text_of(n).trim().is_empty()) {
        walk.skipped.push(Skipped {
            name: "The table's own script (Lua)".into(),
            why: "JuiceBox Trainer never runs code from a table".into(),
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
            // Patches are only taken from tables that show a 64-bit game;
            // captures name registers, which differ between 32 and 64 bits.
            arch: walk
                .cheats
                .iter()
                .any(|c| match &c.implementation {
                    Impl::Patch(_) => true,
                    Impl::Value(v) => v.address.capture.is_some(),
                })
                .then_some(if walk.x64 {
                    PackArch::X64
                } else {
                    PackArch::X86
                }),
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
            source: Some(Source {
                file: Some(format!("{file_stem}.CT")),
                ..Source::default()
            }),
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
    /// Names the table's scripts define that JuiceBox Trainer can redo itself.
    syms: HashMap<String, Sym>,
    /// A script uses 64-bit registers: the game is 64-bit.
    x64: bool,
}

/// Something wrong with an address, said to the player.
const OUT_OF_RANGE: &str = "its address is out of range";

impl Walk {
    /// Reads every script in the table first (as text), so values can use the
    /// names they define wherever they are.
    fn read_scripts(&mut self, root: roxmltree::Node) {
        let mut n = 0usize;
        for s in root
            .descendants()
            .filter(|d| d.has_tag_name("AssemblerScript"))
        {
            n += 1;
            if n > MAX_ENTRIES {
                break;
            }
            let text = text_of(s);
            ct_script::symbols(&text, &mut self.syms);
            self.x64 |= ct_script::mentions_x64(&text);
        }
    }

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
        let address = child(e, "Address")
            .map(|n| text_of(n).trim().to_string())
            .filter(|a| !a.is_empty());
        let group = child(e, "GroupHeader").is_some_and(|n| text_of(n).trim() == "1")
            || (ty.is_none() && address.is_none());

        // The entry's address, if it has one: its children's `+X` addresses
        // add to it even when the entry itself can't come over (e.g. text).
        let chain = address.as_deref().map(|a| self.chain(e, a, parent));
        let kids_base = chain.as_ref().and_then(|c| c.as_ref().ok()).cloned();

        if ty.as_deref() == Some("Auto Assembler Script") {
            let script = child(e, "AssemblerScript").map(text_of).unwrap_or_default();
            self.script(&name, category, &script);
        } else if !group {
            let signed = child(e, "ShowAsSigned").is_some_and(|n| text_of(n).trim() == "1");
            let result = match (ty.as_deref(), chain) {
                (None, _) => Err("it has no value type".to_string()),
                (Some(t), _) if value_type(t, signed).is_none() => Err(format!(
                    "its type ({t}) isn't a number JuiceBox Trainer can set"
                )),
                (_, None) => Err("it has no address".to_string()),
                (Some(t), Some(c)) => {
                    c.map(|c| (c, value_type(t, signed).unwrap_or(ValueType::U32)))
                }
            };
            match result.and_then(|(c, vt)| Ok((c.address()?, vt))) {
                Ok((address, vt)) => self.add_value(&name, category, address, vt),
                Err(why) => self.skipped.push(Skipped {
                    name: name.clone(),
                    why,
                }),
            }
        }
        if let Some(kids) = child(e, "CheatEntries") {
            let cat = if group || category.is_none() {
                name.as_str()
            } else {
                category.unwrap_or_default()
            };
            // Entries under a script use the names that script defines (tables
            // often define the same name in several scripts).
            let saved = if ty.as_deref() == Some("Auto Assembler Script") {
                let script = child(e, "AssemblerScript").map(text_of).unwrap_or_default();
                let mut own = HashMap::new();
                ct_script::symbols(&script, &mut own);
                (!own.is_empty()).then(|| {
                    let saved = self.syms.clone();
                    self.syms.extend(own);
                    saved
                })
            } else {
                None
            };
            self.entries(kids, Some(cat), kids_base.as_ref(), depth + 1);
            if let Some(saved) = saved {
                self.syms = saved;
            }
        }
    }

    /// A script entry: a byte swap becomes a patch cheat; a script that only
    /// finds addresses JuiceBox Trainer can find itself isn't needed; anything
    /// else is left out.
    fn script(&mut self, name: &str, category: Option<&str>, script: &str) {
        if let Some(p) = ct_script::byte_patch(script) {
            if !self.x64 {
                self.skipped.push(Skipped {
                    name: name.into(),
                    why: "it changes game code, but the table doesn't show whether the game is 64-bit".into(),
                });
                return;
            }
            self.add_patch(name, category, p);
            return;
        }
        let mut mine = HashMap::new();
        ct_script::symbols(script, &mut mine);
        let only_finds = ct_script::only_registers(script, &mine);
        if !only_finds {
            self.skipped.push(Skipped {
                name: name.into(),
                why: SCRIPT.into(),
            });
        }
    }

    /// The address an entry's `Address` and `Offsets` describe.
    fn chain(
        &self,
        e: roxmltree::Node,
        address: &str,
        parent: Option<&Chain>,
    ) -> Result<Chain, String> {
        let mut chain = if let Some(rel) = address.strip_prefix('+') {
            let n = eval(rel, &self.syms).ok_or("its address isn't a plain hex offset")?;
            let mut c = parent
                .cloned()
                .ok_or("it builds on an entry above it that couldn't be imported")?;
            c.add(n)?;
            c
        } else if address.starts_with('-') {
            let n = eval(address, &self.syms).ok_or("its address isn't a plain hex offset")?;
            let mut c = parent
                .cloned()
                .ok_or("it builds on an entry above it that couldn't be imported")?;
            c.add(n)?;
            c
        } else {
            parse_address(address, &self.syms)?
        };
        if let Some(offs) = child(e, "Offsets") {
            let list: Vec<String> = offs
                .children()
                .filter(|n| n.has_tag_name("Offset"))
                .map(text_of)
                .collect();
            // Cheat Engine stores them last-applied first.
            for o in list.iter().rev() {
                let n = eval(o, &self.syms).ok_or_else(|| {
                    format!(
                        "its pointer offset \"{}\" isn't something JuiceBox Trainer can work out",
                        o.trim()
                    )
                })?;
                chain.push_offset(n)?;
            }
        }
        Ok(chain)
    }

    fn new_id(&mut self, name: &str) -> Option<String> {
        if self.cheats.len() >= MAX_CHEATS {
            self.skipped.push(Skipped {
                name: name.into(),
                why: format!("only the first {MAX_CHEATS} entries of a table are imported"),
            });
            return None;
        }
        let base = slug(name, "value");
        let mut id = base.chars().take(56).collect::<String>();
        let mut n = 2;
        while self.ids.contains(&id) {
            id = format!("{}-{n}", base.chars().take(56).collect::<String>());
            n += 1;
        }
        self.ids.insert(id.clone());
        Some(id)
    }

    fn add_value(&mut self, name: &str, category: Option<&str>, address: Address, ty: ValueType) {
        let Some(id) = self.new_id(name) else { return };
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
            implementation: Impl::Value(ValueImpl { ty, address }),
        });
    }

    fn add_patch(&mut self, name: &str, category: Option<&str>, p: ct_script::BytePatch) {
        let Some(id) = self.new_id(name) else { return };
        self.cheats.push(Cheat {
            id,
            name: name.chars().take(80).collect(),
            category: category.map(|c| c.chars().take(60).collect()),
            description: Some(
                "Changes a few bytes of game code (from the table); turned off, they're put back."
                    .into(),
            ),
            ui: Ui::Toggle,
            hotkey: None,
            toggle: None,
            slider: None,
            number: None,
            button: None,
            implementation: Impl::Patch(PatchImpl {
                module: p.module,
                aob: wildcard_changed(&p.aob, p.offset, &p.original, &p.replace),
                offset: p.offset,
                original: p.original,
                replace: p.replace,
            }),
        });
    }

    /// The exe the table's addresses use most.
    fn likely_exe(&self) -> Option<String> {
        let mut count: BTreeMap<String, (usize, String)> = BTreeMap::new();
        for c in &self.cheats {
            let module = match &c.implementation {
                Impl::Value(v) => v
                    .address
                    .base
                    .as_ref()
                    .and_then(|b| b.rsplit_once('+').map(|(m, _)| m.to_string()))
                    .or_else(|| v.address.module.clone()),
                Impl::Patch(p) => p.module.clone(),
            };
            if let Some(m) = module
                && m.to_ascii_lowercase().ends_with(".exe")
            {
                count.entry(m.to_ascii_lowercase()).or_insert((0, m)).0 += 1;
            }
        }
        count.into_values().max_by_key(|(n, _)| *n).map(|(_, m)| m)
    }
}

/// The bytes a patch changes become `??` in its signature, so a game that is
/// already patched is still found (and can be put back); only if at least 4
/// fixed bytes are left.
fn wildcard_changed(aob: &str, offset: i64, original: &str, replace: &str) -> String {
    let mut bytes: Vec<String> = aob.split_whitespace().map(str::to_string).collect();
    let (Ok(start), true) = (usize::try_from(offset), offset >= 0) else {
        return aob.to_string();
    };
    for (k, (o, r)) in original
        .split_whitespace()
        .zip(replace.split_whitespace())
        .enumerate()
    {
        if o != r
            && let Some(b) = bytes.get_mut(start + k)
        {
            *b = "??".into();
        }
    }
    if bytes.iter().filter(|b| *b != "??").count() >= 4 {
        bytes.join(" ")
    } else {
        aob.to_string()
    }
}

/// What an address starts from.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Start {
    /// `game.exe+...`
    Module(String),
    /// Found by its bytes (a name a script defines with `aobscanmodule`).
    Aob {
        module: Option<String>,
        aob: String,
        offset: i64,
        rip_next: Option<i64>,
    },
    /// A register's value when the game runs the code there (a pointer a
    /// script grabs). `stored`: still at the script's variable that holds
    /// it; the first pointer read gives the register's value itself.
    Capture {
        module: Option<String>,
        aob: String,
        offset: i64,
        register: String,
        when: Option<(String, i64)>,
        stored: bool,
    },
}

/// One pointer read (`[x]` or a table offset) on a path.
fn deref(start: &mut Option<Start>, base: &mut i64, offsets: &mut Vec<i64>) -> Result<(), String> {
    if let Some(Start::Capture { stored, .. }) = start
        && *stored
    {
        if *base != 0 {
            return Err(
                "it reads next to where a table script keeps a pointer, not the pointer itself"
                    .into(),
            );
        }
        // Reading the script's variable gives the register's value.
        *stored = false;
        return Ok(());
    }
    offsets.push(0);
    Ok(())
}

/// A pointer path: the start plus `base`, then for each offset read a
/// pointer and add it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Chain {
    start: Start,
    base: i64,
    offsets: Vec<i64>,
}

impl Chain {
    /// A table offset: read a pointer, then add `o`.
    fn push_offset(&mut self, o: i64) -> Result<(), String> {
        let mut start = Some(self.start.clone());
        deref(&mut start, &mut self.base, &mut self.offsets)?;
        if let Some(s) = start {
            self.start = s;
        }
        self.add(o)
    }

    fn add(&mut self, n: i64) -> Result<(), String> {
        let slot = match self.offsets.last_mut() {
            Some(o) => o,
            None => &mut self.base,
        };
        *slot = slot.checked_add(n).ok_or(OUT_OF_RANGE)?;
        Ok(())
    }

    /// As a pack address.
    fn address(&self) -> Result<Address, String> {
        let offsets = self.offsets.iter().map(|&o| hex(o)).collect();
        Ok(match &self.start {
            Start::Module(m) => {
                if self.base < 0 {
                    return Err("its address is before the start of the module".into());
                }
                Address {
                    base: Some(format!("{m}+{}", hex(self.base))),
                    aob: None,
                    module: None,
                    offset: None,
                    operand: None,
                    next: None,
                    offsets,
                    capture: None,
                }
            }
            Start::Aob {
                module,
                aob,
                offset,
                rip_next,
            } => {
                let offset = match rip_next {
                    // The found address is the operand's target: adding to it
                    // before reading a pointer can't be said in a pack.
                    Some(_) if self.base != 0 => {
                        return Err("its address adds to a found address in a way JuiceBox Trainer can't express".into());
                    }
                    Some(_) => *offset,
                    None => offset.checked_add(self.base).ok_or(OUT_OF_RANGE)?,
                };
                Address {
                    base: None,
                    aob: Some(aob.clone()),
                    module: module.clone(),
                    offset: (offset != 0).then_some(offset),
                    operand: rip_next.map(|_| Operand::Rip32),
                    next: *rip_next,
                    offsets,
                    capture: None,
                }
            }
            Start::Capture { stored: true, .. } => {
                return Err(
                    "it uses where a table script keeps a pointer, not the pointer itself".into(),
                );
            }
            Start::Capture {
                module,
                aob,
                offset,
                register,
                when,
                stored: false,
            } => Address {
                base: None,
                aob: Some(aob.clone()),
                module: module.clone(),
                offset: (*offset != 0).then_some(*offset),
                operand: None,
                next: None,
                offsets,
                capture: Some(Capture {
                    register: register.clone(),
                    add: (self.base != 0).then(|| hex(self.base)),
                    when: when.as_ref().map(|(r, v)| When {
                        register: r.clone(),
                        equals: *v,
                    }),
                }),
            },
        })
    }

    #[cfg(test)]
    fn base(&self) -> String {
        match &self.start {
            Start::Module(m) => format!("{m}+{}", hex(self.base)),
            Start::Aob { aob, .. } => format!("aob({aob})+{}", hex(self.base)),
            Start::Capture { register, .. } => format!("{register}+{}", hex(self.base)),
        }
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

/// A number word: a name a script defined as a number, else hex.
fn number_word(w: &str, syms: &HashMap<String, Sym>) -> Option<i64> {
    match syms.get(&w.to_ascii_lowercase()) {
        Some(Sym::Const(n)) => Some(*n),
        Some(_) => None,
        None => parse_hex(w),
    }
}

/// Evaluates an offset like `gameoffset*6180-6180+2F90+C`: hex numbers and
/// numbers scripts define, with `+ - *` and brackets. `None` if it uses
/// anything else or overflows.
fn eval(expr: &str, syms: &HashMap<String, Sym>) -> Option<i64> {
    struct E<'a> {
        s: &'a [u8],
        i: usize,
        syms: &'a HashMap<String, Sym>,
        depth: usize,
    }
    impl E<'_> {
        fn ws(&mut self) {
            while self.s.get(self.i).is_some_and(u8::is_ascii_whitespace) {
                self.i += 1;
            }
        }
        fn sum(&mut self) -> Option<i64> {
            let mut v = self.product()?;
            loop {
                self.ws();
                match self.s.get(self.i) {
                    Some(b'+') => {
                        self.i += 1;
                        v = v.checked_add(self.product()?)?;
                    }
                    Some(b'-') => {
                        self.i += 1;
                        v = v.checked_sub(self.product()?)?;
                    }
                    _ => return Some(v),
                }
            }
        }
        fn product(&mut self) -> Option<i64> {
            let mut v = self.atom()?;
            loop {
                self.ws();
                if self.s.get(self.i) == Some(&b'*') {
                    self.i += 1;
                    v = v.checked_mul(self.atom()?)?;
                } else {
                    return Some(v);
                }
            }
        }
        fn atom(&mut self) -> Option<i64> {
            self.ws();
            match self.s.get(self.i)? {
                b'(' => {
                    self.depth += 1;
                    if self.depth > 16 {
                        return None;
                    }
                    self.i += 1;
                    let v = self.sum()?;
                    self.ws();
                    (self.s.get(self.i) == Some(&b')')).then_some(())?;
                    self.i += 1;
                    self.depth -= 1;
                    Some(v)
                }
                b'-' => {
                    self.i += 1;
                    self.atom()?.checked_neg()
                }
                _ => {
                    let start = self.i;
                    while self
                        .s
                        .get(self.i)
                        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
                    {
                        self.i += 1;
                    }
                    let w = std::str::from_utf8(&self.s[start..self.i]).ok()?;
                    if w.is_empty() {
                        return None;
                    }
                    let w = w
                        .strip_prefix("0x")
                        .or_else(|| w.strip_prefix("0X"))
                        .unwrap_or(w);
                    number_word(w, self.syms)
                }
            }
        }
    }
    if expr.len() > 200 {
        return None;
    }
    let mut e = E {
        s: expr.trim().as_bytes(),
        i: 0,
        syms,
        depth: 0,
    };
    let v = e.sum()?;
    e.ws();
    (e.i == e.s.len()).then_some(v)
}

fn is_module(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    (l.ends_with(".exe") || l.ends_with(".dll")) && l.len() > 4 && !s.contains(['/', '\\', ':'])
}

/// Parses `"game.exe"+1234`, `game.exe+10+8`, `[[game.exe+10]+20]+8`, and the
/// same starting from a name a script defines (`[gamedata]+10`).
fn parse_address(s: &str, syms: &HashMap<String, Sym>) -> Result<Chain, String> {
    let mut p = Parser {
        s: s.as_bytes(),
        i: 0,
        depth: 0,
        syms,
    };
    let chain = p.expr()?;
    p.skip_ws();
    if p.i != p.s.len() {
        return Err(format!(
            "its address ({s}) isn't a module address JuiceBox Trainer understands"
        ));
    }
    let start = chain.start.ok_or(
        "its address is a fixed number, which usually changes every time the game starts"
            .to_string(),
    )?;
    if matches!(start, Start::Module(_)) && chain.base < 0 {
        return Err("its address is before the start of the module".into());
    }
    Ok(Chain {
        start,
        base: chain.base,
        offsets: chain.offsets,
    })
}

struct Partial {
    start: Option<Start>,
    base: i64,
    offsets: Vec<i64>,
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    depth: usize,
    syms: &'a HashMap<String, Sym>,
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
            while self
                .s
                .get(self.i)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
            {
                self.i += 1;
            }
            let word = std::str::from_utf8(&self.s[start..self.i]).unwrap_or_default();
            let n = number_word(word, self.syms).ok_or_else(|| {
                format!(
                    "it depends on \"{word}\", which a table script works out while the game runs; JuiceBox Trainer can't redo that one yet"
                )
            })?;
            let n = if sign < 0 { n.checked_neg() } else { Some(n) }.ok_or(OUT_OF_RANGE)?;
            let slot = match c.offsets.last_mut() {
                Some(o) => o,
                None => &mut c.base,
            };
            *slot = slot.checked_add(n).ok_or(OUT_OF_RANGE)?;
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
                if inner.start.is_none() {
                    return Err("its address starts from a fixed number, which usually changes every time the game starts".into());
                }
                deref(&mut inner.start, &mut inner.base, &mut inner.offsets)?;
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
                    && let Some(n) = number_word(right, self.syms)
                {
                    let mut c = self.word(left.trim())?;
                    c.base = c.base.checked_sub(n).ok_or(OUT_OF_RANGE)?;
                    return Ok(c);
                }
                self.word(&raw)
            }
        }
    }

    fn word(&self, w: &str) -> Result<Partial, String> {
        if is_module(w) {
            return Ok(Partial {
                start: Some(Start::Module(w.to_string())),
                base: 0,
                offsets: Vec::new(),
            });
        }
        match self.syms.get(&w.to_ascii_lowercase()) {
            Some(Sym::Module { module, offset }) => {
                return Ok(Partial {
                    start: Some(Start::Module(module.clone())),
                    base: *offset,
                    offsets: Vec::new(),
                });
            }
            Some(Sym::Aob {
                module,
                aob,
                offset,
                rip_next,
            }) => {
                return Ok(Partial {
                    start: Some(Start::Aob {
                        module: module.clone(),
                        aob: aob.clone(),
                        offset: *offset,
                        rip_next: *rip_next,
                    }),
                    base: 0,
                    offsets: Vec::new(),
                });
            }
            Some(Sym::Capture {
                module,
                aob,
                offset,
                register,
                when,
            }) => {
                return Ok(Partial {
                    start: Some(Start::Capture {
                        module: module.clone(),
                        aob: aob.clone(),
                        offset: *offset,
                        register: register.clone(),
                        when: when.clone(),
                        stored: true,
                    }),
                    base: 0,
                    offsets: Vec::new(),
                });
            }
            Some(Sym::Const(n)) => {
                return Ok(Partial {
                    start: None,
                    base: *n,
                    offsets: Vec::new(),
                });
            }
            None => {}
        }
        if let Some(n) = parse_hex(w) {
            return Ok(Partial {
                start: None,
                base: n,
                offsets: Vec::new(),
            });
        }
        if w.is_empty() {
            return Err("its address is empty".into());
        }
        Err(format!(
            "it depends on \"{w}\", which a table script works out while the game runs; JuiceBox Trainer can't redo that one yet"
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
            parse_address(s, &HashMap::new()).map(|c| {
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
    fn offset_math_uses_numbers_scripts_define() {
        let syms = HashMap::from([
            ("slot".to_string(), Sym::Const(2)),
            ("add".to_string(), Sym::Const(1)),
        ]);
        assert_eq!(
            eval("slot*6180-6180+2F90+C", &syms),
            Some(0x6180 + 0x2F90 + 0xC)
        );
        assert_eq!(eval("(slot+1)*10", &syms), Some(0x30));
        assert_eq!(eval("-8", &syms), Some(-8));
        // A defined name wins over reading it as hex ("add" is also hex).
        assert_eq!(eval("add", &syms), Some(1));
        assert_eq!(eval("ADD", &HashMap::new()), Some(0xADD));
        assert_eq!(eval("unknown_name+4", &syms), None);
        assert_eq!(eval("7FFFFFFFFFFFFFFF*2", &syms), None);
        assert_eq!(eval("1+", &syms), None);
        assert_eq!(eval(&"(".repeat(40), &syms), None);
    }

    #[test]
    fn addresses_can_start_from_names_scripts_define() {
        let found = Sym::Aob {
            module: Some("g.exe".into()),
            aob: "48 8B 0D ?? ?? ?? ??".into(),
            offset: 3,
            rip_next: Some(7),
        };
        let syms = HashMap::from([
            ("gamedata".to_string(), found),
            (
                "site".to_string(),
                Sym::Aob {
                    module: None,
                    aob: "75 15 8B D3".into(),
                    offset: 0,
                    rip_next: None,
                },
            ),
            (
                "base".to_string(),
                Sym::Module {
                    module: "g.exe".into(),
                    offset: 0x100,
                },
            ),
        ]);
        let a = parse_address("[[gamedata]+10]+8", &syms)
            .unwrap()
            .address()
            .unwrap();
        assert_eq!(a.aob.as_deref(), Some("48 8B 0D ?? ?? ?? ??"));
        assert_eq!(
            (a.offset, a.operand, a.next),
            (Some(3), Some(Operand::Rip32), Some(7))
        );
        assert_eq!(a.offsets, ["0x10", "0x8"]);
        // Adding to a found code address moves where it points.
        let a = parse_address("site+2", &syms).unwrap().address().unwrap();
        assert_eq!((a.offset, a.operand), (Some(2), None));
        let a = parse_address("base+10", &syms).unwrap().address().unwrap();
        assert_eq!(a.base.as_deref(), Some("g.exe+0x110"));
        // Adding to a RIP target before a pointer read can't be said in a pack.
        assert!(
            parse_address("gamedata+8", &syms)
                .unwrap()
                .address()
                .is_err()
        );
    }
    #[test]
    fn huge_numbers_are_refused_not_wrapped() {
        let ok = |s: &str| parse_address(s, &HashMap::new()).map(|c| c.base());
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

    /// Converts a real table on this PC and prints what came over (manual:
    /// `JB_CT_FILE=... cargo test -p jb-packs ct::tests::real_table -- --ignored --nocapture`).
    #[test]
    #[ignore = "needs a real Cheat Engine table on this PC"]
    fn real_table() {
        let path = std::path::PathBuf::from(std::env::var("JB_CT_FILE").unwrap());
        let got = convert(&std::fs::read(&path).unwrap(), "Real", None, "2026-11-08").unwrap();
        let patches = got
            .pack
            .cheats
            .iter()
            .filter(|c| matches!(c.implementation, Impl::Patch(_)))
            .count();
        println!(
            "exe: {} (known {}), arch {:?}",
            got.pack.game.exe, got.exe_known, got.pack.game.arch
        );
        println!(
            "cheats: {} ({} values, {patches} patches)",
            got.pack.cheats.len(),
            got.pack.cheats.len() - patches
        );
        for c in got.pack.cheats.iter().take(12) {
            println!("  + {} [{}]", c.name, c.category.as_deref().unwrap_or(""));
        }
        let mut why: BTreeMap<String, usize> = BTreeMap::new();
        for s in &got.skipped {
            *why.entry(s.why.chars().take(70).collect()).or_default() += 1;
        }
        println!("skipped: {}", got.skipped.len());
        for (w, n) in why {
            println!("  {n:4} x {w}");
        }
        let text = serde_json::to_string_pretty(&got.pack).unwrap();
        crate::parse(&text).unwrap();
        std::fs::write(std::env::temp_dir().join("ot-real-table.json"), text).unwrap();
    }
}
