//! Controller hotkeys: XInput button chords like `LB+RB+DPadUp`.
//!
//! Parsing is pure; the desktop app polls the controller. A chord needs at
//! least two buttons, because the game sees the same buttons (XInput is
//! shared) and a single button would fire during normal play.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// XInput `wButtons` bits, canonical names in display order.
const BUTTONS: &[(&str, u16)] = &[
    ("LB", 0x0100),
    ("RB", 0x0200),
    ("LS", 0x0040),
    ("RS", 0x0080),
    ("Back", 0x0020),
    ("Start", 0x0010),
    ("A", 0x1000),
    ("B", 0x2000),
    ("X", 0x4000),
    ("Y", 0x8000),
    ("DPadUp", 0x0001),
    ("DPadDown", 0x0002),
    ("DPadLeft", 0x0004),
    ("DPadRight", 0x0008),
];

const ALIASES: &[(&str, &str)] = &[
    ("l1", "LB"),
    ("r1", "RB"),
    ("l3", "LS"),
    ("r3", "RS"),
    ("select", "Back"),
    ("view", "Back"),
    ("menu", "Start"),
    ("up", "DPadUp"),
    ("down", "DPadDown"),
    ("left", "DPadLeft"),
    ("right", "DPadRight"),
    ("dup", "DPadUp"),
    ("ddown", "DPadDown"),
    ("dleft", "DPadLeft"),
    ("dright", "DPadRight"),
    ("l2", "LT"),
    ("r2", "RT"),
];

/// How far a trigger must be pulled (0–255) to count as held.
pub const TRIGGER_THRESHOLD: u8 = 100;

/// A parsed controller chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PadChord {
    /// XInput button bits that must all be held.
    pub buttons: u16,
    pub lt: bool,
    pub rt: bool,
}

impl PadChord {
    /// Parses `LB+RB+DPadUp` (case-insensitive, aliases like `L1`, `Up`).
    pub fn parse(input: &str) -> Result<Self, String> {
        let mut chord = PadChord {
            buttons: 0,
            lt: false,
            rt: false,
        };
        let mut count = 0;
        for part in input.split('+') {
            let part = part.trim();
            if part.is_empty() {
                return Err(format!("'{input}' has an empty part"));
            }
            let lower = part.to_ascii_lowercase();
            let name = ALIASES
                .iter()
                .find(|(a, _)| *a == lower)
                .map_or(part, |(_, n)| n);
            let dup = || format!("'{input}' lists {name} twice");
            if name.eq_ignore_ascii_case("LT") {
                if chord.lt {
                    return Err(dup());
                }
                chord.lt = true;
            } else if name.eq_ignore_ascii_case("RT") {
                if chord.rt {
                    return Err(dup());
                }
                chord.rt = true;
            } else {
                let bit = BUTTONS
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(name))
                    .map(|(_, b)| *b)
                    .ok_or_else(|| {
                        format!("unknown controller button '{part}' (use LB, RB, LT, RT, LS, RS, A, B, X, Y, Back, Start, DPadUp…)")
                    })?;
                if chord.buttons & bit != 0 {
                    return Err(dup());
                }
                chord.buttons |= bit;
            }
            count += 1;
        }
        if count < 2 {
            return Err(format!(
                "'{input}' is one button; use two or more (e.g. LB+RB+DPadUp) so normal play doesn't trigger it"
            ));
        }
        Ok(chord)
    }

    /// Whether the chord is held in this controller state.
    pub fn held(&self, buttons: u16, left_trigger: u8, right_trigger: u8) -> bool {
        buttons & self.buttons == self.buttons
            && (!self.lt || left_trigger >= TRIGGER_THRESHOLD)
            && (!self.rt || right_trigger >= TRIGGER_THRESHOLD)
    }
}

impl fmt::Display for PadChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<&str> = Vec::new();
        for (name, bit) in BUTTONS.iter().take(2) {
            if self.buttons & bit != 0 {
                parts.push(name);
            }
        }
        if self.lt {
            parts.push("LT");
        }
        if self.rt {
            parts.push("RT");
        }
        for (name, bit) in BUTTONS.iter().skip(2) {
            if self.buttons & bit != 0 {
                parts.push(name);
            }
        }
        f.write_str(&parts.join("+"))
    }
}

impl Serialize for PadChord {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for PadChord {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        PadChord::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints_canonically() {
        let c = PadChord::parse("dpadup + rb+lb").unwrap();
        assert_eq!(c.buttons, 0x0100 | 0x0200 | 0x0001);
        assert_eq!(c.to_string(), "LB+RB+DPadUp");
        let t = PadChord::parse("L2+R2+a").unwrap();
        assert!(t.lt && t.rt);
        assert_eq!(t.to_string(), "LT+RT+A");
        assert_eq!(PadChord::parse(&t.to_string()).unwrap(), t);
    }

    #[test]
    fn refuses_single_buttons_unknowns_and_duplicates() {
        assert!(PadChord::parse("A").unwrap_err().contains("two or more"));
        assert!(PadChord::parse("LB+Z").unwrap_err().contains("unknown"));
        assert!(PadChord::parse("LB+L1").unwrap_err().contains("twice"));
        assert!(PadChord::parse("LT+L2").unwrap_err().contains("twice"));
        assert!(PadChord::parse("LB++RB").unwrap_err().contains("empty"));
    }

    #[test]
    fn held_needs_every_button_and_triggers_pulled() {
        let c = PadChord::parse("LB+RT+DPadUp").unwrap();
        assert!(c.held(0x0101, 0, 200));
        assert!(c.held(0x0101 | 0x1000, 0, 255), "extra buttons are fine");
        assert!(!c.held(0x0100, 0, 200));
        assert!(!c.held(0x0101, 0, 50), "trigger not pulled far enough");
    }

    #[test]
    fn serde_round_trip() {
        let c = PadChord::parse("Back+Start").unwrap();
        let j = serde_json::to_string(&c).unwrap();
        assert_eq!(j, "\"Back+Start\"");
        assert_eq!(serde_json::from_str::<PadChord>(&j).unwrap(), c);
        assert!(serde_json::from_str::<PadChord>("\"A\"").is_err());
    }
}
