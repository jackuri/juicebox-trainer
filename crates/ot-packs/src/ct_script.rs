//! Reading Cheat Engine table scripts as text, never running them.
//!
//! Many tables find their addresses with small Auto Assembler / Lua scripts.
//! A few patterns are common and can be redone safely by OpenTrainer's own
//! code, so the values that depend on them can come over:
//!
//! - `aobscanmodule(name, game.exe, 48 8B ...)` + `registersymbol(name)`: the
//!   address where the bytes are found (OpenTrainer's `aob` address).
//! - The Lua idiom `getAddress("name") + 3`, `address + readInteger(address) + 4`,
//!   `registerSymbol("name", address)`: the target of a RIP-relative operand
//!   (OpenTrainer's `aob` address with `operand: rip32`).
//! - Numbers: `define(name, 10)`, or `label(name)` placed at a number
//!   (`1:` then `name:`), used in offsets like `slot*6180+2F90`.
//! - Byte swaps: at a found address, `db 90 90` on enable and `db 75 15` on
//!   disable, with nothing else: OpenTrainer's checked `patch` op.
//!
//! Anything else (allocated memory, injected code, Lua logic) is not
//! understood and stays out. Recognising a pattern only produces pack data;
//! the validator and the trainer runtime check it like any other pack.

use std::collections::HashMap;

/// Where a script-made name points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Sym {
    /// An address in code, found by its bytes. `rip_next` set: the address is
    /// the target of the RIP-relative displacement at `offset`, with the
    /// instruction ending at `rip_next` (both from the match start).
    Aob {
        module: Option<String>,
        aob: String,
        offset: i64,
        rip_next: Option<i64>,
    },
    /// `game.exe+offset`.
    Module { module: String, offset: i64 },
    /// A plain number.
    Const(i64),
}

/// A byte swap at one found address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BytePatch {
    pub module: Option<String>,
    pub aob: String,
    pub offset: i64,
    pub original: String,
    pub replace: String,
}

/// The script's lines, comments removed, split at `[ENABLE]` / `[DISABLE]`.
struct Sections {
    enable: Vec<String>,
    disable: Vec<String>,
    /// The `{$lua}` parts of the enable section, whitespace removed, lowercase.
    lua: String,
    /// Uses Lua at all.
    has_lua: bool,
}

