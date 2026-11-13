//! Instant engine cheats (v1.26.0): cheats found from what every game on an
//! engine has, so a game needs no pack, no AI and no value search first.
//!
//! For Unreal Engine 4.23+ / 5 games, the engine's own classes give the
//! way from the engine object to the player: `GameEngine.GameInstance` →
//! `LocalPlayers[0]` → `PlayerController` → `Pawn` → `CharacterMovement`.
//! Their offsets are read from the game's reflection data by name (see
//! [`crate::unreal`]), so each cheat is an ordinary pointer path from a
//! global in the exe: it follows the player through respawns and level
//! loads, and goes in a normal, declarative pack.
//!
//! What is found:
//! - engine values every Unreal game has: game speed (`WorldSettings
//!   .TimeDilation`), move speed, jump height, gravity, extra jumps
//!   (`CharacterMovementComponent`, `Character`);
//! - the player's own values by their usual names (`Health`, `Stamina`,
//!   `Ammo`, `Gold`, …) on the player's character, its components, things
//!   it owns (its weapon) and its player state. Never through a pointer to
//!   something else (an enemy it last hit), so another actor's values are
//!   never changed.
//!
//! Reading only: nothing in the game is changed here.

use serde::Serialize;

use crate::error::{Error, Result};
use crate::unreal::Property;
use crate::value::ValueType;

fn bad(why: impl Into<String>) -> Error {
    Error::Debugger(why.into())
}

/// What finding cheats needs from a connected engine reader (a trait so
/// tests can fake a game).
pub trait Graph {
    fn ptr(&self, address: u64) -> Option<u64>;
    fn read(&self, address: u64, len: usize) -> Option<Vec<u8>>;
    /// Every property of the object's class, inherited ones too.
    fn properties_of(&self, object: u64) -> Vec<Property>;
    /// The object's class name (`BP_PlayerCharacter_C`).
    fn class_name_of(&self, object: u64) -> String;
    /// The object's class is `class` or derives from it.
    fn is_a(&self, object: u64, class: &str) -> bool;
    /// The object that contains this one (an actor's components are inside it).
    fn outer_of(&self, object: u64) -> Option<u64>;
    /// A live (not default) object of the class or a subclass.
    fn find_live(&self, class: &str) -> Option<u64>;
    /// An address inside the game's exe that holds a pointer to `target`.
    fn global_pointing_to(&self, target: u64) -> Option<u64>;
}

/// How a cheat is used.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Control {
    /// On/off: keep the value at `at` (or where it is, if none).
    Keep { at: Option<f64> },
    /// A value from a range, kept there.
    Slider {
        min: f64,
        max: f64,
        step: f64,
        default: f64,
    },
    /// Type a number to set it once.
    Set,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineCheat {
    /// `game_speed`, `health`, …
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub ty: ValueType,
    /// From the global ([`Found::global`]): read a pointer, add the offset, for each.
    pub offsets: Vec<i64>,
    /// Where the path starts instead, when not at [`Found::global`] in the exe: a module
    /// name and an offset in it (Unity: `GameAssembly.dll`, `UnityPlayer.dll`).
    pub base: Option<(String, u64)>,
    /// Where it was found (`BP_Hero_C.Health`), for the pack's notes.
    pub found_in: String,
    /// Its value when found.
    pub now: f64,
    pub control: Control,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    /// The address inside the exe every cheat's pointer path starts from.
    pub global: u64,
    pub cheats: Vec<EngineCheat>,
    /// The usual cheats that weren't found ("Ammo", "Money").
    pub not_found: Vec<String>,
}

