//! Address expressions typed by the user: `0x1234ABCD`, `1234ABCD` (hex, like
//! Cheat Engine) or `module.exe+0x1234`. Pointer chains arrive with packs (v0.4).

use crate::error::{Error, Result};

/// A parsed address expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddressExpr {
    Absolute(u64),
    /// `module` base plus `offset`.
    ModuleOffset {
        module: String,
        offset: u64,
    },
}

/// Parses an address expression. Numbers are always hexadecimal.
pub fn parse(input: &str) -> Result<AddressExpr> {
    let s = input.trim();
    let bad = || Error::InvalidAddress(input.to_string());
    if s.is_empty() {
        return Err(bad());
    }
    if let Some((module, offset)) = s.split_once('+') {
        let module = module.trim();
        if module.is_empty() || !looks_like_module(module) {
            return Err(bad());
        }
        let offset = parse_hex(offset).ok_or_else(bad)?;
        return Ok(AddressExpr::ModuleOffset {
            module: module.to_string(),
            offset,
        });
    }
    if looks_like_module(s) {
        return Ok(AddressExpr::ModuleOffset {
            module: s.to_string(),
            offset: 0,
        });
    }
    parse_hex(s).map(AddressExpr::Absolute).ok_or_else(bad)
}

fn looks_like_module(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    [".exe", ".dll", ".sys", ".bin"]
        .iter()
        .any(|ext| lower.ends_with(ext))
        || (lower.contains('.') && !lower.starts_with("0x"))
}

fn parse_hex(s: &str) -> Option<u64> {
    let s = s.trim().replace('_', "");
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(&s);
    if s.is_empty() {
        return None;
    }
    u64::from_str_radix(s, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_forms() {
        assert_eq!(parse("0x1234").unwrap(), AddressExpr::Absolute(0x1234));
        assert_eq!(parse("1234").unwrap(), AddressExpr::Absolute(0x1234));
        assert_eq!(
            parse(" 7FF6_A000_1000 ").unwrap(),
            AddressExpr::Absolute(0x7FF6_A000_1000)
        );
    }

    #[test]
    fn module_forms() {
        assert_eq!(
            parse("testbed_native.exe+0x2A1F0").unwrap(),
            AddressExpr::ModuleOffset {
                module: "testbed_native.exe".into(),
                offset: 0x2A1F0
            }
        );
        assert_eq!(
            parse("game.dll + 10").unwrap(),
            AddressExpr::ModuleOffset {
                module: "game.dll".into(),
                offset: 0x10
            }
        );
        assert_eq!(
            parse("game.exe").unwrap(),
            AddressExpr::ModuleOffset {
                module: "game.exe".into(),
                offset: 0
            }
        );
    }

    #[test]
    fn rejects_garbage() {
        for s in [
            "",
            "  ",
            "0x",
            "zz",
            "+0x10",
            "game.exe+",
            "game.exe+xyz",
            "0x10+0x20",
        ] {
            assert!(parse(s).is_err(), "{s:?} should be rejected");
        }
    }
}
