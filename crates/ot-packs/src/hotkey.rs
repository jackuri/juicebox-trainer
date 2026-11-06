//! Hotkey strings in packs: `F1`, `Ctrl+Num1`, `Ctrl+Shift+F5`, `Alt+Num+`.
//!
//! Parsing is pure; the desktop app registers the parsed keys with Windows.
//! Letters, digits and Space need a modifier so a trainer never eats normal
//! typing. Windows-key combos are left to the OS.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A parsed hotkey with a Windows virtual-key code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Windows virtual-key code (`VK_*`).
    pub vk: u16,
}

/// `RegisterHotKey` modifier bits.
pub const MOD_ALT: u32 = 0x1;
pub const MOD_CONTROL: u32 = 0x2;
pub const MOD_SHIFT: u32 = 0x4;

/// Key names (canonical first) and virtual-key codes.
const NAMED: &[(&str, u16)] = &[
    ("Space", 0x20),
    ("PageUp", 0x21),
    ("PageDown", 0x22),
    ("End", 0x23),
    ("Home", 0x24),
    ("Left", 0x25),
    ("Up", 0x26),
    ("Right", 0x27),
    ("Down", 0x28),
    ("Insert", 0x2D),
    ("Delete", 0x2E),
    ("Pause", 0x13),
    ("Num*", 0x6A),
    ("Num+", 0x6B),
    ("Num-", 0x6D),
    ("Num.", 0x6E),
    ("Num/", 0x6F),
];

const ALIASES: &[(&str, &str)] = &[
    ("pgup", "PageUp"),
    ("pgdn", "PageDown"),
    ("ins", "Insert"),
    ("del", "Delete"),
    ("nummul", "Num*"),
    ("numadd", "Num+"),
    ("numsub", "Num-"),
    ("numdecimal", "Num."),
    ("numdiv", "Num/"),
];

impl Hotkey {
    /// Parses a hotkey string. Case-insensitive; returns a reason on error.
    pub fn parse(input: &str) -> Result<Self, String> {
        let s = input.trim();
        if s.is_empty() {
            return Err("empty hotkey".into());
        }
        let mut parts: Vec<&str> = s.split('+').map(str::trim).collect();
        // "Ctrl+Num+" splits into [.., "Num", ""]: the last '+' is the key.
        if parts.len() >= 2 && parts[parts.len() - 1].is_empty() {
            parts.pop();
            let last = parts.pop().unwrap_or_default();
            if !last.eq_ignore_ascii_case("num") {
                return Err(format!("'{input}': ends with '+'"));
            }
            parts.push("Num+");
        }
        let (key, mods) = parts
            .split_last()
            .ok_or_else(|| format!("'{input}': no key"))?;
        let mut hk = Hotkey {
            ctrl: false,
            alt: false,
            shift: false,
            vk: 0,
        };
        for m in mods {
            let flag = match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => &mut hk.ctrl,
                "alt" => &mut hk.alt,
                "shift" => &mut hk.shift,
                "win" | "windows" | "super" | "meta" => {
                    return Err(format!(
                        "'{input}': Windows-key combos are reserved for the OS"
                    ));
                }
                "" => return Err(format!("'{input}': empty part")),
                other => return Err(format!("'{input}': '{other}' is not Ctrl, Alt or Shift")),
            };
            if *flag {
                return Err(format!("'{input}': '{m}' is repeated"));
            }
            *flag = true;
        }
        hk.vk = key_code(key).ok_or_else(|| format!("'{input}': unknown key '{key}'"))?;
        let needs_modifier = matches!(hk.vk, 0x20 | 0x30..=0x39 | 0x41..=0x5A);
        if needs_modifier && !(hk.ctrl || hk.alt) {
            return Err(format!(
                "'{input}': letters, digits and Space need Ctrl or Alt so typing still works"
            ));
        }
        Ok(hk)
    }

    /// `RegisterHotKey` modifier bits (without `MOD_NOREPEAT`).
    pub fn modifiers(&self) -> u32 {
        let mut m = 0;
        if self.alt {
            m |= MOD_ALT;
        }
        if self.ctrl {
            m |= MOD_CONTROL;
        }
        if self.shift {
            m |= MOD_SHIFT;
        }
        m
    }
}