/// Player values looked for by name: (id, cheat name, names, names of its maximum).
pub(crate) const PLAYER_VALUES: &[(&str, &str, &[&str], &[&str])] = &[
    (
        "health",
        "Infinite health",
        &[
            "health",
            "currenthealth",
            "curhealth",
            "hp",
            "currenthp",
            "curhp",
            "hitpoints",
            "healthpoints",
            "life",
            "currentlife",
        ],
        &["maxhealth", "healthmax", "maxhp", "maxhitpoints", "maxlife"],
    ),
    (
        "stamina",
        "Infinite stamina",
        &[
            "stamina",
            "currentstamina",
            "curstamina",
            "energy",
            "currentenergy",
        ],
        &["maxstamina", "staminamax", "maxenergy"],
    ),
    (
        "mana",
        "Infinite mana",
        &["mana", "currentmana", "mp", "currentmp"],
        &["maxmana", "manamax", "maxmp"],
    ),
    (
        "ammo",
        "Infinite ammo",
        &[
            "ammo",
            "currentammo",
            "ammoinclip",
            "clipammo",
            "ammoinmagazine",
            "magazineammo",
            "currentclipammo",
            "currentmagazineammo",
            "roundsinclip",
            "loadedammo",
            "ammocount",
        ],
        &[
            "maxammo",
            "clipsize",
            "magazinesize",
            "maxclipammo",
            "maxammoinclip",
            "magazinecapacity",
        ],
    ),
];

/// Money looked for by name (set once, not kept).
pub(crate) const MONEY: &[&str] = &[
    "money",
    "gold",
    "coins",
    "cash",
    "credits",
    "currency",
    "currentmoney",
    "playermoney",
    "souls",
];

const NOTE: &str = "An instant engine cheat: it works in many Unreal games, not every one. If it \
                    does nothing, this game keeps the value its own way (the AI helper can look).";

/// Letters and digits, lower case, without a leading `m_` / `b`-style prefix.
fn norm(name: &str) -> String {
    let n: String = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    match name.strip_prefix("m_") {
        Some(rest) => norm(rest),
        None => n,
    }
}

fn value_type(ty: &str) -> Option<ValueType> {
    Some(match ty {
        "FloatProperty" => ValueType::F32,
        "DoubleProperty" => ValueType::F64,
        "IntProperty" => ValueType::I32,
        "UInt32Property" => ValueType::U32,
        "Int64Property" => ValueType::I64,
        "Int16Property" => ValueType::I16,
        _ => return None,
    })
}

fn read_num<G: Graph>(g: &G, at: u64, ty: ValueType) -> Option<f64> {
    let b = g.read(at, ty.size(crate::Arch::X64))?;
    Some(match ty {
        ValueType::F32 => f64::from(f32::from_le_bytes(b.try_into().ok()?)),
        ValueType::F64 => f64::from_le_bytes(b.try_into().ok()?),
        ValueType::I32 => f64::from(i32::from_le_bytes(b.try_into().ok()?)),
        ValueType::U32 => f64::from(u32::from_le_bytes(b.try_into().ok()?)),
        ValueType::I64 => i64::from_le_bytes(b.try_into().ok()?) as f64,
        ValueType::I16 => f64::from(i16::from_le_bytes(b.try_into().ok()?)),
        _ => return None,
    })
    .filter(|v| v.is_finite())
}

/// An object reached from the global: `object = *(resolve(path))`.
#[derive(Debug, Clone)]
struct At {
    object: u64,
    path: Vec<i64>,
}

impl At {
    fn field(&self, offset: u64) -> Vec<i64> {
        let mut p = self.path.clone();
        p.push(offset as i64);
        p
    }
}

fn prop<G: Graph>(g: &G, object: u64, name: &str) -> Option<Property> {
    g.properties_of(object).into_iter().find(|p| p.name == name)
}

/// The object an object property points to.
fn follow<G: Graph>(g: &G, from: &At, name: &str) -> Option<At> {
    let p = prop(g, from.object, name).filter(|p| p.ty == "ObjectProperty")?;
    let object = g.ptr(from.object + p.offset)?;
    Some(At {
        object,
        path: from.field(p.offset),
    })
}

