//! "Find a value" for players who don't know value types: searches whole
//! numbers (`i32`) and decimals (`f32`, `f64`) at once, using the number as
//! the game shows it, and narrows all of them together.
//!
//! A shown number may be rounded, so decimals match within one unit of its
//! last digit ("88" finds 87.6; "12.5" finds 12.46). A number with no value
//! on screen (a health bar) starts as unknown, then goes up / down / same.

use serde::Serialize;

use crate::error::{Error, Result};
use crate::scan::{FirstScan, NextScan, Scan, ScanControl, ScanOptions};
use crate::session::AttachedProcess;
use crate::value::{Value, ValueType};

/// Types searched for a number typed by the player, in the order results
/// are offered (whole numbers are the most common).
const EXACT_TYPES: [ValueType; 3] = [ValueType::I32, ValueType::F32, ValueType::F64];
/// Types searched when there's no number to type. One 4-byte snapshot is
/// taken and read both ways, so memory isn't copied twice.
const UNKNOWN_TYPES: [ValueType; 2] = [ValueType::I32, ValueType::F32];

/// How to start.
#[derive(Debug, Clone, PartialEq)]
pub enum FindStart {
    /// The number shown in the game, as typed (`"1,250"`, `"87.5"`).
    Number(String),
    /// Nothing to type: remember everything, then compare.
    Unknown,
}

/// How to narrow.
#[derive(Debug, Clone, PartialEq)]
pub enum FindNext {
    Number(String),
    Increased,
    Decreased,
    Unchanged,
    Changed,
}

/// One place the value might be.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundValue {
    pub address: u64,
    pub ty: ValueType,
    /// Value as of the last search.
    pub value: Value,
}

/// A search over several value types at once.
#[derive(Debug)]
pub struct ValueFinder {
    scans: Vec<Scan>,
    /// The unknown-value snapshot, until the first comparison splits it.
    snapshot: Option<Scan>,
    rounds: u32,
}

impl ValueFinder {
    pub fn start(
        process: &AttachedProcess,
        start: FindStart,
        control: &ScanControl,
    ) -> Result<ValueFinder> {
        let options = ScanOptions::default();
        match start {
            FindStart::Unknown => Ok(ValueFinder {
                scans: Vec::new(),
                snapshot: Some(Scan::first(
                    process,
                    UNKNOWN_TYPES[0],
                    FirstScan::Unknown,
                    options,
                    control,
                )?),
                rounds: 1,
            }),
            FindStart::Number(text) => {
                let n = Number::parse(&text)?;
                let mut scans = Vec::new();
                for ty in EXACT_TYPES {
                    if let Some(mode) = n.first(ty) {
                        scans.push(Scan::first(process, ty, mode, options, control)?);
                    }
                }
                Ok(ValueFinder {
                    scans,
                    snapshot: None,
                    rounds: 1,
                })
            }
        }
    }

    /// Narrows every type. On error or cancel the previous results are kept.
    pub fn next(
        &mut self,
        process: &AttachedProcess,
        next: FindNext,
        control: &ScanControl,
    ) -> Result<()> {
        let number = match &next {
            FindNext::Number(text) => Some(Number::parse(text)?),
            _ => None,
        };
        let mode_for = |ty: ValueType| -> Option<NextScan> {
            match (&next, &number) {
                (FindNext::Number(_), Some(n)) => n.next(ty),
                (FindNext::Increased, _) => Some(NextScan::Increased),
                (FindNext::Decreased, _) => Some(NextScan::Decreased),
                (FindNext::Unchanged, _) => Some(NextScan::Unchanged),
                (FindNext::Changed, _) => Some(NextScan::Changed),
                _ => None,
            }
        };
        let mut narrowed = Vec::new();
        if let Some(snapshot) = &self.snapshot {
            for ty in UNKNOWN_TYPES {
                if let Some(mode) = mode_for(ty) {
                    narrowed.push(snapshot.next_as(process, ty, mode, control)?);
                }
            }
        } else {
            for scan in &self.scans {
                // A type the number can't be (a decimal for whole numbers)
                // drops out.
                if let Some(mode) = mode_for(scan.value_type()) {
                    narrowed.push(scan.next_as(process, scan.value_type(), mode, control)?);
                }
            }
        }
        // Only replace the results once every type has finished.
        self.snapshot = None;
        self.scans = narrowed;
        self.rounds += 1;
        Ok(())
    }

