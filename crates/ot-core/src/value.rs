//! Typed values read from and written to target memory (little-endian).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::process::Arch;

/// The type of a value in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValueType {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
    /// A pointer: 4 bytes on x86 targets, 8 on x64.
    Ptr,
}

impl ValueType {
    pub const ALL: [ValueType; 11] = [
        ValueType::I8,
        ValueType::U8,
        ValueType::I16,
        ValueType::U16,
        ValueType::I32,
        ValueType::U32,
        ValueType::I64,
        ValueType::U64,
        ValueType::F32,
        ValueType::F64,
        ValueType::Ptr,
    ];

    /// Size in bytes in a target with the given architecture.
    pub fn size(self, arch: Arch) -> usize {
        match self {
            ValueType::I8 | ValueType::U8 => 1,
            ValueType::I16 | ValueType::U16 => 2,
            ValueType::I32 | ValueType::U32 | ValueType::F32 => 4,
            ValueType::I64 | ValueType::U64 | ValueType::F64 => 8,
            ValueType::Ptr => arch.pointer_size(),
        }
    }

    /// Parses user input into a value of this type. Integers accept decimal or
    /// `0x` hex; pointers accept the same and must fit the target.
    pub fn parse(self, input: &str, arch: Arch) -> Result<Value> {
        let s = input.trim();
        let bad = || Error::InvalidValue {
            input: input.to_string(),
            ty: self,
        };
        let int = |s: &str| -> Option<i128> {
            let (neg, body) = match s.strip_prefix('-') {
                Some(rest) => (true, rest),
                None => (false, s),
            };
            let body = body.replace('_', "");
            let v = match body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
                Some(hex) => i128::from_str_radix(hex, 16).ok()?,
                None => body.parse::<i128>().ok()?,
            };
            Some(if neg { -v } else { v })
        };
        macro_rules! int_as {
            ($t:ty, $variant:ident) => {
                int(s)
                    .and_then(|v| <$t>::try_from(v).ok())
                    .map(Value::$variant)
                    .ok_or_else(bad)
            };
        }
        match self {
            ValueType::I8 => int_as!(i8, I8),
            ValueType::U8 => int_as!(u8, U8),
            ValueType::I16 => int_as!(i16, I16),
            ValueType::U16 => int_as!(u16, U16),
            ValueType::I32 => int_as!(i32, I32),
            ValueType::U32 => int_as!(u32, U32),
            ValueType::I64 => int_as!(i64, I64),
            ValueType::U64 => int_as!(u64, U64),
            ValueType::F32 => s
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .map(Value::F32)
                .ok_or_else(bad),
            ValueType::F64 => s
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .map(Value::F64)
                .ok_or_else(bad),
            ValueType::Ptr => {
                let v = int(s).and_then(|v| u64::try_from(v).ok()).ok_or_else(bad)?;
                if arch == Arch::X86 && v > u64::from(u32::MAX) {
                    return Err(bad());
                }
                Ok(Value::Ptr(v))
            }
        }
    }
}

impl fmt::Display for ValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        f.write_str(&s)
    }
}

/// A typed value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
pub enum Value {
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F32(f32),
    F64(f64),
    Ptr(u64),
}

impl Value {
    pub fn ty(&self) -> ValueType {
        match self {
            Value::I8(_) => ValueType::I8,
            Value::U8(_) => ValueType::U8,
            Value::I16(_) => ValueType::I16,
            Value::U16(_) => ValueType::U16,
            Value::I32(_) => ValueType::I32,
            Value::U32(_) => ValueType::U32,
            Value::I64(_) => ValueType::I64,
            Value::U64(_) => ValueType::U64,
            Value::F32(_) => ValueType::F32,
            Value::F64(_) => ValueType::F64,
            Value::Ptr(_) => ValueType::Ptr,
        }
    }

    /// Little-endian bytes as stored in a target with `arch`.
    pub fn to_bytes(&self, arch: Arch) -> Vec<u8> {
        match *self {
            Value::I8(v) => v.to_le_bytes().to_vec(),
            Value::U8(v) => v.to_le_bytes().to_vec(),
            Value::I16(v) => v.to_le_bytes().to_vec(),
            Value::U16(v) => v.to_le_bytes().to_vec(),
            Value::I32(v) => v.to_le_bytes().to_vec(),
            Value::U32(v) => v.to_le_bytes().to_vec(),
            Value::I64(v) => v.to_le_bytes().to_vec(),
            Value::U64(v) => v.to_le_bytes().to_vec(),
            Value::F32(v) => v.to_le_bytes().to_vec(),
            Value::F64(v) => v.to_le_bytes().to_vec(),
            Value::Ptr(v) if arch == Arch::X86 => (v as u32).to_le_bytes().to_vec(),
            Value::Ptr(v) => v.to_le_bytes().to_vec(),
        }
    }