/// The first element of an array property of object pointers.
fn first_of<G: Graph>(g: &G, from: &At, name: &str) -> Option<At> {
    let p = prop(g, from.object, name).filter(|p| p.ty == "ArrayProperty")?;
    let data = g.ptr(from.object + p.offset)?;
    let object = g.ptr(data)?;
    let mut path = from.field(p.offset);
    path.push(0);
    Some(At { object, path })
}

/// A numeric value on an object.
fn number<G: Graph>(g: &G, at: &At, name: &str) -> Option<(Property, ValueType, f64)> {
    let p = prop(g, at.object, name)?;
    let ty = value_type(&p.ty)?;
    let now = read_num(g, at.object + p.offset, ty)?;
    Some((p, ty, now))
}

fn round_to(v: f64, step: f64) -> f64 {
    ((v / step).round() * step * 1000.0).round() / 1000.0
}

/// A slider around the value the game uses now (`low`..`high` times it).
pub(crate) fn around(now: f64, low: f64, high: f64) -> Option<Control> {
    if now.is_nan() || now <= 0.0 || now > 1e7 {
        return None;
    }
    let step = if now >= 100.0 {
        10f64.powf((now.log10() - 1.0).floor())
    } else if now >= 10.0 {
        1.0
    } else {
        0.1
    };
    let min = round_to(now * low, step).max(step);
    let max = round_to(now * high, step);
    Some(Control::Slider {
        min,
        max,
        step,
        default: round_to(now, step).clamp(min, max),
    })
}

