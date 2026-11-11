//! Array-of-bytes (AOB) signatures: `48 8B 05 ?? ?? ?? ?? 48 85 C0`.
//!
//! A pattern is a list of hex bytes and wildcards (`??` or `?`), separated by
//! spaces. Searching happens in [`AttachedProcess::aob_scan`](crate::AttachedProcess::aob_scan);
//! this module only parses patterns and matches them against byte slices.

use std::fmt;

use crate::error::{Error, Result};

/// A parsed AOB pattern. `None` entries are wildcards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    bytes: Vec<Option<u8>>,
    /// First non-wildcard byte: (index, value). Patterns are never all wildcards.
    anchor: (usize, u8),
}

impl Pattern {
    /// Parses `"48 8B ?? 05"`. Rejects empty and all-wildcard patterns.
    pub fn parse(input: &str) -> Result<Self> {
        let bad = |why: &str| Error::InvalidPattern(format!("'{input}': {why}"));
        let mut bytes = Vec::new();
        for token in input.split_whitespace() {
            if token == "??" || token == "?" {
                bytes.push(None);
            } else if token.len() == 2 && token.chars().all(|c| c.is_ascii_hexdigit()) {
                bytes.push(Some(u8::from_str_radix(token, 16).expect("two hex digits")));
            } else {
                return Err(bad(&format!("'{token}' is not a hex byte or ??")));
            }
        }
        if bytes.is_empty() {
            return Err(bad("empty pattern"));
        }
        let anchor = bytes
            .iter()
            .enumerate()
            .find_map(|(i, b)| b.map(|v| (i, v)))
            .ok_or_else(|| bad("a pattern needs at least one fixed byte"))?;
        Ok(Self { bytes, anchor })
    }

    /// Builds a pattern from bytes and wildcards. Fails if every byte is a
    /// wildcard.
    pub fn from_masked(bytes: Vec<Option<u8>>) -> Result<Self> {
        let anchor = bytes
            .iter()
            .enumerate()
            .find_map(|(i, b)| b.map(|v| (i, v)))
            .ok_or_else(|| {
                Error::InvalidPattern("a pattern needs at least one fixed byte".into())
            })?;
        Ok(Self { bytes, anchor })
    }

    /// Parses plain hex bytes with no wildcards (e.g. patch bytes).
    pub fn parse_exact(input: &str) -> Result<Vec<u8>> {
        let p = Self::parse(input)?;
        p.bytes
            .iter()
            .map(|b| {
                b.ok_or_else(|| Error::InvalidPattern(format!("'{input}': no ?? allowed here")))
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Number of fixed (non-wildcard) bytes.
    pub fn fixed_len(&self) -> usize {
        self.bytes.iter().filter(|b| b.is_some()).count()
    }

    /// The pattern's bytes; `None` is a wildcard.
    pub fn bytes(&self) -> &[Option<u8>] {
        &self.bytes
    }

    /// True if the pattern matches `hay` at its start.
    pub fn matches_at(&self, hay: &[u8]) -> bool {
        hay.len() >= self.bytes.len()
            && self
                .bytes
                .iter()
                .zip(hay)
                .all(|(p, h)| p.is_none_or(|v| v == *h))
    }

    /// Offsets of matches in `hay`, at most `limit` of them.
    pub fn find_all(&self, hay: &[u8], limit: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let m = self.bytes.len();
        if hay.len() < m || limit == 0 {
            return out;
        }
        let (ai, ab) = self.anchor;
        let last_start = hay.len() - m;
        let mut start = 0;
        while start <= last_start {
            // Jump to the next place the anchor byte could line up.
            let window = &hay[start + ai..=last_start + ai];
            let Some(p) = window.iter().position(|&b| b == ab) else {
                break;
            };
            let at = start + p;
            if self.matches_at(&hay[at..]) {
                out.push(at);
                if out.len() == limit {
                    break;
                }
            }
            start = at + 1;
        }
        out
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, b) in self.bytes.iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            match b {
                Some(v) => write!(f, "{v:02X}")?,
                None => f.write_str("??")?,
            }
        }
        Ok(())
    }
}

/// Formats bytes as `"29 D0 89 01"`.
pub fn hex_string(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_displays() {
        let p = Pattern::parse("48 8b ?? ? 05").unwrap();
        assert_eq!(p.len(), 5);
        assert_eq!(p.fixed_len(), 3);
        assert_eq!(p.to_string(), "48 8B ?? ?? 05");
    }

    #[test]
    fn rejects_bad_patterns() {
        for s in ["", "   ", "?? ??", "4", "123", "GG", "48-8B", "0x48"] {
            assert!(Pattern::parse(s).is_err(), "{s:?} should be rejected");
        }
        assert!(Pattern::parse_exact("90 ?? 90").is_err());
        assert_eq!(Pattern::parse_exact("90 90").unwrap(), vec![0x90, 0x90]);
    }

    #[test]
    fn finds_matches_with_wildcards_and_leading_wildcard() {
        let hay = [0x00, 0x48, 0x8B, 0x05, 0x11, 0x48, 0x8B, 0x0D, 0x48];
        let p = Pattern::parse("48 8B ??").unwrap();
        assert_eq!(p.find_all(&hay, 10), vec![1, 5]);
        assert_eq!(p.find_all(&hay, 1), vec![1]);
        let lead = Pattern::parse("?? 8B 0D").unwrap();
        assert_eq!(lead.find_all(&hay, 10), vec![5]);
        // A match ending exactly at the end of the slice.
        let tail = Pattern::parse("0D 48").unwrap();
        assert_eq!(tail.find_all(&hay, 10), vec![7]);
        // Longer than the slice.
        assert!(
            Pattern::parse("48 8B 05 11 48 8B 0D 48 00 00")
                .unwrap()
                .find_all(&hay, 10)
                .is_empty()
        );
    }

    #[test]
    fn overlapping_matches_are_all_found() {
        let hay = [0xAA; 5];
        assert_eq!(
            Pattern::parse("AA AA").unwrap().find_all(&hay, 10),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn hex_string_formats() {
        assert_eq!(hex_string(&[0x29, 0xD0]), "29 D0");
    }
}
