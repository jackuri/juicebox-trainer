//! Pack validation: strict parsing plus checks the type system can't express.
//!
//! [`parse`] is the only way the app reads a pack. It refuses packs with any
//! error-level issue; warnings are shown but don't block loading.

use std::collections::{HashMap, HashSet};
use std::fmt;

use ot_core::{Pattern, ValueType};
use serde::Serialize;
use serde_json::Number;

use crate::hotkey::Hotkey;
use crate::numbers::{Num, in_bounds, is_float, to_value};
use crate::schema::{
    Address, Capture, Cheat, Impl, MAX_SOURCE_BY, MAX_SOURCE_LICENCE, Operand, Pack, PackArch,
    PatchImpl, SAVE_CODECS, SAVE_PLACEHOLDERS, SCHEMA_VERSION, Ui, ValueImpl, is_source_url,
    is_support_url, parse_offset,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// One problem found in a pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub severity: Severity,
    /// JSON path, e.g. `cheats[2].impl.aob`.
    pub path: String,
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{sev}: {}: {}", self.path, self.message)
    }
}

/// Why a pack could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadError {
    /// Not JSON, or the wrong shape (with line and column).
    #[error("{0}")]
    Syntax(String),
    /// Parsed, but has error-level issues.
    #[error("{} error(s): {}", .0.iter().filter(|i| i.severity == Severity::Error).count(),
        .0.iter().filter(|i| i.severity == Severity::Error).map(|i| i.to_string()).collect::<Vec<_>>().join("; "))]
    Invalid(Vec<Issue>),
}

/// Parses and validates pack JSON. On success returns the pack and its
/// warnings.
pub fn parse(text: &str) -> Result<(Pack, Vec<Issue>), LoadError> {
    let raw: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| LoadError::Syntax(format!("not valid JSON: {e}")))?;
    precheck(&raw)?;
    // Parse from the text (not `raw`) so shape errors carry line and column.
    let pack: Pack = serde_json::from_str(text).map_err(|e| LoadError::Syntax(e.to_string()))?;
    let issues = validate(&pack);
    if issues.iter().any(|i| i.severity == Severity::Error) {
        return Err(LoadError::Invalid(issues));
    }
    Ok((pack, issues))
}

/// Friendlier messages for things serde would only call "unknown variant".
fn precheck(raw: &serde_json::Value) -> Result<(), LoadError> {
    let err = |path: &str, message: String| {
        LoadError::Invalid(vec![Issue {
            severity: Severity::Error,
            path: path.into(),
            message,
        }])
    };
    match raw.get("schema").and_then(|s| s.as_u64()) {
        Some(v) if v == u64::from(SCHEMA_VERSION) => {}
        Some(v) => {
            return Err(err(
                "schema",
                format!(
                    "schema {v} is not supported by this version (it reads schema {SCHEMA_VERSION})"
                ),
            ));
        }
        None => return Err(err("schema", "missing \"schema\": 1".into())),
    }
    if let Some(cheats) = raw.get("cheats").and_then(|c| c.as_array()) {
        for (i, c) in cheats.iter().enumerate() {
            if c.pointer("/impl/kind").and_then(|k| k.as_str()) == Some("cave") {
                return Err(err(
                    &format!("cheats[{i}].impl.kind"),
                    "code caves are not supported yet (planned for a later version); use value or patch".into(),
                ));
            }
        }
    }
    Ok(())
}

struct Ctx {
    issues: Vec<Issue>,
}

impl Ctx {
    fn error(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.issues.push(Issue {
            severity: Severity::Error,
            path: path.into(),
            message: message.into(),
        });
    }

    fn warn(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.issues.push(Issue {
            severity: Severity::Warning,
            path: path.into(),
            message: message.into(),
        });
    }
}

pub(crate) fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        && s.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
}

fn is_module_name(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    !s.contains(['/', '\\', ':']) && (l.ends_with(".exe") || l.ends_with(".dll"))
}