/// Finds the instant cheats of a running Unreal game.
pub fn find_unreal<G: Graph>(g: &G) -> Result<Found> {
    let engine = g
        .find_live("GameEngine")
        .or_else(|| g.find_live("Engine"))
        .ok_or_else(|| bad("couldn't find the game's engine object"))?;
    let global = g
        .global_pointing_to(engine)
        .ok_or_else(|| bad("couldn't find where the game keeps its engine"))?;
    let root = At {
        object: engine,
        path: Vec::new(),
    };
    let not_in_game = || {
        bad(
            "your character isn't in the world yet: load your save or start playing, then try again",
        )
    };
    let pc = follow(g, &root, "GameInstance")
        .and_then(|gi| first_of(g, &gi, "LocalPlayers"))
        .and_then(|lp| follow(g, &lp, "PlayerController"))
        .ok_or_else(not_in_game)?;
    let pawn = follow(g, &pc, "Pawn")
        .or_else(|| follow(g, &pc, "AcknowledgedPawn"))
        .ok_or_else(not_in_game)?;
    let who = g.class_name_of(pawn.object);

    let mut cheats = Vec::new();
    let mut not_found = Vec::new();

    // World: game speed.
    let world_settings = follow(g, &root, "GameViewport")
        .and_then(|v| follow(g, &v, "World"))
        .and_then(|w| follow(g, &w, "PersistentLevel"))
        .and_then(|level| {
            follow(g, &level, "WorldSettings").or_else(|| {
                // Not a listed property in some versions: look in the level.
                (0x28u64..0x800).step_by(8).find_map(|o| {
                    let p = g.ptr(level.object + o)?;
                    g.is_a(p, "WorldSettings").then(|| At {
                        object: p,
                        path: level.field(o),
                    })
                })
            })
        });
    match world_settings.and_then(|ws| number(g, &ws, "TimeDilation").map(|n| (ws, n))) {
        Some((ws, (p, ty, now))) => cheats.push(EngineCheat {
            id: "game_speed".into(),
            name: "Game speed".into(),
            category: "World".into(),
            description: format!(
                "1 is normal; lower slows the game down, higher speeds it up. {NOTE}"
            ),
            ty,
            offsets: ws.field(p.offset),
            base: None,
            found_in: format!("{}.TimeDilation", g.class_name_of(ws.object)),
            now,
            control: Control::Slider {
                min: 0.1,
                max: 5.0,
                step: 0.1,
                default: 1.0,
            },
        }),
        None => not_found.push("Game speed".into()),
    }

    // Movement: the character's movement component.
    let movement = follow(g, &pawn, "CharacterMovement");
    let moves: [(&str, &str, &str, f64, f64, &str); 3] = [
        (
            "move_speed",
            "Move speed",
            "MaxWalkSpeed",
            0.5,
            5.0,
            "How fast you walk and run.",
        ),
        (
            "jump_height",
            "Jump height",
            "JumpZVelocity",
            0.5,
            4.0,
            "How high you jump.",
        ),
        (
            "gravity",
            "Gravity",
            "GravityScale",
            0.1,
            3.0,
            "Lower makes you float, higher pulls you down.",
        ),
    ];
    for (id, name, field, low, high, what) in moves {
        let hit = movement.as_ref().and_then(|m| {
            let (p, ty, now) = number(g, m, field)?;
            let control = if id == "gravity" {
                (now > 0.0).then_some(Control::Slider {
                    min: 0.1,
                    max: 3.0,
                    step: 0.1,
                    default: round_to(now, 0.1).clamp(0.1, 3.0),
                })?
            } else {
                around(now, low, high)?
            };
            Some(EngineCheat {
                id: id.into(),
                name: name.into(),
                category: "Movement".into(),
                description: format!("{what} {NOTE}"),
                ty,
                offsets: m.field(p.offset),
                base: None,
                found_in: format!("{}.{field}", g.class_name_of(m.object)),
                now,
                control,
            })
        });
        match hit {
            Some(c) => cheats.push(c),
            None => not_found.push(name.into()),
        }
    }
    match number(g, &pawn, "JumpMaxCount")
        .filter(|(_, ty, now)| *ty == ValueType::I32 && *now >= 1.0)
    {
        Some((p, ty, now)) => cheats.push(EngineCheat {
            id: "extra_jumps".into(),
            name: "Jumps in the air".into(),
            category: "Movement".into(),
            description: format!("How many times you can jump before landing. {NOTE}"),
            ty,
            offsets: pawn.field(p.offset),
            base: None,
            found_in: format!("{who}.JumpMaxCount"),
            now,
            control: Control::Slider {
                min: 1.0,
                max: 10.0,
                step: 1.0,
                default: now.clamp(1.0, 10.0),
            },
        }),
        None => not_found.push("Jumps in the air".into()),
    }

    // The player's own values, by name: on the character first, then its
    // components and things it owns, then its player state.
    let places = player_places(g, &pawn, &pc);
    for (id, name, names, maxes) in PLAYER_VALUES {
        match find_named(g, &places, names) {
            Some((at, p, ty, now)) => {
                let max = g
                    .properties_of(at.object)
                    .into_iter()
                    .find(|m| maxes.contains(&norm(&m.name).as_str()) && m.ty == p.ty)
                    .and_then(|m| read_num(g, at.object + m.offset, ty))
                    .filter(|m| *m > 0.0);
                let keep_at = max.or((now > 0.0).then_some(now));
                cheats.push(EngineCheat {
                    id: (*id).into(),
                    name: (*name).into(),
                    category: "Player".into(),
                    description: format!(
                        "Keeps {} at {}. {NOTE}",
                        p.name,
                        keep_at.map_or("where it is".into(), |v| format!("{v}"))
                    ),
                    ty,
                    offsets: at.field(p.offset),
                    base: None,
                    found_in: format!("{}.{}", g.class_name_of(at.object), p.name),
                    now,
                    control: Control::Keep { at: keep_at },
                });
            }
            None => {
                let what = name.trim_start_matches("Infinite ");
                let mut c = what.chars();
                not_found.push(c.next().map_or_else(String::new, |f| {
                    f.to_ascii_uppercase().to_string() + c.as_str()
                }));
            }
        }
    }
    match find_named(g, &places, MONEY) {
        Some((at, p, ty, now)) => cheats.push(EngineCheat {
            id: "money".into(),
            name: format!("Set {}", p.name.trim_start_matches("m_")),
            category: "Items".into(),
            description: format!("Type how much {} you want. {NOTE}", p.name),
            ty,
            offsets: at.field(p.offset),
            base: None,
            found_in: format!("{}.{}", g.class_name_of(at.object), p.name),
            now,
            control: Control::Set,
        }),
        None => not_found.push("Money".into()),
    }

    Ok(Found {
        global,
        cheats,
        not_found,
    })
}