fn key_code(key: &str) -> Option<u16> {
    let lower = key.to_ascii_lowercase();
    if let Some(n) = lower.strip_prefix('f')
        && let Ok(n) = n.parse::<u16>()
        && (1..=24).contains(&n)
    {
        return Some(0x70 + n - 1);
    }
    if let Some(n) = lower.strip_prefix("num")
        && n.len() == 1
        && let Some(d) = n.chars().next().and_then(|c| c.to_digit(10))
    {
        return Some(0x60 + d as u16);
    }
    if key.len() == 1 {
        let c = key.chars().next()?.to_ascii_uppercase();
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            return Some(c as u16);
        }
    }
    let canonical = ALIASES
        .iter()
        .find(|(a, _)| *a == lower)
        .map(|(_, c)| *c)
        .unwrap_or(key);
    NAMED
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(canonical))
        .map(|(_, vk)| *vk)
}

fn key_name(vk: u16) -> String {
    match vk {
        0x70..=0x87 => format!("F{}", vk - 0x70 + 1),
        0x60..=0x69 => format!("Num{}", vk - 0x60),
        0x30..=0x39 | 0x41..=0x5A => char::from(vk as u8).to_string(),
        _ => NAMED
            .iter()
            .find(|(_, v)| *v == vk)
            .map(|(n, _)| (*n).to_string())
            .unwrap_or_else(|| format!("VK{vk:#04X}")),
    }
}

impl fmt::Display for Hotkey {
    /// Canonical form: `Ctrl+Alt+Shift+Key`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ctrl {
            f.write_str("Ctrl+")?;
        }
        if self.alt {
            f.write_str("Alt+")?;
        }
        if self.shift {
            f.write_str("Shift+")?;
        }
        f.write_str(&key_name(self.vk))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canon(s: &str) -> String {
        Hotkey::parse(s)
            .unwrap_or_else(|e| panic!("{e}"))
            .to_string()
    }

    #[test]
    fn parses_and_normalizes() {
        assert_eq!(canon("F1"), "F1");
        assert_eq!(canon("f12"), "F12");
        assert_eq!(canon("F24"), "F24");
        assert_eq!(canon("ctrl+num1"), "Ctrl+Num1");
        assert_eq!(canon("Shift + Ctrl + F5"), "Ctrl+Shift+F5");
        assert_eq!(canon("Alt+Num+"), "Alt+Num+");
        assert_eq!(canon("Ctrl+NumAdd"), "Ctrl+Num+");
        assert_eq!(canon("Num-"), "Num-");
        assert_eq!(canon("Ctrl+a"), "Ctrl+A");
        assert_eq!(canon("Alt+7"), "Alt+7");
        assert_eq!(canon("PgUp"), "PageUp");
        assert_eq!(canon("Ctrl+Space"), "Ctrl+Space");
        assert_eq!(canon("insert"), "Insert");
    }

    #[test]
    fn virtual_key_codes_and_modifiers() {
        let hk = Hotkey::parse("Ctrl+Shift+F1").unwrap();
        assert_eq!(hk.vk, 0x70);
        assert_eq!(hk.modifiers(), MOD_CONTROL | MOD_SHIFT);
        assert_eq!(Hotkey::parse("Num0").unwrap().vk, 0x60);
        assert_eq!(Hotkey::parse("Alt+Z").unwrap().vk, 0x5A);
        assert_eq!(Hotkey::parse("Alt+Z").unwrap().modifiers(), MOD_ALT);
    }

    #[test]
    fn rejects_bad_hotkeys() {
        for s in [
            "",
            "F0",
            "F25",
            "Ctrl+",
            "+F1",
            "Ctrl+Ctrl+F1",
            "Win+F1",
            "Hyper+F1",
            "A",
            "1",
            "Shift+A",
            "Space",
            "Ctrl+Nope",
            "Ctrl+F1+",
        ] {
            assert!(Hotkey::parse(s).is_err(), "{s:?} should be rejected");
        }
    }
}