fn is_semver(s: &str) -> bool {
    let core = s.split(['-', '+']).next().unwrap_or("");
    let parts: Vec<_> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.parse::<u64>().is_ok())
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// Checks a parsed pack. Returns every issue found (errors and warnings).
pub fn validate(pack: &Pack) -> Vec<Issue> {
    let mut cx = Ctx { issues: Vec::new() };
    if pack.schema != SCHEMA_VERSION {
        cx.error("schema", format!("must be {SCHEMA_VERSION}"));
    }

    let g = &pack.game;
    if !is_slug(&g.id) {
        cx.error(
            "game.id",
            "use lowercase letters, digits, '-' and '_' (max 64)",
        );
    }
    if g.name.trim().is_empty() {
        cx.error("game.name", "must not be empty");
    }
    if !g.exe.to_ascii_lowercase().ends_with(".exe") || g.exe.contains(['/', '\\', ':']) {
        cx.error(
            "game.exe",
            "must be an exe file name like game.exe (no folders)",
        );
    }
    let mut hashes = HashSet::new();
    for (i, b) in g.builds.iter().enumerate() {
        let p = format!("game.builds[{i}].sha256");
        if b.sha256.len() != 64 || !b.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            cx.error(&p, "must be 64 hex characters (SHA-256 of the exe)");
        } else if b.sha256.chars().any(|c| c.is_ascii_uppercase()) {
            cx.error(&p, "use lowercase hex");
        }
        if !hashes.insert(b.sha256.to_ascii_lowercase()) {
            cx.error(&p, "duplicate build");
        }
    }
    if g.builds.is_empty() {
        cx.warn(
            "game.builds",
            "no builds listed: cheats can't be checked against the game version",
        );
    }

    let m = &pack.meta;
    if m.author.trim().is_empty() {
        cx.error("meta.author", "must not be empty");
    }
    if !is_semver(&m.pack_version) {
        cx.error("meta.packVersion", "must be SemVer, e.g. 1.0.0");
    }
    if let Some(d) = &m.created
        && !is_date(d)
    {
        cx.error("meta.created", "must be YYYY-MM-DD");
    }
    if let Some(src) = &m.source {
        if let Some(by) = &src.by
            && (by.trim().is_empty()
                || by.chars().count() > MAX_SOURCE_BY
                || by.chars().any(char::is_control))
        {
            cx.error(
                "meta.source.by",
                format!("must be 1 to {MAX_SOURCE_BY} characters on one line"),
            );
        }
        if let Some(url) = &src.url
            && !is_source_url(url)
        {
            cx.error(
                "meta.source.url",
                "must be a web address starting with https:// (or http://)",
            );
        }
        if let Some(file) = &src.file
            && (file.trim().is_empty()
                || file.chars().count() > 200
                || file
                    .chars()
                    .any(|c| c.is_control() || c == '/' || c == '\\'))
        {
            cx.error("meta.source.file", "must be a file name");
        }
        if let Some(licence) = &src.licence
            && (licence.trim().is_empty()
                || licence.chars().count() > MAX_SOURCE_LICENCE
                || licence.chars().any(char::is_control))
        {
            cx.error(
                "meta.source.licence",
                format!("must be 1 to {MAX_SOURCE_LICENCE} characters on one line"),
            );
        }
        if let Some(support) = &src.support
            && !is_support_url(support)
        {
            cx.error(
                "meta.source.support",
                "must be an https page on Patreon, Ko-fi, Buy Me a Coffee, GitHub, YouTube or a table site",
            );
        }
    }

    if pack.cheats.is_empty() {
        cx.warn("cheats", "the pack has no cheats");
    }
    let mut ids = HashSet::new();
    let mut hotkeys: HashMap<String, String> = HashMap::new();
    for (i, c) in pack.cheats.iter().enumerate() {
        let p = format!("cheats[{i}]");
        if !is_slug(&c.id) {
            cx.error(
                format!("{p}.id"),
                "use lowercase letters, digits, '-' and '_' (max 64)",
            );
        } else if !ids.insert(c.id.clone()) {
            cx.error(format!("{p}.id"), format!("duplicate cheat id '{}'", c.id));
        }
        if c.name.trim().is_empty() {
            cx.error(format!("{p}.name"), "must not be empty");
        }
        check_hotkey(&mut cx, &p, c, &mut hotkeys);
        check_ui(&mut cx, &p, c);
        match &c.implementation {
            Impl::Value(v) => {
                check_value(&mut cx, &p, c, v);
                if let Some(cap) = &v.address.capture {
                    check_capture(
                        &mut cx,
                        &format!("{p}.impl.address"),
                        &v.address,
                        cap,
                        g.arch,
                    );
                }
            }
            Impl::Patch(pt) => check_patch(&mut cx, &p, c, pt, g.arch),
        }
    }

    let mut save_ids = HashSet::new();
    for (i, s) in pack.save_codes.iter().enumerate() {
        let p = format!("saveCodes[{i}]");
        if !is_slug(&s.id) {
            cx.error(
                format!("{p}.id"),
                "use lowercase letters, digits, '-' and '_' (max 64)",
            );
        } else if !save_ids.insert(s.id.clone()) {
            cx.error(
                format!("{p}.id"),
                format!("duplicate save code id '{}'", s.id),
            );
        }
        if s.name.trim().is_empty() {
            cx.error(format!("{p}.name"), "must not be empty");
        }
        if !SAVE_CODECS.contains(&s.codec.as_str()) {
            cx.error(
                format!("{p}.codec"),
                format!("use one of: {}", SAVE_CODECS.join(", ")),
            );
        }
        check_save_file(&mut cx, &format!("{p}.file"), &s.file, &s.codec);
        if s.set.path.trim().is_empty() {
            cx.error(format!("{p}.set.path"), "must not be empty");
        }
        if !(s.set.value.is_number() || s.set.value.is_string() || s.set.value.is_boolean()) {
            cx.error(
                format!("{p}.set.value"),
                "must be a number, text or true/false",
            );
        }
    }
    cx.issues
}