/// The character, then objects inside it or owned by it (components, its
/// weapon), then the player state. Only objects that belong to the player.
fn player_places<G: Graph>(g: &G, pawn: &At, pc: &At) -> Vec<At> {
    let mut out = vec![pawn.clone()];
    let owned = |at: &At, owner: u64| {
        g.outer_of(at.object) == Some(owner)
            || prop(g, at.object, "Owner")
                .filter(|p| p.ty == "ObjectProperty")
                .and_then(|p| g.ptr(at.object + p.offset))
                == Some(owner)
    };
    let add_owned = |from: &At, out: &mut Vec<At>| {
        for p in g.properties_of(from.object) {
            if p.ty != "ObjectProperty" || p.name == "Owner" || p.name == "Instigator" {
                continue;
            }
            let Some(object) = g.ptr(from.object + p.offset) else {
                continue;
            };
            let at = At {
                object,
                path: from.field(p.offset),
            };
            if object != from.object
                && !out.iter().any(|o: &At| o.object == object)
                && owned(&at, from.object)
            {
                out.push(at);
            }
        }
    };
    add_owned(pawn, &mut out);
    if let Some(state) = follow(g, pc, "PlayerState") {
        if !out.iter().any(|o| o.object == state.object) {
            out.push(state.clone());
        }
        add_owned(&state, &mut out);
    }
    out
}