/// Removes `// ...` and `{ ... }` comments (but keeps `{$lua}` / `{$asm}`).
fn strip_comments(script: &str) -> String {
    let mut out = String::with_capacity(script.len());
    let mut chars = script.chars().peekable();
    let mut depth = 0usize;
    while let Some(c) = chars.next() {
        if depth > 0 {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            continue;
        }
        match c {
            '{' if chars.peek() == Some(&'$') => out.push(c),
            '{' => depth = 1,
            '/' if chars.peek() == Some(&'/') => {
                for d in chars.by_ref() {
                    if d == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

fn sections(script: &str) -> Sections {
    let clean = strip_comments(script);
    let mut enable = Vec::new();
    let mut disable = Vec::new();
    let mut lua = String::new();
    let mut has_lua = false;
    let mut in_disable = false;
    let mut in_lua = false;
    for raw in clean.lines() {
        let line = raw.trim();
        let lower = line.to_ascii_lowercase();
        match lower.as_str() {
            "[enable]" => {
                in_disable = false;
                continue;
            }
            "[disable]" => {
                in_disable = true;
                in_lua = false;
                continue;
            }
            "{$lua}" => {
                in_lua = true;
                has_lua = true;
                continue;
            }
            "{$asm}" => {
                in_lua = false;
                continue;
            }
            _ => {}
        }
        if line.is_empty() {
            continue;
        }
        if in_disable {
            disable.push(line.to_string());
        } else {
            if in_lua {
                lua.extend(lower.chars().filter(|c| !c.is_whitespace()));
                lua.push('\n');
            }
            // Auto Assembler inside the Lua `autoAssemble([[ ... ]])` is still
            // read as Auto Assembler lines.
            enable.push(line.to_string());
        }
    }
    Sections {
        enable,
        disable,
        lua,
        has_lua,
    }
}

/// `name(a, b, c)` → the arguments, if the line is that call.
fn call<'a>(line: &'a str, name: &str) -> Option<Vec<&'a str>> {
    let lower = line.to_ascii_lowercase();
    let rest = lower.strip_prefix(name)?.trim_start();
    if !rest.starts_with('(') {
        return None;
    }
    let open = line.len() - rest.len();
    let close = line.rfind(')')?;
    if close <= open {
        return None;
    }
    Some(line[open + 1..close].split(',').map(str::trim).collect())
}

/// A CE byte pattern (`48 8B 0D * * ?? x`) as OpenTrainer's (`48 8B 0D ?? ??`).
fn aob_text(s: &str) -> Option<String> {
    let mut out = Vec::new();
    for b in s.split_whitespace() {
        if b.chars().all(|c| matches!(c, '*' | '?' | 'x' | 'X')) {
            out.push("??".to_string());
        } else if b.len() == 2 && b.chars().all(|c| c.is_ascii_hexdigit()) {
            out.push(b.to_ascii_uppercase());
        } else {
            return None;
        }
    }
    (!out.is_empty() && out.len() <= 256).then(|| out.join(" "))
}

/// `$1000`, `1000`, `0x1000` (Auto Assembler numbers are hex).
fn aa_number(s: &str) -> Option<i64> {
    let t = s.trim().trim_start_matches('$');
    if t.is_empty() || t.len() > 16 {
        return None;
    }
    crate::schema::parse_offset(t)
}

/// A decimal or `0x` hex Lua number.
fn lua_number(s: &str) -> Option<i64> {
    if let Some(h) = s.strip_prefix("0x") {
        i64::from_str_radix(h, 16).ok()
    } else {
        s.parse().ok()
    }
}

/// The names one script defines that OpenTrainer can redo.
pub(crate) fn symbols(script: &str, out: &mut HashMap<String, Sym>) {
    let s = sections(script);
    let mut aobs: HashMap<String, (Option<String>, String)> = HashMap::new();
    let mut registered = Vec::new();
    let mut consts: HashMap<String, i64> = HashMap::new();
    let mut defines: HashMap<String, Sym> = HashMap::new();
    let mut at_number: Option<i64> = None;
    for line in &s.enable {
        // `autoAssemble([[aobscanmodule(...)` puts the first line after `[[`.
        let line = line.rsplit("[[").next().unwrap_or(line).trim();
        if let Some(a) = call(line, "aobscanmodule")
            && a.len() == 3
            && let Some(aob) = aob_text(a[2])
        {
            aobs.insert(
                a[0].to_ascii_lowercase(),
                (Some(a[1].trim_matches('"').to_string()), aob),
            );
        } else if let Some(a) = call(line, "aobscan")
            && a.len() == 2
            && let Some(aob) = aob_text(a[1])
        {
            aobs.insert(a[0].to_ascii_lowercase(), (None, aob));
        } else if let Some(a) = call(line, "registersymbol") {
            registered.extend(a.iter().map(|n| n.to_ascii_lowercase()));
        } else if let Some(a) = call(line, "define")
            && a.len() == 2
        {
            let name = a[0].to_ascii_lowercase();
            let value = a[1].trim_matches('"');
            if let Some(n) = aa_number(value) {
                defines.insert(name, Sym::Const(n));
            } else if let Some((m, off)) = a[1].split_once('+')
                && let Some(off) = aa_number(off)
            {
                let m = m.trim().trim_matches('"');
                let l = m.to_ascii_lowercase();
                if l.ends_with(".exe") || l.ends_with(".dll") {
                    defines.insert(
                        name,
                        Sym::Module {
                            module: m.to_string(),
                            offset: off,
                        },
                    );
                }
            }
        } else if let Some(label) = line.strip_suffix(':') {
            // `1:` then `name:` puts the label at address 1: a number.
            if let Some(n) = aa_number(label) {
                at_number = Some(n);
                continue;
            }
            if let Some(n) = at_number
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                consts.insert(label.to_ascii_lowercase(), n);
            }
        }
        at_number = None;
    }

    for name in registered {
        if let Some(c) = consts.get(&name) {
            out.insert(name, Sym::Const(*c));
        } else if let Some(d) = defines.get(&name) {
            out.insert(name, d.clone());
        } else if let Some((module, aob)) = aobs.get(&name) {
            out.insert(
                name.clone(),
                Sym::Aob {
                    module: module.clone(),
                    aob: aob.clone(),
                    offset: 0,
                    rip_next: None,
                },
            );
        }
    }
    // Defines are visible to the whole table too.
    for (name, d) in defines {
        out.entry(name).or_insert(d);
    }

    if s.has_lua {
        for (name, (module, aob)) in &aobs {
            if let Some(k) = lua_rip(&s.lua, name) {
                out.insert(
                    name.clone(),
                    Sym::Aob {
                        module: module.clone(),
                        aob: aob.clone(),
                        offset: k,
                        rip_next: Some(k + 4),
                    },
                );
            }
        }
    }
}

/// The Lua RIP idiom for `name`: `getAddress("name") + K`, then
/// `x + readInteger(x) + 4`, then `registerSymbol("name", ...)`. Returns K.
fn lua_rip(lua: &str, name: &str) -> Option<i64> {
    let get = lua
        .find(&format!("getaddress(\"{name}\")+"))
        .or_else(|| lua.find(&format!("getaddress('{name}')+")))?;
    let after = &lua[get..];
    let plus = after.find(")+")? + 2;
    let digits: String = after[plus..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    let k = lua_number(&digits)?;
    // The very next statement adds the displacement and the 4 bytes after it.
    let rest = &after[plus + digits.len()..];
    let read = rest.find("+readinteger(")?;
    let tail = &rest[read..];
    let close = tail.find(')')?;
    if !tail[close + 1..].starts_with("+4") {
        return None;
    }
    let later = &rest[read..];
    let registers = [
        format!("registersymbol(\"{name}\""),
        format!("registersymbol('{name}'"),
    ]
    .iter()
    .any(|needle| {
        // `unregistersymbol(...)` contains the same text: not that one.
        later
            .match_indices(needle.as_str())
            .any(|(i, _)| !later[..i].ends_with("un"))
    });
    (registers && (0..=16).contains(&k)).then_some(k)
}

/// `name:` or `name+3:` → (name, 3).
fn site(line: &str) -> Option<(String, i64)> {
    let label = line.strip_suffix(':')?;
    let (name, off) = match label.split_once('+') {
        Some((n, o)) => (n, aa_number(o)?),
        None => (label, 0),
    };
    let name = name.trim();
    (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .then(|| (name.to_ascii_lowercase(), off))
}

/// The bytes a `db` / `nop` line writes.
fn bytes_of(line: &str) -> Option<Vec<String>> {
    let lower = line.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("db ") {
        let out: Option<Vec<String>> = rest
            .split_whitespace()
            .map(|b| {
                (b.len() == 2 && b.chars().all(|c| c.is_ascii_hexdigit()))
                    .then(|| b.to_ascii_uppercase())
            })
            .collect();
        return out.filter(|v| !v.is_empty());
    }
    if lower == "nop" {
        return Some(vec!["90".into()]);
    }
    if let Some(n) = lower.strip_prefix("nop ") {
        let n = aa_number(n)?;
        return (1..=64)
            .contains(&n)
            .then(|| vec!["90".to_string(); n as usize]);
    }
    None
}

/// A script that only swaps bytes at one found address, and puts them back.
pub(crate) fn byte_patch(script: &str) -> Option<BytePatch> {
    let s = sections(script);
    if s.has_lua {
        return None;
    }
    let mut aob: Option<(String, Option<String>, String)> = None;
    let mut writes: Vec<(String, i64, Vec<String>)> = Vec::new();
    for line in &s.enable {
        if let Some(a) = call(line, "aobscanmodule") {
            if aob.is_some() || a.len() != 3 {
                return None;
            }
            aob = Some((
                a[0].to_ascii_lowercase(),
                Some(a[1].trim_matches('"').to_string()),
                aob_text(a[2])?,
            ));
        } else if let Some(a) = call(line, "aobscan") {
            if aob.is_some() || a.len() != 2 {
                return None;
            }
            aob = Some((a[0].to_ascii_lowercase(), None, aob_text(a[1])?));
        } else if call(line, "registersymbol").is_some() {
        } else if let Some((name, off)) = site(line) {
            writes.push((name, off, Vec::new()));
        } else {
            // Anything else (alloc, label, jmp, mov, ...) isn't a plain byte swap.
            let b = bytes_of(line)?;
            writes.last_mut()?.2.extend(b);
        }
    }
    let mut restores: Vec<(String, i64, Vec<String>)> = Vec::new();
    for line in &s.disable {
        if call(line, "unregistersymbol").is_some() {
        } else if let Some((name, off)) = site(line) {
            restores.push((name, off, Vec::new()));
        } else {
            let b = bytes_of(line)?;
            restores.last_mut()?.2.extend(b);
        }
    }
    let (name, module, aob) = aob?;
    let [(wn, wo, replace)] = writes.as_slice() else {
        return None;
    };
    let [(rn, ro, original)] = restores.as_slice() else {
        return None;
    };
    if *wn != name || rn != wn || ro != wo || replace.len() != original.len() || replace.is_empty()
    {
        return None;
    }
    Some(BytePatch {
        module,
        aob,
        offset: *wo,
        original: original.join(" "),
        replace: replace.join(" "),
    })
}

/// The script only names addresses (`registersymbol`) that OpenTrainer can
/// find itself (`known`), and changes nothing in the game.
pub(crate) fn only_registers(script: &str, known: &HashMap<String, Sym>) -> bool {
    let s = sections(script);
    let mut names = Vec::new();
    for line in &s.enable {
        let line = line.rsplit("[[").next().unwrap_or(line).trim();
        if call(line, "alloc").is_some()
            || call(line, "globalalloc").is_some()
            || bytes_of(line).is_some()
        {
            return false;
        }
        if let Some(a) = call(line, "registersymbol") {
            if a.iter().any(|n| n.contains(['"', '\''])) {
                // Lua's registerSymbol("name", address, ...): only the name.
                if let Some(first) = a.first() {
                    names.push(first.trim_matches(['"', '\'']).to_ascii_lowercase());
                }
            } else {
                names.extend(a.iter().map(|n| n.to_ascii_lowercase()));
            }
        }
    }
    !names.is_empty() && names.iter().all(|n| known.contains_key(n))
}

/// The script is for a 64-bit game (uses 64-bit registers).
pub(crate) fn mentions_x64(script: &str) -> bool {
    let lower = strip_comments(script).to_ascii_lowercase();
    lower.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| {
        matches!(
            w,
            "rax"
                | "rbx"
                | "rcx"
                | "rdx"
                | "rsi"
                | "rdi"
                | "rsp"
                | "rbp"
                | "r8"
                | "r9"
                | "r10"
                | "r11"
                | "r12"
                | "r13"
                | "r14"
                | "r15"
                | "qword"
                | "dq"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LUA_RIP: &str = r#"[ENABLE]
{$lua}
if syntaxcheck then return end
autoAssemble([[
aobscanmodule(gamedata,DBXV2.exe,48 8B 0D * * * * E8 * * * * 48 8B C8)
registersymbol(gamedata)
label(gameoffset)
1:
gameoffset:
registersymbol(gameoffset)
]])
local address = getAddress("gamedata") + 3
address = address + readInteger(address) + 4
unregisterSymbol("gamedata")
registerSymbol("gamedata", address, true)
{$asm}
[DISABLE]
unregistersymbol(gamedata)
unregistersymbol(gameoffset)
"#;

    #[test]
    fn reads_rip_symbols_and_label_numbers_without_running_lua() {
        let mut syms = HashMap::new();
        symbols(LUA_RIP, &mut syms);
        assert_eq!(
            syms.get("gamedata"),
            Some(&Sym::Aob {
                module: Some("DBXV2.exe".into()),
                aob: "48 8B 0D ?? ?? ?? ?? E8 ?? ?? ?? ?? 48 8B C8".into(),
                offset: 3,
                rip_next: Some(7),
            })
        );
        assert_eq!(syms.get("gameoffset"), Some(&Sym::Const(1)));
    }

    #[test]
    fn plain_aob_symbols_and_defines() {
        let mut syms = HashMap::new();
        symbols(
            "[ENABLE]\naobscanmodule(inv,Game.exe,44 8B 40 18 45 85 C9) // should be unique\nregistersymbol(inv)\n\
             define(slot,2)\ndefine(base,\"Game.exe\"+1A0)\n[DISABLE]\nunregistersymbol(inv)",
            &mut syms,
        );
        assert!(matches!(
            syms.get("inv"),
            Some(Sym::Aob {
                offset: 0,
                rip_next: None,
                ..
            })
        ));
        assert_eq!(syms.get("slot"), Some(&Sym::Const(2)));
        assert_eq!(
            syms.get("base"),
            Some(&Sym::Module {
                module: "Game.exe".into(),
                offset: 0x1A0
            })
        );
    }

    #[test]
    fn rip_idiom_must_be_complete() {
        let mut syms = HashMap::new();
        // No `+ 4`: not the idiom, so not trusted.
        symbols(
            &LUA_RIP.replace("readInteger(address) + 4", "readInteger(address) + 8"),
            &mut syms,
        );
        assert!(matches!(
            syms.get("gamedata"),
            Some(Sym::Aob { rip_next: None, .. })
        ));
    }

    #[test]
    fn byte_swaps_become_patches_and_nothing_else_does() {
        let swap = "[ENABLE]\naobscanmodule(shenron,DBXV2.exe,75 15 8B D3 48 8B CE)\nshenron:\n  db 90 90\n\
                    registersymbol(shenron)\n[DISABLE]\nshenron:\n  db 75 15\nunregistersymbol(shenron)\n\
                    {\n// ORIGINAL CODE\n\"DBXV2.exe\"+3A5F51: 75 15 - jne\n}";
        assert_eq!(
            byte_patch(swap),
            Some(BytePatch {
                module: Some("DBXV2.exe".into()),
                aob: "75 15 8B D3 48 8B CE".into(),
                offset: 0,
                original: "75 15".into(),
                replace: "90 90".into(),
            })
        );
        let nops = "[ENABLE]\naobscanmodule(x,g.exe,0F 84 BB 00 00 00)\nx+2:\n nop 3\n[DISABLE]\nx+2:\n db BB 00 00\n";
        assert_eq!(byte_patch(nops).unwrap().replace, "90 90 90");
        assert_eq!(byte_patch(nops).unwrap().offset, 2);
        // Injected code, two places, or Lua: not a byte swap.
        assert!(byte_patch("[ENABLE]\naobscanmodule(h,g.exe,0F 57 DB)\nalloc(newmem,$1000,h)\nnewmem:\n jmp return\n[DISABLE]\nh:\n db 0F 57 DB").is_none());
        assert!(byte_patch("[ENABLE]\naobscanmodule(a,g.exe,0F 84)\na:\n db 90 90\naobscanmodule(b,g.exe,0F 85)\nb:\n db 90 90\n[DISABLE]\na:\n db 0F 84\nb:\n db 0F 85").is_none());
        assert!(byte_patch("[ENABLE]\n{$lua}\nprint(1)\n{$asm}\n[DISABLE]\n").is_none());
        // Restoring different bytes than it wrote over is refused too.
        assert!(
            byte_patch(
                "[ENABLE]\naobscanmodule(s,g.exe,75 15 8B)\ns:\n db 90 90\n[DISABLE]\ns:\n db 75\n"
            )
            .is_none()
        );
    }

    #[test]
    fn tells_64_bit_scripts() {
        assert!(mentions_x64("newmem:\n mov rax,player_ptr\n mov [rax],rdx"));
        assert!(!mentions_x64(
            "newmem:\n mov eax,[esi+10] // rax in a comment\n"
        ));
    }
}