/// `{localappdatalow}/Company/Game/save1.json`: a known folder placeholder,
/// then a relative path with no `..`.
fn check_save_file(cx: &mut Ctx, p: &str, file: &str, codec: &str) {
    let Some(end) = file.find('}') else {
        cx.error(
            p,
            format!(
                "start with a folder placeholder: {}",
                SAVE_PLACEHOLDERS.join(", ")
            ),
        );
        return;
    };
    let (head, rest) = file.split_at(end + 1);
    if !SAVE_PLACEHOLDERS.contains(&head) {
        cx.error(
            p,
            format!(
                "unknown folder {head}; use one of: {}",
                SAVE_PLACEHOLDERS.join(", ")
            ),
        );
        return;
    }
    let parts: Vec<&str> = rest.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    if parts.is_empty()
        || parts
            .iter()
            .any(|s| *s == ".." || *s == "." || s.contains(':'))
    {
        cx.error(p, "give a path inside the folder, without '..'");
    }
    if (head == "{playerprefs}") != (codec == "playerprefs") && codec != "auto" {
        cx.error(
            p,
            "use {playerprefs}/Company/Product with codec playerprefs (and only then)",
        );
    }
}

fn check_hotkey(cx: &mut Ctx, p: &str, c: &Cheat, seen: &mut HashMap<String, String>) {
    let Some(h) = &c.hotkey else { return };
    let path = format!("{p}.hotkey");
    if !matches!(c.ui, Ui::Toggle | Ui::Button) {
        cx.error(&path, "only toggles and buttons can have a hotkey");
        return;
    }
    match Hotkey::parse(h) {
        Ok(hk) => {
            let canon = hk.to_string();
            if let Some(other) = seen.get(&canon) {
                cx.error(&path, format!("{canon} is already used by '{other}'"));
            } else {
                seen.insert(canon, c.id.clone());
            }
        }
        Err(e) => cx.error(&path, e),
    }
}

fn check_ui(cx: &mut Ctx, p: &str, c: &Cheat) {
    let present = [
        ("toggle", c.toggle.is_some(), Ui::Toggle),
        ("slider", c.slider.is_some(), Ui::Slider),
        ("number", c.number.is_some(), Ui::Number),
        ("button", c.button.is_some(), Ui::Button),
    ];
    for (name, is_set, ui) in present {
        if is_set && ui != c.ui {
            cx.error(
                format!("{p}.{name}"),
                format!("'{name}' settings need \"ui\": \"{name}\""),
            );
        }
    }
    if c.ui == Ui::Slider && c.slider.is_none() {
        cx.error(format!("{p}.slider"), "a slider needs min, max and step");
    }
    if c.ui == Ui::Button && c.button.is_none() {
        cx.error(format!("{p}.button"), "a button needs \"add\" or \"set\"");
    }
    if let Some(b) = &c.button
        && b.add.is_some() == b.set.is_some()
    {
        cx.error(
            format!("{p}.button"),
            "use exactly one of \"add\" or \"set\"",
        );
    }
    if matches!(c.implementation, Impl::Patch(_)) && c.ui != Ui::Toggle {
        cx.error(
            format!("{p}.ui"),
            "patch cheats must be toggles (on = patched, off = restored)",
        );
    }
    if matches!(c.implementation, Impl::Patch(_)) && c.toggle.is_some() {
        cx.error(format!("{p}.toggle"), "a patch toggle takes no settings");
    }
}

/// Checks a pack number fits the value type; returns it if so.
fn fits(cx: &mut Ctx, path: &str, ty: ValueType, n: &Number) -> Option<Num> {
    let num = Num::from_json(n);
    if to_value(ty, num).is_none() {
        let what = if is_float(ty) { "a finite" } else { "a whole" };
        cx.error(path, format!("{n} is not {what} {ty} value"));
        return None;
    }
    Some(num)
}