/// The first numeric property with one of these names (in `places` order).
fn find_named<G: Graph>(
    g: &G,
    places: &[At],
    names: &[&str],
) -> Option<(At, Property, ValueType, f64)> {
    for at in places {
        for p in g.properties_of(at.object) {
            if !names.contains(&norm(&p.name).as_str()) {
                continue;
            }
            let Some(ty) = value_type(&p.ty) else {
                continue;
            };
            if let Some(now) = read_num(g, at.object + p.offset, ty) {
                return Some((at.clone(), p, ty, now));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// A tiny fake Unreal game: objects with a class, an outer and fields.
    #[derive(Default)]
    struct Fake {
        mem: HashMap<u64, Vec<u8>>,
        objects: HashMap<u64, (String, Vec<String>, Option<u64>)>,
        classes: HashMap<String, Vec<Property>>,
        global: Option<(u64, u64)>,
    }

    impl Fake {
        fn class(&mut self, name: &str, props: &[(&str, &str, u64)]) {
            self.classes.insert(
                name.into(),
                props
                    .iter()
                    .map(|(n, t, o)| Property {
                        name: (*n).into(),
                        ty: (*t).into(),
                        offset: *o,
                        size: 8,
                    })
                    .collect(),
            );
        }
        /// An object of `class` (also counted as each of `is`).
        fn object(&mut self, at: u64, class: &str, is: &[&str], outer: Option<u64>) {
            self.objects.insert(
                at,
                (
                    class.into(),
                    is.iter().map(|s| (*s).to_string()).collect(),
                    outer,
                ),
            );
        }
        fn put_ptr(&mut self, at: u64, p: u64) {
            for (i, b) in p.to_le_bytes().into_iter().enumerate() {
                self.mem.insert(at + i as u64, vec![b]);
            }
        }
        fn put_f32(&mut self, at: u64, v: f32) {
            for (i, b) in v.to_le_bytes().into_iter().enumerate() {
                self.mem.insert(at + i as u64, vec![b]);
            }
        }
        fn put_i32(&mut self, at: u64, v: i32) {
            for (i, b) in v.to_le_bytes().into_iter().enumerate() {
                self.mem.insert(at + i as u64, vec![b]);
            }
        }
    }

    impl Graph for Fake {
        fn ptr(&self, a: u64) -> Option<u64> {
            self.read(a, 8)
                .map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
                .filter(|p| *p >= 0x10000)
        }
        fn read(&self, a: u64, len: usize) -> Option<Vec<u8>> {
            (0..len as u64)
                .map(|i| self.mem.get(&(a + i)).map(|b| b[0]))
                .collect()
        }
        fn properties_of(&self, object: u64) -> Vec<Property> {
            let Some((class, is, _)) = self.objects.get(&object) else {
                return Vec::new();
            };
            std::iter::once(class)
                .chain(is)
                .filter_map(|c| self.classes.get(c))
                .flatten()
                .cloned()
                .collect()
        }
        fn class_name_of(&self, object: u64) -> String {
            self.objects
                .get(&object)
                .map_or("?".into(), |o| o.0.clone())
        }
        fn is_a(&self, object: u64, class: &str) -> bool {
            self.objects
                .get(&object)
                .is_some_and(|(c, is, _)| c == class || is.iter().any(|i| i == class))
        }
        fn outer_of(&self, object: u64) -> Option<u64> {
            self.objects.get(&object).and_then(|o| o.2)
        }
        fn find_live(&self, class: &str) -> Option<u64> {
            let mut all: Vec<u64> = self
                .objects
                .keys()
                .copied()
                .filter(|&o| self.is_a(o, class))
                .collect();
            all.sort_unstable();
            all.first().copied()
        }
        fn global_pointing_to(&self, target: u64) -> Option<u64> {
            self.global.filter(|g| g.1 == target).map(|g| g.0)
        }
    }

    const ENGINE: u64 = 0x10_0000;
    const GI: u64 = 0x11_0000;
    const LP_DATA: u64 = 0x12_0000;
    const LP: u64 = 0x13_0000;
    const PC: u64 = 0x14_0000;
    const PAWN: u64 = 0x15_0000;
    const CMC: u64 = 0x16_0000;
    const VIEWPORT: u64 = 0x17_0000;
    const WORLD: u64 = 0x18_0000;
    const LEVEL: u64 = 0x19_0000;
    const WS: u64 = 0x1A_0000;
    const HEALTHC: u64 = 0x1B_0000;
    const ENEMY: u64 = 0x1C_0000;
    const WEAPON: u64 = 0x1D_0000;
    const STATE: u64 = 0x1E_0000;

    fn game() -> Fake {
        let mut f = Fake::default();
        f.class(
            "GameEngine",
            &[
                ("GameInstance", "ObjectProperty", 0xD28),
                ("GameViewport", "ObjectProperty", 0x780),
            ],
        );
        f.class("GameInstance", &[("LocalPlayers", "ArrayProperty", 0x38)]);
        f.class(
            "LocalPlayer",
            &[("PlayerController", "ObjectProperty", 0x30)],
        );
        f.class(
            "PlayerController",
            &[
                ("Pawn", "ObjectProperty", 0x250),
                ("PlayerState", "ObjectProperty", 0x240),
            ],
        );
        f.class(
            "Actor",
            &[
                ("Owner", "ObjectProperty", 0x140),
                ("CustomTimeDilation", "FloatProperty", 0x64),
            ],
        );
        f.class(
            "BP_Hero_C",
            &[
                ("CharacterMovement", "ObjectProperty", 0x288),
                ("JumpMaxCount", "IntProperty", 0x3F0),
                ("HealthComp", "ObjectProperty", 0x600),
                ("LastHitEnemy", "ObjectProperty", 0x608),
                ("CurrentWeapon", "ObjectProperty", 0x610),
            ],
        );
        f.class(
            "CharacterMovementComponent",
            &[
                ("GravityScale", "FloatProperty", 0x168),
                ("MaxWalkSpeed", "FloatProperty", 0x1C8),
                ("JumpZVelocity", "FloatProperty", 0x160),
            ],
        );
        f.class("GameViewportClient", &[("World", "ObjectProperty", 0x78)]);
        f.class("World", &[("PersistentLevel", "ObjectProperty", 0x30)]);
        f.class("Level", &[]);
        f.class("WorldSettings", &[("TimeDilation", "FloatProperty", 0x2E8)]);
        f.class(
            "HealthComponent",
            &[
                ("Health", "FloatProperty", 0xB0),
                ("MaxHealth", "FloatProperty", 0xB4),
            ],
        );
        f.class("BP_Enemy_C", &[("Health", "FloatProperty", 0x500)]);
        f.class(
            "BP_Rifle_C",
            &[
                ("AmmoInClip", "IntProperty", 0x300),
                ("ClipSize", "IntProperty", 0x304),
            ],
        );
        f.class("BP_PlayerState_C", &[("Gold", "IntProperty", 0x3A0)]);

        f.object(ENGINE, "GameEngine", &["Engine"], None);
        f.object(GI, "GameInstance", &[], Some(ENGINE));
        f.object(LP, "LocalPlayer", &[], Some(ENGINE));
        f.object(PC, "PlayerController", &["Actor"], Some(LEVEL));
        f.object(PAWN, "BP_Hero_C", &["Character", "Actor"], Some(LEVEL));
        f.object(CMC, "CharacterMovementComponent", &[], Some(PAWN));
        f.object(VIEWPORT, "GameViewportClient", &[], Some(ENGINE));
        f.object(WORLD, "World", &[], None);
        f.object(LEVEL, "Level", &[], Some(WORLD));
        f.object(WS, "WorldSettings", &["Actor"], Some(LEVEL));
        f.object(HEALTHC, "HealthComponent", &[], Some(PAWN));
        f.object(ENEMY, "BP_Enemy_C", &["Actor"], Some(LEVEL));
        f.object(WEAPON, "BP_Rifle_C", &["Actor"], Some(LEVEL));
        f.object(STATE, "BP_PlayerState_C", &["Actor"], Some(LEVEL));

        f.global = Some((0x1_4500_0000, ENGINE));
        f.put_ptr(ENGINE + 0xD28, GI);
        f.put_ptr(ENGINE + 0x780, VIEWPORT);
        f.put_ptr(GI + 0x38, LP_DATA);
        f.put_ptr(LP_DATA, LP);
        f.put_ptr(LP + 0x30, PC);
        f.put_ptr(PC + 0x250, PAWN);
        f.put_ptr(PC + 0x240, STATE);
        f.put_ptr(PAWN + 0x288, CMC);
        f.put_i32(PAWN + 0x3F0, 1);
        f.put_ptr(PAWN + 0x600, HEALTHC);
        f.put_ptr(PAWN + 0x608, ENEMY);
        f.put_ptr(PAWN + 0x610, WEAPON);
        f.put_ptr(WEAPON + 0x140, PAWN);
        f.put_f32(CMC + 0x1C8, 600.0);
        f.put_f32(CMC + 0x160, 420.0);
        f.put_f32(CMC + 0x168, 1.0);
        f.put_ptr(VIEWPORT + 0x78, WORLD);
        f.put_ptr(WORLD + 0x30, LEVEL);
        // WorldSettings isn't a listed property of Level here: found by looking.
        f.put_ptr(LEVEL + 0x298, WS);
        f.put_f32(WS + 0x2E8, 1.0);
        f.put_f32(HEALTHC + 0xB0, 37.0);
        f.put_f32(HEALTHC + 0xB4, 150.0);
        f.put_f32(ENEMY + 0x500, 999.0);
        f.put_i32(WEAPON + 0x300, 12);
        f.put_i32(WEAPON + 0x304, 30);
        f.put_i32(STATE + 0x3A0, 250);
        f
    }

    fn cheat<'a>(f: &'a Found, id: &str) -> &'a EngineCheat {
        f.cheats
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("no {id}: {f:#?}"))
    }

    #[test]
    fn finds_engine_and_player_cheats_as_pointer_paths() {
        let found = find_unreal(&game()).expect("found");
        assert_eq!(found.global, 0x1_4500_0000);
        assert!(found.not_found.contains(&"Stamina".to_string()));
        assert!(found.not_found.contains(&"Mana".to_string()));

        let speed = cheat(&found, "game_speed");
        // engine → GameViewport → World → PersistentLevel → (looked up) → TimeDilation
        assert_eq!(speed.offsets, vec![0x780, 0x78, 0x30, 0x298, 0x2E8]);
        assert_eq!(speed.ty, ValueType::F32);

        // engine → GameInstance → LocalPlayers[0] → PlayerController → Pawn → CharacterMovement → MaxWalkSpeed
        let walk = cheat(&found, "move_speed");
        assert_eq!(
            walk.offsets,
            vec![0xD28, 0x38, 0, 0x30, 0x250, 0x288, 0x1C8]
        );
        assert_eq!(walk.now, 600.0);
        assert_eq!(
            walk.control,
            Control::Slider {
                min: 300.0,
                max: 3000.0,
                step: 10.0,
                default: 600.0
            }
        );
        assert_eq!(cheat(&found, "gravity").offsets.last(), Some(&0x168));
        assert_eq!(cheat(&found, "extra_jumps").ty, ValueType::I32);

        // The player's health component, kept at its maximum; never the enemy it last hit.
        let health = cheat(&found, "health");
        assert_eq!(
            health.offsets,
            vec![0xD28, 0x38, 0, 0x30, 0x250, 0x600, 0xB0]
        );
        assert_eq!(health.control, Control::Keep { at: Some(150.0) });
        assert_eq!(health.found_in, "HealthComponent.Health");

        // The weapon it owns; the clip's size is its maximum.
        let ammo = cheat(&found, "ammo");
        assert_eq!(
            ammo.offsets,
            vec![0xD28, 0x38, 0, 0x30, 0x250, 0x610, 0x300]
        );
        assert_eq!(ammo.control, Control::Keep { at: Some(30.0) });

        // Gold on the player state.
        let gold = cheat(&found, "money");
        assert_eq!(gold.offsets, vec![0xD28, 0x38, 0, 0x30, 0x240, 0x3A0]);
        assert_eq!(gold.name, "Set Gold");
        assert_eq!(gold.control, Control::Set);
    }

    #[test]
    fn another_actors_values_are_never_used() {
        let mut f = game();
        // Without its health component, the only "Health" left is the enemy's.
        f.put_ptr(PAWN + 0x600, 0);
        let found = find_unreal(&f).expect("found");
        assert!(found.cheats.iter().all(|c| c.id != "health"), "{found:#?}");
        assert!(found.not_found.contains(&"Health".to_string()));
    }

    #[test]
    fn needs_the_player_in_the_world() {
        let mut f = game();
        f.put_ptr(PC + 0x250, 0);
        let e = find_unreal(&f).expect_err("no pawn");
        assert!(e.to_string().contains("isn't in the world yet"), "{e}");
        let mut f = game();
        f.global = None;
        assert!(find_unreal(&f).is_err());
    }

    #[test]
    fn names_and_sliders() {
        assert_eq!(norm("m_CurrentHealth"), "currenthealth");
        assert_eq!(norm("Ammo_In_Clip"), "ammoinclip");
        assert_eq!(
            around(1.0, 0.5, 5.0),
            Some(Control::Slider {
                min: 0.5,
                max: 5.0,
                step: 0.1,
                default: 1.0
            })
        );
        assert_eq!(
            around(42.0, 0.5, 4.0),
            Some(Control::Slider {
                min: 21.0,
                max: 168.0,
                step: 1.0,
                default: 42.0
            })
        );
        assert_eq!(around(0.0, 0.5, 5.0), None);
        assert_eq!(around(f64::NAN, 0.5, 5.0), None);
    }
}
