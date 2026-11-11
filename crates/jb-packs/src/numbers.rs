//! Converting pack numbers (JSON) to typed game values, with range checks.

use jb_core::{Value, ValueType};
use serde_json::Number;

/// A number that is exact for every integer type up to 64 bits.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Num {
    Int(i128),
    Float(f64),
}

impl Num {
    pub fn from_json(n: &Number) -> Num {
        if let Some(i) = n.as_i64() {
            Num::Int(i128::from(i))
        } else if let Some(u) = n.as_u64() {
            Num::Int(i128::from(u))
        } else {
            Num::Float(n.as_f64().unwrap_or(f64::NAN))
        }
    }

    pub fn from_value(v: &Value) -> Num {
        match *v {
            Value::I8(x) => Num::Int(x.into()),
            Value::U8(x) => Num::Int(x.into()),
            Value::I16(x) => Num::Int(x.into()),
            Value::U16(x) => Num::Int(x.into()),
            Value::I32(x) => Num::Int(x.into()),
            Value::U32(x) => Num::Int(x.into()),
            Value::I64(x) => Num::Int(x.into()),
            Value::U64(x) => Num::Int(x.into()),
            Value::Ptr(x) => Num::Int(x.into()),
            Value::F32(x) => Num::Float(x.into()),
            Value::F64(x) => Num::Float(x),
        }
    }

    pub fn as_f64(self) -> f64 {
        match self {
            Num::Int(i) => i as f64,
            Num::Float(f) => f,
        }
    }

    /// The integer, if this is one (floats with no fraction count).
    pub fn as_int(self) -> Option<i128> {
        match self {
            Num::Int(i) => Some(i),
            Num::Float(f) if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.0e15 => {
                Some(f as i128)
            }
            Num::Float(_) => None,
        }
    }

    /// Compares two numbers, exactly when both are integers.
    pub fn cmp_num(self, other: Num) -> Option<std::cmp::Ordering> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => Some(a.cmp(&b)),
            _ => self.as_f64().partial_cmp(&other.as_f64()),
        }
    }
}

/// Integer range of a type, or `None` for floats. `ptr` is not a pack value type.
fn int_range(ty: ValueType) -> Option<(i128, i128)> {
    Some(match ty {
        ValueType::I8 => (i8::MIN.into(), i8::MAX.into()),
        ValueType::U8 => (0, u8::MAX.into()),
        ValueType::I16 => (i16::MIN.into(), i16::MAX.into()),
        ValueType::U16 => (0, u16::MAX.into()),
        ValueType::I32 => (i32::MIN.into(), i32::MAX.into()),
        ValueType::U32 => (0, u32::MAX.into()),
        ValueType::I64 => (i64::MIN.into(), i64::MAX.into()),
        ValueType::U64 | ValueType::Ptr => (0, u64::MAX.into()),
        ValueType::F32 | ValueType::F64 => return None,
    })
}

pub fn is_float(ty: ValueType) -> bool {
    matches!(ty, ValueType::F32 | ValueType::F64)
}

fn int_value(ty: ValueType, i: i128) -> Option<Value> {
    Some(match ty {
        ValueType::I8 => Value::I8(i.try_into().ok()?),
        ValueType::U8 => Value::U8(i.try_into().ok()?),
        ValueType::I16 => Value::I16(i.try_into().ok()?),
        ValueType::U16 => Value::U16(i.try_into().ok()?),
        ValueType::I32 => Value::I32(i.try_into().ok()?),
        ValueType::U32 => Value::U32(i.try_into().ok()?),
        ValueType::I64 => Value::I64(i.try_into().ok()?),
        ValueType::U64 => Value::U64(i.try_into().ok()?),
        ValueType::Ptr => Value::Ptr(i.try_into().ok()?),
        ValueType::F32 | ValueType::F64 => return None,
    })
}

/// Converts a number to `ty`. `None` if it doesn't fit (or isn't an integer
/// for an integer type).
pub fn to_value(ty: ValueType, n: Num) -> Option<Value> {
    match ty {
        ValueType::F32 => {
            let f = n.as_f64() as f32;
            f.is_finite().then_some(Value::F32(f))
        }
        ValueType::F64 => {
            let f = n.as_f64();
            f.is_finite().then_some(Value::F64(f))
        }
        _ => int_value(ty, n.as_int()?),
    }
}

/// `current + delta`, clamped to the type's range (integers saturate).
pub fn add(current: &Value, delta: Num) -> Option<Value> {
    let ty = current.ty();
    match int_range(ty) {
        Some((lo, hi)) => {
            let cur = Num::from_value(current).as_int()?;
            int_value(ty, cur.saturating_add(delta.as_int()?).clamp(lo, hi))
        }
        None => to_value(
            ty,
            Num::Float(Num::from_value(current).as_f64() + delta.as_f64()),
        ),
    }
}

/// True if `v` is within the optional bounds.
pub fn in_bounds(v: Num, min: Option<Num>, max: Option<Num>) -> bool {
    use std::cmp::Ordering::*;
    let ge = min.is_none_or(|m| matches!(v.cmp_num(m), Some(Greater | Equal)));
    let le = max.is_none_or(|m| matches!(v.cmp_num(m), Some(Less | Equal)));
    ge && le
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> Num {
        Num::from_json(&serde_json::from_str::<Number>(s).unwrap())
    }

    #[test]
    fn converts_with_range_checks() {
        assert_eq!(
            to_value(ValueType::I32, n("9999999")),
            Some(Value::I32(9_999_999))
        );
        assert_eq!(to_value(ValueType::U8, n("256")), None);
        assert_eq!(to_value(ValueType::U8, n("-1")), None);
        assert_eq!(to_value(ValueType::I32, n("1.5")), None);
        assert_eq!(to_value(ValueType::I32, n("2.0")), Some(Value::I32(2)));
        assert_eq!(to_value(ValueType::F32, n("2.5")), Some(Value::F32(2.5)));
        assert_eq!(to_value(ValueType::F32, n("1e300")), None);
        assert_eq!(
            to_value(ValueType::U64, n("18446744073709551615")),
            Some(Value::U64(u64::MAX))
        );
    }

    #[test]
    fn add_saturates() {
        assert_eq!(add(&Value::I32(250), n("1000")), Some(Value::I32(1250)));
        assert_eq!(
            add(&Value::I32(i32::MAX - 1), n("1000")),
            Some(Value::I32(i32::MAX))
        );
        assert_eq!(add(&Value::U8(5), n("-10")), Some(Value::U8(0)));
        assert_eq!(add(&Value::F32(1.0), n("0.5")), Some(Value::F32(1.5)));
        assert_eq!(add(&Value::I32(1), n("0.5")), None);
    }

    #[test]
    fn bounds() {
        assert!(in_bounds(n("5"), Some(n("0")), Some(n("10"))));
        assert!(in_bounds(n("0"), Some(n("0")), None));
        assert!(!in_bounds(n("11"), None, Some(n("10"))));
        assert!(in_bounds(n("0.5"), Some(n("0.5")), Some(n("5"))));
        assert!(!in_bounds(n("0.49"), Some(n("0.5")), Some(n("5"))));
    }
}