    /// Decodes a value from little-endian bytes. `bytes` must be exactly
    /// `ty.size(arch)` long.
    pub fn from_bytes(ty: ValueType, bytes: &[u8], arch: Arch) -> Option<Value> {
        if bytes.len() != ty.size(arch) {
            return None;
        }
        fn arr<const N: usize>(b: &[u8]) -> [u8; N] {
            let mut a = [0u8; N];
            a.copy_from_slice(b);
            a
        }
        Some(match ty {
            ValueType::I8 => Value::I8(i8::from_le_bytes(arr(bytes))),
            ValueType::U8 => Value::U8(bytes[0]),
            ValueType::I16 => Value::I16(i16::from_le_bytes(arr(bytes))),
            ValueType::U16 => Value::U16(u16::from_le_bytes(arr(bytes))),
            ValueType::I32 => Value::I32(i32::from_le_bytes(arr(bytes))),
            ValueType::U32 => Value::U32(u32::from_le_bytes(arr(bytes))),
            ValueType::I64 => Value::I64(i64::from_le_bytes(arr(bytes))),
            ValueType::U64 => Value::U64(u64::from_le_bytes(arr(bytes))),
            ValueType::F32 => Value::F32(f32::from_le_bytes(arr(bytes))),
            ValueType::F64 => Value::F64(f64::from_le_bytes(arr(bytes))),
            ValueType::Ptr if arch == Arch::X86 => {
                Value::Ptr(u64::from(u32::from_le_bytes(arr(bytes))))
            }
            ValueType::Ptr => Value::Ptr(u64::from_le_bytes(arr(bytes))),
        })
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::I8(v) => write!(f, "{v}"),
            Value::U8(v) => write!(f, "{v}"),
            Value::I16(v) => write!(f, "{v}"),
            Value::U16(v) => write!(f, "{v}"),
            Value::I32(v) => write!(f, "{v}"),
            Value::U32(v) => write!(f, "{v}"),
            Value::I64(v) => write!(f, "{v}"),
            Value::U64(v) => write!(f, "{v}"),
            Value::F32(v) => write!(f, "{v}"),
            Value::F64(v) => write!(f, "{v}"),
            Value::Ptr(v) => write!(f, "0x{v:X}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_type() {
        for arch in [Arch::X86, Arch::X64] {
            for ty in ValueType::ALL {
                let v = ty.parse("42", arch).unwrap();
                let bytes = v.to_bytes(arch);
                assert_eq!(bytes.len(), ty.size(arch), "{ty} on {arch}");
                assert_eq!(
                    Value::from_bytes(ty, &bytes, arch),
                    Some(v),
                    "{ty} on {arch}"
                );
            }
        }
    }

    #[test]
    fn parses_hex_negative_and_ranges() {
        assert_eq!(
            ValueType::I32.parse("-5", Arch::X64).unwrap(),
            Value::I32(-5)
        );
        assert_eq!(
            ValueType::U32.parse("0xFF", Arch::X64).unwrap(),
            Value::U32(255)
        );
        assert_eq!(
            ValueType::I32.parse("1_000", Arch::X64).unwrap(),
            Value::I32(1000)
        );
        assert!(ValueType::U8.parse("256", Arch::X64).is_err());
        assert!(ValueType::U16.parse("-1", Arch::X64).is_err());
        assert!(ValueType::I32.parse("abc", Arch::X64).is_err());
        assert!(ValueType::F32.parse("inf", Arch::X64).is_err());
        assert_eq!(
            ValueType::F32.parse("2.5", Arch::X64).unwrap(),
            Value::F32(2.5)
        );
    }

    #[test]
    fn pointer_size_follows_arch() {
        assert_eq!(ValueType::Ptr.size(Arch::X86), 4);
        assert_eq!(ValueType::Ptr.size(Arch::X64), 8);
        assert!(ValueType::Ptr.parse("0x1_0000_0000", Arch::X86).is_err());
        assert!(ValueType::Ptr.parse("0x1_0000_0000", Arch::X64).is_ok());
        assert_eq!(Value::Ptr(0xABC).to_string(), "0xABC");
    }

    #[test]
    fn from_bytes_rejects_wrong_length() {
        assert_eq!(Value::from_bytes(ValueType::I32, &[1, 2], Arch::X64), None);
    }

    #[test]
    fn display_names_are_lowercase() {
        assert_eq!(ValueType::F32.to_string(), "f32");
        assert_eq!(ValueType::Ptr.to_string(), "ptr");
    }
}
