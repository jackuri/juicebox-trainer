//! "My cheats": cheats the player saves from Find a value, kept in their own
//! pack for each game (`%APPDATA%\JuiceBox Trainer\packs\mine-<exe>\pack.json`).
//! Only value cheats at a lasting address (a module address or a pointer
//! path from one) are written; the result is checked like any other pack.

use serde::Deserialize;
use serde_json::{Value as Json, json};

use jb_core::{Arch, ValueType};

/// Hotkeys handed out in order to new cheats.
const HOTKEYS: [&str; 12] = [
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
];

/// A cheat as the Find a value helper sends it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCheat {
    /// What the player called it ("Gold").
    pub name: String,
    pub ty: ValueType,
    /// The number to keep it at, or to set it to.
    pub value: String,
    /// Keep it there (a switch) rather than set it once (a button).
    pub keep: bool,
    /// Where it lives: `module+module_offset`, then a pointer read and an
    /// offset for each of `offsets`.
    pub module: String,
    pub module_offset: u64,
    #[serde(default)]
    pub offsets: Vec<i64>,
}

/// The attached game.
#[derive(Debug, Clone)]
pub struct GameInfo {
    pub name: String,
    pub exe: String,
    pub arch: Arch,
    pub sha256: Option<String>,
}

/// The pack id for a game's own cheats, from its exe name.
pub fn pack_id(exe: &str) -> String {
    let stem = exe
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(exe)
        .trim_end_matches(".exe")
        .trim_end_matches(".EXE");
    let mut slug = slugify(stem, '-');
    slug.truncate(59);
    format!("mine-{}", slug.trim_end_matches('-'))
}

fn slugify(s: &str, sep: char) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with(sep) {
            out.push(sep);
        }
    }
    let out = out.trim_end_matches(sep).to_string();
    if out.is_empty() { "game".into() } else { out }
}

fn hex(n: i64) -> String {
    if n < 0 {
        format!("-0x{:X}", n.unsigned_abs())
    } else {
        format!("0x{n:X}")
    }
}

/// The number as JSON, checked for the cheat's type.
fn number(ty: ValueType, text: &str, arch: Arch) -> Result<Json, String> {
    let clean: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',')
        .collect();
    let bad = || format!("\"{text}\" isn't a number for this value");
    ty.parse(&clean, arch).map_err(|_| bad())?;
    match ty {
        ValueType::F32 | ValueType::F64 => clean
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Json::Number)
            .ok_or_else(bad),
        _ => clean.parse::<i64>().map(Json::from).map_err(|_| bad()),
    }
}

