//! Turns instant engine cheats ([`jb_core::engine_cheats`]) into an ordinary
//! pack (v1.26.0): pointer paths from the exe, hotkeys F1… for the on/off
//! cheats, sliders that keep their value. The pack is checked like any other.

use jb_core::ValueType;
use jb_core::engine_cheats::{Control, Found};
use serde_json::Number;

use crate::schema::{
    Address, Build, Cheat, Game, Impl, Meta, NumberConfig, Pack, PackArch, SCHEMA_VERSION,
    SliderConfig, ToggleConfig, Ui, ValueImpl,
};

/// The game the cheats were found in.
#[derive(Debug, Clone)]
pub struct EngineGame {
    /// What the player calls it ("Species: Unknown").
    pub name: String,
    /// `Species-Win64-Shipping.exe`.
    pub exe: String,
    pub sha256: Option<String>,
    /// `Unreal Engine 5.7`.
    pub engine: String,
    /// `YYYY-MM-DD`.
    pub created: String,
}

/// The pack id for a game's instant cheats: `engine-<exe name>`.
pub fn pack_id(exe: &str) -> String {
    let stem = exe.rsplit(['\\', '/']).next().unwrap_or(exe);
    let stem = stem
        .strip_suffix(".exe")
        .or_else(|| stem.strip_suffix(".EXE"))
        .unwrap_or(stem);
    let mut slug = String::new();
    for c in stem.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.truncate(57);
    let slug = slug.trim_end_matches('-');
    format!("engine-{}", if slug.is_empty() { "game" } else { slug })
}

fn number(ty: ValueType, v: f64) -> Option<Number> {
    match ty {
        ValueType::F32 | ValueType::F64 => Number::from_f64(v),
        _ => Some(Number::from(v.round() as i64)),
    }
}

fn hex(n: i64) -> String {
    if n < 0 {
        format!("-0x{:X}", n.unsigned_abs())
    } else {
        format!("0x{n:X}")
    }
}

