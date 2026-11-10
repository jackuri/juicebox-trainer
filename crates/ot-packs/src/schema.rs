//! Cheat pack format v1 (`docs/PLAN.md` §4). The JSON Schema for editors and
//! the AI lives in `schema/pack.v1.json`; these types are what the app reads.
//! Unknown fields are rejected everywhere.

use ot_core::ValueType;
use serde::{Deserialize, Serialize};
use serde_json::Number;

/// The only schema version this build reads.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pack {
    pub schema: u32,
    pub game: Game,
    pub meta: Meta,
    pub cheats: Vec<Cheat>,
    /// Save-file quick codes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub save_codes: Vec<SaveCode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Game {
    /// Lowercase slug, e.g. `testbed-native`. Also the pack's folder name.
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    /// Executable file name, e.g. `testbed_native.exe`.
    pub exe: String,
    /// Architecture of the game build. Required for packs with code patches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<PackArch>,
    /// Builds (exe SHA-256) this pack was tested on.
    #[serde(default)]
    pub builds: Vec<Build>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackArch {
    X86,
    X64,
}

impl PackArch {
    pub fn core(self) -> ot_core::Arch {
        match self {
            PackArch::X86 => ot_core::Arch::X86,
            PackArch::X64 => ot_core::Arch::X64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Build {
    /// Lowercase hex SHA-256 of the exe.
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Every cheat was confirmed working on this build.
    #[serde(default)]
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Meta {
    /// Free text. `ai` marks a pack made by the AI creator.
    pub author: String,
    /// `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// SemVer of the pack itself.
    pub pack_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Where the cheats came from, to credit their maker (v1.15.0): for a
    /// converted Cheat Engine table, the table's maker and page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// Credit for cheats made by someone else (a Cheat Engine table's maker).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    /// Who made them (free text, e.g. a forum name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    /// The page they were published on (http or https).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The file they were converted from, e.g. `Hollow_Knight.CT`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

/// The longest maker name and page address a pack may give.
pub const MAX_SOURCE_BY: usize = 100;
pub const MAX_SOURCE_URL: usize = 400;

/// A page address a pack may give for its source: http(s), no spaces or
/// control characters.
pub fn is_source_url(url: &str) -> bool {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    rest.is_some_and(|r| {
        !r.is_empty()
            && !r.starts_with('/')
            && url.len() <= MAX_SOURCE_URL
            && !url
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || c == '"' || c == '<' || c == '>')
    })
}

impl Meta {
    pub fn is_ai(&self) -> bool {
        self.author.trim().eq_ignore_ascii_case("ai")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Cheat {
    /// Lowercase slug, unique in the pack.
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub ui: Ui,
    /// e.g. `F1`, `Ctrl+Num1`. Toggles and buttons only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggle: Option<ToggleConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slider: Option<SliderConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<NumberConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button: Option<ButtonConfig>,
    #[serde(rename = "impl")]
    pub implementation: Impl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ui {
    /// On/off. Value cheats freeze; patch cheats apply and restore.
    Toggle,
    /// Sets a value from a range.
    Slider,
    /// Sets a typed-in value.
    Number,
    /// One-shot action (add to or set a value).
    Button,
}

/// Options for a value toggle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToggleConfig {
    /// Freeze at this value. Without it, the value is frozen where it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Number>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SliderConfig {
    pub min: Number,
    pub max: Number,
    pub step: Number,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Number>,
    /// Keep the value frozen after setting it.
    #[serde(default)]
    pub hold: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumberConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<Number>,
    /// Keep the value frozen after setting it.
    #[serde(default)]
    pub hold: bool,
}

/// What a button does. Exactly one of `add` / `set`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ButtonConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add: Option<Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set: Option<Number>,
}

/// How a cheat touches the game. `cave` ops arrive in v0.5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Impl {
    Value(ValueImpl),
    Patch(PatchImpl),
}

/// A typed value at a resolved address.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValueImpl {
    #[serde(rename = "type")]
    pub ty: ValueType,
    pub address: Address,
}

/// Where a value lives. Either a static `base` (`module.exe+0x…`) or an AOB
/// signature in code, optionally followed by a pointer chain.
///
/// AOB form: the match address plus `offset` is the start point. With no
/// `operand` that is the base. `abs32` reads a 32-bit absolute address stored
/// there (x86 code); `rip32` reads a signed 32-bit displacement there and adds
/// it to the end of the instruction, `match + next` (x64 code).
///
/// Then for each entry in `offsets`: read a pointer, add the offset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Address {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aob: Option<String>,
    /// Module to search; defaults to the game exe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operand: Option<Operand>,
    /// For `rip32`: offset of the end of the instruction from the match start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<i64>,
    /// Hex offsets (`"0x18"`, `"-0x8"`), applied after a pointer read each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub offsets: Vec<String>,
    /// AOB form only: instead of the found address itself, the value a
    /// register holds when the game runs the code there (see [`Capture`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture: Option<Capture>,
}

/// Read a register when the game runs the code at the found address (match +
/// `offset`), with a hardware breakpoint: no game code is changed. That value
/// plus `add` is the start point; then `offsets` apply as usual. The same idea
/// as a Cheat Engine script that copies a register into a pointer variable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capture {
    /// `rdx`, `rsi`, `r12` (64-bit games; `edx` for the low half) or `edx`
    /// (32-bit games).
    pub register: String,
    /// Hex offset added to the register's value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add: Option<String>,
    /// Only when this register holds this value at that moment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<When>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct When {
    pub register: String,
    pub equals: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operand {
    Rip32,
    Abs32,
}

/// Replaces bytes at an AOB-located site. The original bytes are checked
/// before every write; turning the cheat off writes them back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PatchImpl {
    /// Module to search; defaults to the game exe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    pub aob: String,
    /// From the match start to the first patched byte.
    #[serde(default)]
    pub offset: i64,
    /// Bytes expected at the site, no wildcards.
    pub original: String,
    /// Bytes to write, same length as `original`.
    pub replace: String,
}

/// Folder placeholders a save code's `file` starts with. Keep in sync with
/// `ot_saves::quick::PLACEHOLDERS` (a desktop test checks).
pub const SAVE_PLACEHOLDERS: &[&str] = &[
    "{documents}",
    "{mygames}",
    "{savedgames}",
    "{appdata}",
    "{localappdata}",
    "{localappdatalow}",
    "{playerprefs}",
];

/// Save code formats. Keep in sync with `ot_saves::quick::CODECS`.
pub const SAVE_CODECS: &[&str] = &[
    "auto",
    "json",
    "xml",
    "ini",
    "yaml",
    "gvas",
    "sqlite",
    "playerprefs",
];

/// A named save-file edit, applied from the Save backups view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveCode {
    pub id: String,
    pub name: String,
    pub codec: String,
    pub file: String,
    pub set: SaveSet,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveSet {
    pub path: String,
    pub value: serde_json::Value,
}

/// Parses a hex offset: `0x18`, `18`, `-0x8`.
pub fn parse_offset(s: &str) -> Option<i64> {
    let t = s.trim().replace('_', "");
    let (neg, body) = match t.strip_prefix('-') {
        Some(rest) => (true, rest.to_string()),
        None => (false, t.strip_prefix('+').unwrap_or(&t).to_string()),
    };
    let body = body
        .strip_prefix("0x")
        .or_else(|| body.strip_prefix("0X"))
        .unwrap_or(&body);
    // One sign only: `from_str_radix` would also take a second one.
    if body.is_empty() || body.starts_with(['+', '-']) {
        return None;
    }
    let v = i64::from_str_radix(body, 16).ok()?;
    if neg { v.checked_neg() } else { Some(v) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_parse_as_hex() {
        assert_eq!(parse_offset("0x18"), Some(0x18));
        assert_eq!(parse_offset("18"), Some(0x18));
        assert_eq!(parse_offset("-0x8"), Some(-8));
        assert_eq!(parse_offset("+0x8"), Some(8));
        assert_eq!(parse_offset(""), None);
        assert_eq!(parse_offset("0x"), None);
        assert_eq!(parse_offset("zz"), None);
        // Never a double sign or an overflow.
        assert_eq!(parse_offset("--8000000000000000"), None);
        assert_eq!(parse_offset("-+8"), None);
        assert_eq!(parse_offset("+-8"), None);
        assert_eq!(parse_offset("-8000000000000000"), None);
        assert_eq!(parse_offset("-7FFFFFFFFFFFFFFF"), Some(-i64::MAX));
    }

    #[test]
    fn unknown_fields_are_rejected_inside_tagged_impls() {
        let ok = r#"{"kind":"patch","aob":"90","original":"90","replace":"90"}"#;
        assert!(serde_json::from_str::<Impl>(ok).is_ok());
        let extra = r#"{"kind":"patch","aob":"90","original":"90","replace":"90","x":1}"#;
        assert!(serde_json::from_str::<Impl>(extra).is_err());
        let extra_value =
            r#"{"kind":"value","type":"i32","address":{"base":"a.exe+0x10","bogus":true}}"#;
        assert!(serde_json::from_str::<Impl>(extra_value).is_err());
    }
}