    /// Places left (all types). An unknown start counts every slot once.
    pub fn count(&self) -> usize {
        match &self.snapshot {
            Some(s) => s.count(),
            None => self.scans.iter().map(Scan::count).sum(),
        }
    }

    pub fn rounds(&self) -> u32 {
        self.rounds
    }

    /// Up to `limit` places, whole numbers first. Empty before the first
    /// comparison of an unknown start (everything is still possible).
    pub fn hits(&self, limit: usize) -> Vec<FoundValue> {
        let mut out = Vec::new();
        for scan in &self.scans {
            if out.len() >= limit {
                break;
            }
            for h in scan.hits(0, limit - out.len()) {
                out.push(FoundValue {
                    address: h.address,
                    ty: scan.value_type(),
                    value: h.value,
                });
            }
        }
        out
    }
}

/// A number as the player typed it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Number {
    value: f64,
    /// Whole numbers only (no decimal point).
    whole: Option<i64>,
    /// One unit of the last digit typed: 1 for "88", 0.1 for "12.5".
    unit: f64,
}

impl Number {
    /// Accepts `1250`, `1,250`, `1 250`, `-3`, `87.5`. Commas are thousands
    /// separators; the decimal mark is a dot.
    fn parse(text: &str) -> Result<Number> {
        let clean: String = text
            .chars()
            .filter(|c| !c.is_whitespace() && *c != ',' && *c != '_')
            .collect();
        let bad = || Error::InvalidValue {
            input: text.to_string(),
            ty: ValueType::F64,
        };
        let value: f64 = clean.parse().map_err(|_| bad())?;
        if !value.is_finite() {
            return Err(bad());
        }
        let decimals = clean.split_once('.').map_or(0, |(_, d)| d.len()).min(9);
        Ok(Number {
            value,
            whole: if clean.contains('.') {
                None
            } else {
                clean.parse().ok()
            },
            unit: 10f64.powi(-(decimals as i32)),
        })
    }

    fn range(&self, ty: ValueType) -> Option<(Value, Value)> {
        let (lo, hi) = (self.value - self.unit, self.value + self.unit);
        match ty {
            ValueType::F32 => Some((Value::F32(lo as f32), Value::F32(hi as f32))),
            ValueType::F64 => Some((Value::F64(lo), Value::F64(hi))),
            _ => None,
        }
    }

    fn whole_value(&self, ty: ValueType) -> Option<Value> {
        match ty {
            ValueType::I32 => i32::try_from(self.whole?).ok().map(Value::I32),
            _ => None,
        }
    }

    fn first(&self, ty: ValueType) -> Option<FirstScan> {
        if let Some(v) = self.whole_value(ty) {
            return Some(FirstScan::Exact(v));
        }
        self.range(ty).map(|(a, b)| FirstScan::Between(a, b))
    }

    fn next(&self, ty: ValueType) -> Option<NextScan> {
        if let Some(v) = self.whole_value(ty) {
            return Some(NextScan::Exact(v));
        }
        self.range(ty).map(|(a, b)| NextScan::Between(a, b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_as_players_type_them() {
        let n = Number::parse(" 1,250 ").unwrap();
        assert_eq!(n.whole, Some(1250));
        assert_eq!(n.unit, 1.0);
        assert_eq!(
            n.first(ValueType::I32),
            Some(FirstScan::Exact(Value::I32(1250)))
        );
        assert_eq!(
            n.first(ValueType::F64),
            Some(FirstScan::Between(Value::F64(1249.0), Value::F64(1251.0)))
        );

        let d = Number::parse("12.5").unwrap();
        assert_eq!(d.whole, None);
        assert!((d.unit - 0.1).abs() < 1e-12);
        // A decimal can't be a whole number.
        assert_eq!(d.first(ValueType::I32), None);
        assert!(matches!(
            d.next(ValueType::F32),
            Some(NextScan::Between(..))
        ));

        assert_eq!(Number::parse("-3").unwrap().whole, Some(-3));
        // Too big for i32: only the decimal types are searched.
        assert_eq!(
            Number::parse("5000000000").unwrap().first(ValueType::I32),
            None
        );
        assert!(Number::parse("abc").is_err());
        assert!(Number::parse("").is_err());
        assert!(Number::parse("inf").is_err());
    }
}