/// Adds `cheat` to the game's own pack (`existing` is its current text, if
/// any). Returns the new pack text and the hotkey given (if one was free).
pub fn add_cheat(
    existing: Option<&str>,
    game: &GameInfo,
    cheat: &NewCheat,
) -> Result<(String, Option<String>), String> {
    let name = cheat.name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        return Err("give the cheat a name (up to 60 letters)".into());
    }
    let mut pack: Json = match existing {
        Some(text) => serde_json::from_str(text)
            .map_err(|e| format!("your cheats file for this game is broken ({e})"))?,
        None => json!({
            "schema": 1,
            "game": {
                "id": pack_id(&game.exe),
                "name": format!("{} (my cheats)", game.name.trim()),
                "exe": game.exe,
                "arch": if game.arch == Arch::X86 { "x86" } else { "x64" },
                "builds": [],
            },
            "meta": {
                "author": "You (Find a value)",
                "packVersion": "1.0.0",
                "notes": "Cheats you saved with Find a value in the Cheats tab.",
            },
            "cheats": [],
        }),
    };
    let cheats = pack["cheats"]
        .as_array()
        .ok_or("your cheats file for this game has no cheats list")?;
    let ids: Vec<&str> = cheats.iter().filter_map(|c| c["id"].as_str()).collect();
    let keys: Vec<String> = cheats
        .iter()
        .filter_map(|c| c["hotkey"].as_str())
        .map(str::to_ascii_uppercase)
        .collect();
    let base_id = {
        let mut s = slugify(name, '_');
        s.truncate(56);
        if s.starts_with(|c: char| c.is_ascii_alphanumeric()) {
            s
        } else {
            format!("c{s}")
        }
    };
    let id = (1..)
        .map(|n| {
            if n == 1 {
                base_id.clone()
            } else {
                format!("{base_id}_{n}")
            }
        })
        .find(|id| !ids.contains(&id.as_str()))
        .expect("an unused id");
    let hotkey = HOTKEYS
        .iter()
        .find(|k| !keys.contains(&k.to_string()))
        .map(|k| k.to_string());

    let module = cheat.module.to_ascii_lowercase();
    if cheat.module.contains(['/', '\\', ':', '[', ']', '+'])
        || !(module.ends_with(".exe") || module.ends_with(".dll"))
    {
        return Err(format!("\"{}\" isn't a module name", cheat.module));
    }
    let value = number(cheat.ty, &cheat.value, game.arch)?;
    let mut address =
        json!({ "base": format!("{}+{}", cheat.module, hex(cheat.module_offset as i64)) });
    if !cheat.offsets.is_empty() {
        address["offsets"] = json!(cheat.offsets.iter().map(|&o| hex(o)).collect::<Vec<_>>());
    }
    let mut new = json!({
        "id": id,
        "name": name,
        "category": "My cheats",
        "impl": { "kind": "value", "type": cheat.ty, "address": address },
    });
    if cheat.keep {
        new["ui"] = json!("toggle");
        new["toggle"] = json!({ "value": value });
        new["description"] = json!(format!("Keeps {name} at {}.", cheat.value.trim()));
    } else {
        new["ui"] = json!("button");
        new["button"] = json!({ "set": value });
        new["description"] = json!(format!("Sets {name} to {}.", cheat.value.trim()));
    }
    if let Some(k) = &hotkey {
        new["hotkey"] = json!(k);
    }
    pack["cheats"]
        .as_array_mut()
        .expect("checked above")
        .push(new);

    if let Some(sha) = &game.sha256
        && let Some(builds) = pack["game"]["builds"].as_array_mut()
        && !builds.iter().any(|b| b["sha256"] == json!(sha))
    {
        builds
            .push(json!({ "sha256": sha, "label": "saved with Find a value", "verified": false }));
    }

    let text = serde_json::to_string_pretty(&pack).map_err(|e| e.to_string())? + "\n";
    jb_packs::parse(&text).map_err(|e| format!("the cheat couldn't be saved ({e})"))?;
    Ok((text, hotkey))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> GameInfo {
        GameInfo {
            name: "Big Game".into(),
            exe: "BigGame.exe".into(),
            arch: Arch::X64,
            sha256: Some("ab".repeat(32)),
        }
    }

    fn gold(keep: bool) -> NewCheat {
        NewCheat {
            name: "Gold".into(),
            ty: ValueType::I32,
            value: "9,999".into(),
            keep,
            module: "BigGame.exe".into(),
            module_offset: 0x1A2B0,
            offsets: vec![0x10, -0x8],
        }
    }

    #[test]
    fn ids_come_from_the_exe() {
        assert_eq!(pack_id("BigGame.exe"), "mine-biggame");
        assert_eq!(pack_id("DRAGON BALL_x64.exe"), "mine-dragon-ball-x64");
        assert_eq!(pack_id("!!!.exe"), "mine-game");
    }

    #[test]
    fn saves_cheats_with_free_hotkeys_into_a_valid_pack() {
        let (text, key) = add_cheat(None, &game(), &gold(true)).unwrap();
        assert_eq!(key.as_deref(), Some("F1"));
        let (pack, _) = jb_packs::parse(&text).unwrap();
        assert_eq!(pack.game.id, "mine-biggame");
        assert_eq!(pack.game.name, "Big Game (my cheats)");
        assert_eq!(pack.game.builds.len(), 1);
        let c = &pack.cheats[0];
        assert_eq!((c.id.as_str(), c.name.as_str()), ("gold", "Gold"));
        let v: Json = serde_json::from_str(&text).unwrap();
        assert_eq!(v["cheats"][0]["toggle"]["value"], json!(9999));
        assert_eq!(
            v["cheats"][0]["impl"]["address"]["base"],
            json!("BigGame.exe+0x1A2B0")
        );
        assert_eq!(
            v["cheats"][0]["impl"]["address"]["offsets"],
            json!(["0x10", "-0x8"])
        );

        // A second one: its own id and the next hotkey; the build isn't repeated.
        let (text, key) = add_cheat(Some(&text), &game(), &gold(false)).unwrap();
        assert_eq!(key.as_deref(), Some("F2"));
        let v: Json = serde_json::from_str(&text).unwrap();
        assert_eq!(v["cheats"][1]["id"], json!("gold_2"));
        assert_eq!(v["cheats"][1]["button"]["set"], json!(9999));
        assert_eq!(v["game"]["builds"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn decimals_and_static_addresses() {
        let mut c = gold(true);
        c.ty = ValueType::F32;
        c.value = "100.5".into();
        c.offsets.clear();
        let (text, _) = add_cheat(None, &game(), &c).unwrap();
        let v: Json = serde_json::from_str(&text).unwrap();
        assert_eq!(v["cheats"][0]["toggle"]["value"], json!(100.5));
        assert!(v["cheats"][0]["impl"]["address"].get("offsets").is_none());
    }

    #[test]
    fn refuses_bad_input() {
        let mut c = gold(true);
        c.value = "lots".into();
        assert!(add_cheat(None, &game(), &c).is_err());
        let mut c = gold(true);
        c.value = "1.5".into(); // not a whole number
        assert!(add_cheat(None, &game(), &c).is_err());
        let mut c = gold(true);
        c.name = "  ".into();
        assert!(add_cheat(None, &game(), &c).is_err());
        let mut c = gold(true);
        c.module = r"C:\evil\x.exe".into();
        assert!(add_cheat(None, &game(), &c).is_err());
        assert!(add_cheat(Some("not json"), &game(), &gold(true)).is_err());
    }
}