fn check_value(cx: &mut Ctx, p: &str, c: &Cheat, v: &ValueImpl) {
    let ty = v.ty;
    if ty == ValueType::Ptr {
        cx.error(
            format!("{p}.impl.type"),
            "ptr is not a cheat value type; use an integer type",
        );
        return;
    }
    check_address(cx, &format!("{p}.impl.address"), &v.address);

    if let Some(t) = &c.toggle
        && let Some(n) = &t.value
    {
        fits(cx, &format!("{p}.toggle.value"), ty, n);
    }
    if let Some(b) = &c.button {
        if let Some(n) = &b.set {
            fits(cx, &format!("{p}.button.set"), ty, n);
        }
        if let Some(n) = &b.add
            && !is_float(ty)
            && Num::from_json(n).as_int().is_none()
        {
            cx.error(
                format!("{p}.button.add"),
                format!("{n} is not a whole number for {ty}"),
            );
        }
    }
    if let Some(n) = &c.number {
        let min = n
            .min
            .as_ref()
            .and_then(|m| fits(cx, &format!("{p}.number.min"), ty, m));
        let max = n
            .max
            .as_ref()
            .and_then(|m| fits(cx, &format!("{p}.number.max"), ty, m));
        if let (Some(a), Some(b)) = (min, max)
            && a.cmp_num(b) != Some(std::cmp::Ordering::Less)
        {
            cx.error(format!("{p}.number"), "min must be less than max");
        }
    }
    if let Some(s) = &c.slider {
        let sp = format!("{p}.slider");
        let min = fits(cx, &format!("{sp}.min"), ty, &s.min);
        let max = fits(cx, &format!("{sp}.max"), ty, &s.max);
        let step = Num::from_json(&s.step);
        if step.as_f64().partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            cx.error(format!("{sp}.step"), "must be greater than 0");
        } else if !is_float(ty) && step.as_int().is_none() {
            cx.error(
                format!("{sp}.step"),
                format!("must be a whole number for {ty}"),
            );
        }
        if let (Some(a), Some(b)) = (min, max) {
            if a.cmp_num(b) != Some(std::cmp::Ordering::Less) {
                cx.error(&sp, "min must be less than max");
            }
            if let Some(d) = &s.default
                && let Some(dv) = fits(cx, &format!("{sp}.default"), ty, d)
                && !in_bounds(dv, Some(a), Some(b))
            {
                cx.error(format!("{sp}.default"), "must be between min and max");
            }
        }
    }
}

fn check_address(cx: &mut Ctx, p: &str, a: &Address) {
    match (&a.base, &a.aob) {
        (Some(_), Some(_)) => cx.error(p, "use either \"base\" or \"aob\", not both"),
        (None, None) => cx.error(p, "needs \"base\" (module.exe+0x…) or \"aob\""),
        (Some(base), None) => {
            for (field, set) in [
                ("module", a.module.is_some()),
                ("offset", a.offset.is_some()),
                ("operand", a.operand.is_some()),
                ("next", a.next.is_some()),
            ] {
                if set {
                    cx.error(format!("{p}.{field}"), "only used with \"aob\"");
                }
            }
            match ot_core::address::parse(base) {
                Ok(ot_core::address::AddressExpr::Absolute(_)) => cx.warn(
                    format!("{p}.base"),
                    "an absolute address usually changes every time the game starts; use module.exe+0x…",
                ),
                Ok(_) => {}
                Err(_) => cx.error(format!("{p}.base"), "use module.exe+0x… (hex offset)"),
            }
        }
        (None, Some(aob)) => {
            check_aob(cx, &format!("{p}.aob"), aob);
            if let Some(m) = &a.module
                && !is_module_name(m)
            {
                cx.error(
                    format!("{p}.module"),
                    "must be a module file name like game.exe or engine.dll",
                );
            }
            match (a.operand, a.next) {
                (Some(Operand::Rip32), None) => {
                    cx.error(
                        format!("{p}.next"),
                        "rip32 needs \"next\": the instruction end, from the match start",
                    );
                }
                (Some(Operand::Rip32), Some(n)) if n <= a.offset.unwrap_or(0) + 3 => {
                    cx.error(
                        format!("{p}.next"),
                        "must be after the 4 displacement bytes",
                    );
                }
                (Some(Operand::Abs32) | None, Some(_)) => {
                    cx.error(format!("{p}.next"), "only used with \"operand\": \"rip32\"");
                }
                _ => {}
            }
        }
    }
    for (i, o) in a.offsets.iter().enumerate() {
        if parse_offset(o).is_none() {
            cx.error(
                format!("{p}.offsets[{i}]"),
                "must be a hex offset like \"0x18\"",
            );
        }
    }
}

