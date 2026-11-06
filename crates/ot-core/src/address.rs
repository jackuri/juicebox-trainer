//! Address expressions typed by the user: `0x1234ABCD`, `1234ABCD` (hex, like
//! Cheat Engine), `module.exe+0x1234`, or a pointer path in Cheat Engine
//! notation, `[[module.exe+0x1234]+0x10]+0x18` (read a pointer, add an
//! offset, repeat).

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
    /// Start at `base`, then for each offset read a pointer and add it.
    Pointer {
        base: Box<AddressExpr>,
        offsets: Vec<i64>,
    },
}

impl AddressExpr {
    /// True for pointer paths, whose target can move while the game runs.
    pub fn is_pointer(&self) -> bool {
        matches!(self, AddressExpr::Pointer { .. })
    }
}

/// Deepest pointer path accepted.
const MAX_LEVELS: usize = 16;

/// Parses `[inner]+off`, `[inner]-off` or `[inner]`.
fn parse_pointer(s: &str, input: &str) -> Result<AddressExpr> {
    let bad = || Error::InvalidAddress(input.to_string());
    // Find the bracket that closes the first one.
    let mut depth = 0usize;
    let mut close = None;
    for (i, c) in s.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth = depth.checked_sub(1).ok_or_else(bad)?;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close.ok_or_else(bad)?;
    let inner = parse_inner(&s[1..close], input)?;
    let rest = s[close + 1..].trim();
    let offset = if rest.is_empty() {
        0
    } else if let Some(r) = rest.strip_prefix('+') {
        i64::try_from(parse_hex(r).ok_or_else(bad)?).map_err(|_| bad())?
    } else if let Some(r) = rest.strip_prefix('-') {
        -i64::try_from(parse_hex(r).ok_or_else(bad)?).map_err(|_| bad())?
    } else {
        return Err(bad());
    };
    let (base, mut offsets) = match inner {
        AddressExpr::Pointer { base, offsets } => (base, offsets),
        plain => (Box::new(plain), Vec::new()),
    };
    offsets.push(offset);
    if offsets.len() > MAX_LEVELS {
        return Err(bad());
    }
    Ok(AddressExpr::Pointer { base, offsets })
}

fn parse_inner(s: &str, input: &str) -> Result<AddressExpr> {
    let s = s.trim();
    if s.starts_with('[') {
        parse_pointer(s, input)
    } else {
        parse(s).map_err(|_| Error::InvalidAddress(input.to_string()))
    }
}

/// Parses an address expression. Numbers are always hexadecimal.
pub fn parse(input: &str) -> Result<AddressExpr> {
    let s = input.trim();
    let bad = || Error::InvalidAddress(input.to_string());
    if s.is_empty() {
        return Err(bad());
    }
    if s.starts_with('[') {
        return parse_pointer(s, input);
    }
    if s.contains(['[', ']']) {
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
    fn pointer_forms() {
        let p = parse("[[game.exe+0x10]+0x18]+0x40").unwrap();
        assert_eq!(
            p,
            AddressExpr::Pointer {
                base: Box::new(AddressExpr::ModuleOffset {
                    module: "game.exe".into(),
                    offset: 0x10
                }),
                offsets: vec![0x18, 0x40],
            }
        );
        assert!(p.is_pointer());
        assert_eq!(
            parse(" [ 0x1000 ] - 8 ").unwrap(),
            AddressExpr::Pointer {
                base: Box::new(AddressExpr::Absolute(0x1000)),
                offsets: vec![-8],
            }
        );
        assert_eq!(
            parse("[game.exe+4]").unwrap(),
            AddressExpr::Pointer {
                base: Box::new(AddressExpr::ModuleOffset {
                    module: "game.exe".into(),
                    offset: 4
                }),
                offsets: vec![0],
            }
        );
        for s in [
            "[game.exe+4",
            "game.exe+4]",
            "[]",
            "[game.exe]*4",
            "[[x]]+zz",
            "a[b]",
        ] {
            assert!(parse(s).is_err(), "{s:?} should be rejected");
        }
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