/// The pack for `found`, whose pointer paths start at `exe+exe_offset`.
pub fn engine_pack(game: &EngineGame, exe_offset: u64, found: &Found) -> Pack {
    let keys = ["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8"];
    let mut next_key = keys.iter();
    let base = format!("{}+0x{exe_offset:X}", game.exe);
    let cheats = found
        .cheats
        .iter()
        .filter_map(|c| {
            let mut cheat = Cheat {
                id: c.id.clone(),
                name: c.name.clone(),
                category: Some(c.category.clone()),
                description: Some(c.description.clone()),
                ui: Ui::Toggle,
                hotkey: None,
                toggle: None,
                slider: None,
                number: None,
                button: None,
                implementation: Impl::Value(ValueImpl {
                    ty: c.ty,
                    address: Address {
                        base: Some(base.clone()),
                        aob: None,
                        module: None,
                        offset: None,
                        operand: None,
                        next: None,
                        offsets: c.offsets.iter().map(|&o| hex(o)).collect(),
                        capture: None,
                    },
                }),
            };
            match &c.control {
                Control::Keep { at } => {
                    cheat.hotkey = next_key.next().map(|k| (*k).to_string());
                    cheat.toggle = Some(ToggleConfig {
                        value: at.and_then(|v| number(c.ty, v)),
                    });
                }
                Control::Slider {
                    min,
                    max,
                    step,
                    default,
                } => {
                    cheat.ui = Ui::Slider;
                    cheat.slider = Some(SliderConfig {
                        min: number(c.ty, *min)?,
                        max: number(c.ty, *max)?,
                        step: number(c.ty, *step)?,
                        default: number(c.ty, *default),
                        hold: true,
                    });
                }
                Control::Set => {
                    cheat.ui = Ui::Number;
                    cheat.number = Some(NumberConfig {
                        min: Some(Number::from(0)),
                        max: None,
                        hold: false,
                    });
                }
            }
            Some(cheat)
        })
        .collect();
    let found_in: Vec<String> = found
        .cheats
        .iter()
        .map(|c| format!("{}: {}", c.name, c.found_in))
        .collect();
    Pack {
        schema: SCHEMA_VERSION,
        game: Game {
            id: pack_id(&game.exe),
            name: format!("{} (instant cheats)", game.name.trim()),
            engine: Some(game.engine.clone()),
            exe: game.exe.clone(),
            arch: Some(PackArch::X64),
            builds: game
                .sha256
                .iter()
                .map(|s| Build {
                    sha256: s.clone(),
                    label: Some("made on this PC".into()),
                    verified: false,
                })
                .collect(),
        },
        meta: Meta {
            author: "JuiceBox Trainer (instant engine cheats)".into(),
            created: Some(game.created.clone()),
            pack_version: "1.0.0".into(),
            notes: Some(format!(
                "Made from the game engine's own names, without searching for values. They work in \
                 many {} games, not every one. Found: {}.",
                game.engine,
                found_in.join("; ")
            )),
            source: None,
        },
        cheats,
        save_codes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use jb_core::engine_cheats::EngineCheat;

    use super::*;
    use crate::validate::Severity;

    fn cheat(id: &str, ty: ValueType, control: Control) -> EngineCheat {
        EngineCheat {
            id: id.into(),
            name: format!("Cheat {id}"),
            category: "Player".into(),
            description: "d".into(),
            ty,
            offsets: vec![0xD28, 0x38, 0, 0x30, 0x250, -0x8],
            found_in: "BP_Hero_C.Health".into(),
            now: 37.0,
            control,
        }
    }

    #[test]
    fn makes_a_valid_pack() {
        let found = Found {
            global: 0x1_4500_0000,
            cheats: vec![
                cheat("health", ValueType::F32, Control::Keep { at: Some(150.0) }),
                cheat("stamina", ValueType::F32, Control::Keep { at: None }),
                cheat(
                    "move_speed",
                    ValueType::F32,
                    Control::Slider {
                        min: 300.0,
                        max: 3000.0,
                        step: 10.0,
                        default: 600.0,
                    },
                ),
                cheat(
                    "extra_jumps",
                    ValueType::I32,
                    Control::Slider {
                        min: 1.0,
                        max: 10.0,
                        step: 1.0,
                        default: 1.0,
                    },
                ),
                cheat("money", ValueType::I32, Control::Set),
            ],
            not_found: vec![],
        };
        let game = EngineGame {
            name: "Hero Game ".into(),
            exe: "Hero-Win64-Shipping.exe".into(),
            sha256: Some("ab".repeat(32)),
            engine: "Unreal Engine 5.4".into(),
            created: "2026-11-12".into(),
        };
        let pack = engine_pack(&game, 0x4500_0000, &found);
        assert_eq!(pack.game.id, "engine-hero-win64-shipping");
        assert_eq!(pack.game.name, "Hero Game (instant cheats)");
        let issues: Vec<_> = crate::validate(&pack)
            .into_iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        assert!(issues.is_empty(), "{issues:#?}");
        // Through text too, as the store reads it.
        let text = serde_json::to_string_pretty(&pack).unwrap();
        let (back, _) = crate::parse(&text).expect("parses");
        assert_eq!(back, pack);

        let c = &pack.cheats;
        assert_eq!(c[0].hotkey.as_deref(), Some("F1"));
        assert_eq!(c[1].hotkey.as_deref(), Some("F2"));
        let Impl::Value(v) = &c[0].implementation else {
            panic!()
        };
        assert_eq!(
            v.address.base.as_deref(),
            Some("Hero-Win64-Shipping.exe+0x45000000")
        );
        assert_eq!(v.address.offsets.last().map(String::as_str), Some("-0x8"));
        assert!(c[2].slider.as_ref().is_some_and(|s| s.hold));
        assert_eq!(c[3].slider.as_ref().unwrap().step, Number::from(1));
        assert_eq!(c[4].ui, Ui::Number);
    }

    #[test]
    fn pack_ids() {
        assert_eq!(
            pack_id("C:\\G\\Species-Win64-Shipping.exe"),
            "engine-species-win64-shipping"
        );
        assert_eq!(pack_id("???.exe"), "engine-game");
    }
}