/// A register read when game code runs: needs the code (aob), a register the
/// game's architecture has, and no operand (the register replaces it).
fn check_capture(cx: &mut Ctx, p: &str, a: &Address, cap: &Capture, arch: Option<PackArch>) {
    if a.aob.is_none() {
        cx.error(
            format!("{p}.capture"),
            "needs \"aob\": the game code where the register is read",
        );
    }
    if a.operand.is_some() {
        cx.error(
            format!("{p}.operand"),
            "not used with \"capture\" (the register's value is the start point)",
        );
    }
    if let Some(add) = &cap.add
        && parse_offset(add).is_none()
    {
        cx.error(
            format!("{p}.capture.add"),
            "must be a hex offset like \"0x18\"",
        );
    }
    let Some(arch) = arch else {
        cx.error(
            "game.arch",
            "packs that capture a register must say \"arch\": \"x86\" or \"x64\"",
        );
        return;
    };
    let reg_ok = |name: &str| ot_core::capture::Register::parse(name, arch.core()).is_some();
    if !reg_ok(&cap.register) {
        cx.error(
            format!("{p}.capture.register"),
            format!(
                "not a register of {} games (e.g. rdx, rsi, r12; edx)",
                arch.core()
            ),
        );
    }
    if let Some(w) = &cap.when
        && !reg_ok(&w.register)
    {
        cx.error(
            format!("{p}.capture.when.register"),
            format!("not a register of {} games", arch.core()),
        );
    }
}

fn check_aob(cx: &mut Ctx, path: &str, aob: &str) -> Option<Pattern> {
    match Pattern::parse(aob) {
        Ok(pat) => {
            if pat.fixed_len() < 4 {
                cx.warn(
                    path,
                    "fewer than 4 fixed bytes; the signature is unlikely to be unique",
                );
            }
            Some(pat)
        }
        Err(e) => {
            cx.error(path, e.to_string());
            None
        }
    }
}

fn check_patch(cx: &mut Ctx, p: &str, _c: &Cheat, pt: &PatchImpl, arch: Option<PackArch>) {
    let ip = format!("{p}.impl");
    if let Some(m) = &pt.module
        && !is_module_name(m)
    {
        cx.error(
            format!("{ip}.module"),
            "must be a module file name like game.exe or engine.dll",
        );
    }
    let pattern = check_aob(cx, &format!("{ip}.aob"), &pt.aob);
    let original = Pattern::parse_exact(&pt.original)
        .map_err(|e| cx.error(format!("{ip}.original"), e.to_string()))
        .ok();
    let replace = Pattern::parse_exact(&pt.replace)
        .map_err(|e| cx.error(format!("{ip}.replace"), e.to_string()))
        .ok();
    let (Some(original), Some(replace)) = (original, replace) else {
        return;
    };
    if original.len() != replace.len() {
        cx.error(
            format!("{ip}.replace"),
            format!(
                "{} bytes, but original has {}",
                replace.len(),
                original.len()
            ),
        );
        return;
    }
    if original == replace {
        cx.warn(
            format!("{ip}.replace"),
            "same as original: the patch changes nothing",
        );
    }
    if let Some(pat) = pattern {
        let start = pt.offset;
        let end = start + original.len() as i64;
        if start >= 0 && end <= pat.len() as i64 {
            // The pattern covers the site: it must not pin bytes that differ
            // from `original`, and should wildcard bytes the patch changes so
            // a patched game is still found (and can be restored).
            let mut pins_changed_byte = false;
            for (k, (o, r)) in original.iter().zip(&replace).enumerate() {
                let at = start as usize + k;
                if let Some(b) = pat.bytes()[at] {
                    if b != *o {
                        cx.error(
                            format!("{ip}.original"),
                            format!("byte {k} doesn't match the aob at {at}"),
                        );
                        return;
                    }
                    pins_changed_byte |= o != r;
                }
            }
            if pins_changed_byte {
                cx.warn(
                    format!("{ip}.aob"),
                    "wildcard (??) the bytes the patch changes, so the site is still found while patched",
                );
            }
        }
    }
    let arch = match arch {
        Some(a) => a.core(),
        None => {
            cx.error(
                "game.arch",
                "packs with patch cheats must say \"arch\": \"x86\" or \"x64\"",
            );
            return;
        }
    };
    // Instruction classes only here; jump targets are checked against the
    // real module when the patch is applied.
    if let Err(e) = ot_core::code::check_patch(&replace, arch, 0x1000_0000, 0..u64::MAX) {
        cx.error(format!("{ip}.replace"), e.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"{
      "schema": 1,
      "game": { "id": "testbed-native", "name": "Testbed", "exe": "testbed_native.exe", "arch": "x64",
                "builds": [{ "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" }] },
      "meta": { "author": "ai", "created": "2026-11-06", "packVersion": "1.0.0" },
      "cheats": [
        { "id": "inf_health", "name": "Infinite Health", "ui": "toggle", "hotkey": "F1",
          "impl": { "kind": "patch", "aob": "0F 1F 80 54 4F 54 01 8B 01 ?? ?? 89 01", "offset": 9,
                    "original": "29 D0", "replace": "90 90" } },
        { "id": "gold", "name": "Gold", "ui": "number",
          "impl": { "kind": "value", "type": "i32",
                    "address": { "base": "testbed_native.exe+0x2A1F0", "offsets": ["0x10", "0xC"] } } }
      ]
    }"#;

    fn with(edit: impl FnOnce(&mut serde_json::Value)) -> String {
        let mut v: serde_json::Value = serde_json::from_str(BASE).unwrap();
        edit(&mut v);
        v.to_string()
    }

    fn errors(text: &str) -> Vec<String> {
        match parse(text) {
            Ok(_) => vec![],
            Err(LoadError::Invalid(issues)) => issues
                .into_iter()
                .filter(|i| i.severity == Severity::Error)
                .map(|i| format!("{}: {}", i.path, i.message))
                .collect(),
            Err(LoadError::Syntax(s)) => vec![format!("syntax: {s}")],
        }
    }

    fn has_error(text: &str, needle: &str) {
        let errs = errors(text);
        assert!(
            errs.iter().any(|e| e.contains(needle)),
            "expected '{needle}' in {errs:#?}"
        );
    }

    #[test]
    fn base_pack_is_valid_without_warnings() {
        let (pack, warnings) = parse(BASE).unwrap();
        assert_eq!(pack.cheats.len(), 2);
        assert!(warnings.is_empty(), "{warnings:#?}");
        assert!(pack.meta.is_ai());
    }

    #[test]
    fn syntax_errors_and_unknown_fields() {
        assert!(matches!(parse("{"), Err(LoadError::Syntax(_))));
        has_error(&with(|v| v["game"]["extra"] = 1.into()), "unknown field");
        has_error(
            &with(|v| v["cheats"][0]["impl"]["kind"] = "asm".into()),
            "unknown variant",
        );
    }

    #[test]
    fn schema_version_and_cave_have_clear_messages() {
        has_error(
            &with(|v| v["schema"] = 2.into()),
            "schema 2 is not supported",
        );
        has_error(
            &with(|v| {
                v.as_object_mut()
                    .unwrap()
                    .remove("schema")
                    .map(|_| ())
                    .unwrap()
            }),
            "missing",
        );
        has_error(
            &with(|v| v["cheats"][0]["impl"]["kind"] = "cave".into()),
            "not supported yet",
        );
    }

    #[test]
    fn game_and_meta_fields() {
        has_error(&with(|v| v["game"]["id"] = "Bad Id".into()), "game.id");
        has_error(
            &with(|v| v["game"]["exe"] = r"C:\x\game.exe".into()),
            "game.exe",
        );
        has_error(
            &with(|v| v["game"]["builds"][0]["sha256"] = "abc".into()),
            "64 hex",
        );
        has_error(&with(|v| v["meta"]["packVersion"] = "1.0".into()), "SemVer");
        has_error(
            &with(|v| v["meta"]["created"] = "06/11/2026".into()),
            "YYYY-MM-DD",
        );
        has_error(&with(|v| v["meta"]["author"] = " ".into()), "meta.author");
    }

    #[test]
    fn source_credit() {
        let good = with(|v| {
            v["meta"]["source"] = serde_json::json!({
                "by": "SomeTableMaker",
                "url": "https://fearlessrevolution.com/viewtopic.php?t=1",
                "file": "Game.CT"
            })
        });
        let (pack, issues) = parse(&good).expect("valid");
        assert!(issues.iter().all(|i| i.severity != Severity::Error));
        assert_eq!(
            pack.meta.source.and_then(|s| s.by).as_deref(),
            Some("SomeTableMaker")
        );
        for url in [
            "javascript:alert(1)",
            "file:///C:/x",
            "https://",
            "https://a b",
        ] {
            has_error(
                &with(|v| v["meta"]["source"] = serde_json::json!({ "url": url })),
                "meta.source.url",
            );
        }
        has_error(
            &with(|v| v["meta"]["source"] = serde_json::json!({ "by": "a\nb" })),
            "meta.source.by",
        );
        has_error(
            &with(|v| v["meta"]["source"] = serde_json::json!({ "file": "..\\x.CT" })),
            "meta.source.file",
        );
        assert!(
            parse(&with(
                |v| v["meta"]["source"] = serde_json::json!({ "who": "x" })
            ))
            .is_err()
        );
    }

    #[test]
    fn maker_terms_and_support_page() {
        let good = with(|v| {
            v["meta"]["source"] = serde_json::json!({
                "by": "TableMaker",
                "licence": "Shared with permission; all rights kept by the maker",
                "support": "https://www.patreon.com/tablemaker",
                "fromMaker": true
            })
        });
        let (pack, _) = parse(&good).expect("valid");
        let src = pack.meta.source.unwrap();
        assert!(src.from_maker);
        assert_eq!(
            src.support.as_deref(),
            Some("https://www.patreon.com/tablemaker")
        );
        for bad in [
            "https://evil.example/pay",
            "http://patreon.com/x",
            "https://patreon.com.evil.example/x",
            "https://user@patreon.com/x",
        ] {
            has_error(
                &with(|v| v["meta"]["source"] = serde_json::json!({ "support": bad })),
                "meta.source.support",
            );
        }
        has_error(
            &with(|v| v["meta"]["source"] = serde_json::json!({ "licence": "a\nb" })),
            "meta.source.licence",
        );
        // fromMaker false isn't written back.
        let text =
            with(|v| v["meta"]["source"] = serde_json::json!({ "by": "x", "fromMaker": false }));
        let (pack, _) = parse(&text).unwrap();
        assert!(!serde_json::to_string(&pack).unwrap().contains("fromMaker"));
    }

    #[test]
    fn duplicate_ids_and_hotkey_conflicts() {
        has_error(
            &with(|v| v["cheats"][1]["id"] = "inf_health".into()),
            "duplicate cheat id",
        );
        let text = with(|v| {
            v["cheats"][1]["ui"] = "button".into();
            v["cheats"][1]["button"] = serde_json::json!({ "add": 1000 });
            v["cheats"][1]["hotkey"] = "f1".into();
        });
        has_error(&text, "F1 is already used by 'inf_health'");
        has_error(
            &with(|v| v["cheats"][0]["hotkey"] = "A".into()),
            "need Ctrl or Alt",
        );
        has_error(
            &with(|v| v["cheats"][1]["hotkey"] = "F2".into()),
            "only toggles and buttons",
        );
    }

    #[test]
    fn ui_settings_must_match() {
        has_error(
            &with(|v| v["cheats"][1]["slider"] = serde_json::json!({"min":0,"max":1,"step":1})),
            "need \"ui\": \"slider\"",
        );
        has_error(
            &with(|v| v["cheats"][1]["ui"] = "slider".into()),
            "needs min, max and step",
        );
        has_error(
            &with(|v| v["cheats"][1]["ui"] = "button".into()),
            "needs \"add\" or \"set\"",
        );
        has_error(
            &with(|v| {
                v["cheats"][1]["ui"] = "button".into();
                v["cheats"][1]["button"] = serde_json::json!({ "add": 1, "set": 2 });
            }),
            "exactly one",
        );
        has_error(
            &with(|v| v["cheats"][0]["ui"] = "button".into()),
            "patch cheats must be toggles",
        );
    }

    #[test]
    fn values_must_fit_their_type() {
        let slider = |s: serde_json::Value| {
            with(move |v| {
                v["cheats"][1]["ui"] = "slider".into();
                v["cheats"][1]["slider"] = s;
            })
        };
        has_error(
            &slider(serde_json::json!({"min":0,"max":10,"step":0.5})),
            "whole number",
        );
        has_error(
            &slider(serde_json::json!({"min":10,"max":0,"step":1})),
            "min must be less than max",
        );
        has_error(
            &slider(serde_json::json!({"min":0,"max":10,"step":0})),
            "greater than 0",
        );
        has_error(
            &slider(serde_json::json!({"min":0,"max":10,"step":1,"default":11})),
            "between min and max",
        );
        has_error(
            &slider(serde_json::json!({"min":0,"max":3000000000u64,"step":1})),
            "whole i32",
        );
        has_error(
            &with(|v| {
                v["cheats"][1]["ui"] = "toggle".into();
                v["cheats"][1]["toggle"] = serde_json::json!({ "value": 1.5 });
            }),
            "toggle.value",
        );
        has_error(
            &with(|v| v["cheats"][1]["impl"]["type"] = "ptr".into()),
            "ptr is not",
        );
    }

    #[test]
    fn addresses() {
        let addr = |a: serde_json::Value| with(move |v| v["cheats"][1]["impl"]["address"] = a);
        has_error(&addr(serde_json::json!({})), "needs \"base\"");
        has_error(
            &addr(serde_json::json!({"base":"a.exe+0x1","aob":"90 90 90 90"})),
            "not both",
        );
        has_error(
            &addr(serde_json::json!({"base":"a.exe+0x1","operand":"rip32"})),
            "only used with \"aob\"",
        );
        has_error(&addr(serde_json::json!({"base":"+0x1"})), "module.exe+0x");
        has_error(
            &addr(serde_json::json!({"aob":"48 8B 0D ?? ?? ?? ??","operand":"rip32"})),
            "needs \"next\"",
        );
        has_error(
            &addr(
                serde_json::json!({"aob":"48 8B 0D ?? ?? ?? ??","offset":3,"operand":"rip32","next":5}),
            ),
            "after the 4",
        );
        has_error(
            &addr(serde_json::json!({"aob":"8B 0D ?? ?? ?? ??","operand":"abs32","next":6})),
            "only used with",
        );
        has_error(&addr(serde_json::json!({"aob":"zz"})), "not a hex byte");
        has_error(
            &addr(serde_json::json!({"aob":"48 8B 0D 00","module":"C:\\x.dll"})),
            "module file name",
        );
        has_error(
            &addr(serde_json::json!({"base":"a.exe+0x1","offsets":["0xZZ"]})),
            "offsets[0]",
        );
        // Valid AOB forms.
        let ok = addr(
            serde_json::json!({"aob":"48 8B 0D ?? ?? ?? ??","offset":3,"operand":"rip32","next":7,"offsets":["0x10"]}),
        );
        assert!(errors(&ok).is_empty(), "{:?}", errors(&ok));
        let ok = addr(serde_json::json!({"aob":"8B 0D ?? ?? ?? ??","offset":2,"operand":"abs32"}));
        assert!(errors(&ok).is_empty(), "{:?}", errors(&ok));
    }

    #[test]
    fn absolute_base_is_a_warning() {
        let text = with(|v| v["cheats"][1]["impl"]["address"]["base"] = "0x7FF600001000".into());
        let (_, warnings) = parse(&text).unwrap();
        assert!(warnings.iter().any(|w| w.message.contains("absolute")));
    }

    #[test]
    fn patches() {
        let set = |field: &'static str, val: serde_json::Value| {
            with(move |v| v["cheats"][0]["impl"][field] = val)
        };
        has_error(&set("replace", "90".into()), "1 bytes, but original has 2");
        has_error(&set("replace", "E8 00".into()), "patch rejected");
        has_error(&set("replace", "0F 05".into()), "system calls");
        has_error(&set("original", "90 ??".into()), "no ?? allowed");
        has_error(
            &with(|v| {
                v["cheats"][0]["impl"]["aob"] = "0F 1F 80 54 4F 54 01 8B 01 29 D0 89 01".into();
                v["cheats"][0]["impl"]["original"] = "29 D1".into();
            }),
            "doesn't match the aob",
        );
        has_error(
            &with(|v| {
                v["game"]
                    .as_object_mut()
                    .unwrap()
                    .remove("arch")
                    .map(|_| ())
                    .unwrap()
            }),
            "must say \"arch\"",
        );
        // Pinning the patched bytes in the aob is only a warning.
        let pinned = set("aob", "0F 1F 80 54 4F 54 01 8B 01 29 D0 89 01".into());
        let (_, warnings) = parse(&pinned).unwrap();
        assert!(
            warnings.iter().any(|w| w.message.contains("wildcard")),
            "{warnings:?}"
        );
    }

    #[test]
    fn save_codes_are_validated() {
        let code = |codec: &str, file: &str, value: serde_json::Value| {
            with(|v| {
                v["saveCodes"] = serde_json::json!([{ "id": "max_money", "name": "Max money",
                  "codec": codec, "file": file, "set": { "path": "player.money", "value": value } }]);
            })
        };
        let (pack, warnings) = parse(&code(
            "json",
            "{localappdatalow}/TestCo/Testbed/save1.json",
            serde_json::json!(9_999_999),
        ))
        .unwrap();
        assert_eq!(pack.save_codes.len(), 1);
        assert!(!warnings.iter().any(|w| w.path.starts_with("saveCodes")));
        assert!(
            parse(&code(
                "playerprefs",
                "{playerprefs}/TestCo/Testbed",
                serde_json::json!("x")
            ))
            .is_ok()
        );
        has_error(
            &with(
                |v| v["saveCodes"] = serde_json::json!([{ "id": "x", "name": "", "codec": "json", "file": "{documents}/a.json", "set": { "path": "a", "value": 1 } }]),
            ),
            "saveCodes[0].name",
        );
        let n = serde_json::json!(1);
        has_error(
            &code("sqlite3", "{documents}/a.db", n.clone()),
            "saveCodes[0].codec",
        );
        has_error(
            &code("json", "C:/Windows/a.json", n.clone()),
            "saveCodes[0].file",
        );
        has_error(
            &code("json", "{documents}/../a.json", n.clone()),
            "saveCodes[0].file",
        );
        has_error(
            &code("json", "{nope}/a.json", n.clone()),
            "saveCodes[0].file",
        );
        has_error(
            &code("json", "{playerprefs}/A/B", n.clone()),
            "saveCodes[0].file",
        );
        has_error(
            &code("json", "{documents}/a.json", serde_json::json!([1])),
            "saveCodes[0].set.value",
        );
    }

    #[test]
    fn round_trips_through_serde() {
        let (pack, _) = parse(BASE).unwrap();
        let again: Pack = serde_json::from_str(&serde_json::to_string(&pack).unwrap()).unwrap();
        assert_eq!(pack, again);
    }
}
