//! Unreal Engine saves (`.sav`, "GVAS"). The property tree is read from the
//! bytes; nothing is run. Numbers and true/false values can be edited in
//! place (same size, so nothing else in the file moves). Text, structs the
//! engine stores in its own binary form, maps and sets are shown read-only or
//! as "N bytes".
//!
//! Supports the property format used up to UE 5.3. Saves from UE 5.4+ (new
//! property tags) fall back to the hex view.

use super::json::Spot;
use super::{Field, FieldKind, MAX_FIELDS};

/// UE5 package version that changed property tags (UE 5.4).
const UE5_COMPLETE_TYPE_NAME: i32 = 1012;
const MAX_STRING: usize = 1 << 20;
const MAX_DEPTH: usize = 64;

/// Structs Unreal writes as raw binary rather than as a property list.
const NATIVE_STRUCTS: &[&str] = &[
    "Vector",
    "Vector2D",
    "Vector4",
    "Rotator",
    "Quat",
    "LinearColor",
    "Color",
    "Guid",
    "DateTime",
    "Timespan",
    "IntPoint",
    "IntVector",
    "Box",
    "Box2D",
    "Transform",
    "Plane",
    "Matrix",
    "SoftObjectPath",
    "SoftClassPath",
    "GameplayTagContainer",
    "UniqueNetIdRepl",
];

struct R<'a> {
    b: &'a [u8],
    i: usize,
    end: usize,
}

type Res<T> = Result<T, String>;

impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Res<&'a [u8]> {
        if n > self.end.saturating_sub(self.i) {
            return Err(format!("unexpected end at byte {}", self.i));
        }
        let s = &self.b[self.i..self.i + n];
        self.i += n;
        Ok(s)
    }
    fn u8(&mut self) -> Res<u8> {
        Ok(self.take(1)?[0])
    }
    fn i32(&mut self) -> Res<i32> {
        let t = self.take(4)?;
        Ok(i32::from_le_bytes([t[0], t[1], t[2], t[3]]))
    }
    fn fstring(&mut self) -> Res<String> {
        let len = self.i32()?;
        if len == 0 {
            return Ok(String::new());
        }
        if len > 0 {
            let n = len as usize;
            if n > MAX_STRING {
                return Err("string too long".into());
            }
            let raw = self.take(n)?;
            let raw = raw.strip_suffix(&[0]).unwrap_or(raw);
            return Ok(String::from_utf8_lossy(raw).into_owned());
        }
        let units = len.unsigned_abs() as usize;
        if units > MAX_STRING {
            return Err("string too long".into());
        }
        let raw = self.take(units * 2)?;
        let mut w: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        if w.last() == Some(&0) {
            w.pop();
        }
        Ok(String::from_utf16_lossy(&w))
    }
    fn guid_flag(&mut self) -> Res<()> {
        if self.u8()? != 0 {
            self.take(16)?;
        }
        Ok(())
    }
}

fn fixed_size(ty: &str) -> Option<usize> {
    Some(match ty {
        "Int8Property" | "ByteProperty" | "BoolProperty" => 1,
        "Int16Property" | "UInt16Property" => 2,
        "IntProperty" | "UInt32Property" | "FloatProperty" => 4,
        "Int64Property" | "UInt64Property" | "DoubleProperty" => 8,
        _ => return None,
    })
}

fn read_number(ty: &str, b: &[u8]) -> Option<(FieldKind, String)> {
    let a = |n: usize| -> Option<[u8; 8]> {
        let mut v = [0u8; 8];
        v[..n].copy_from_slice(b.get(..n)?);
        Some(v)
    };
    Some(match ty {
        "Int8Property" => (FieldKind::Int, (b.first()?.cast_signed()).to_string()),
        "ByteProperty" => (FieldKind::Int, b.first()?.to_string()),
        "BoolProperty" => (FieldKind::Bool, (*b.first()? != 0).to_string()),
        "Int16Property" => (
            FieldKind::Int,
            i16::from_le_bytes([b[0], *b.get(1)?]).to_string(),
        ),
        "UInt16Property" => (
            FieldKind::Int,
            u16::from_le_bytes([b[0], *b.get(1)?]).to_string(),
        ),
        "IntProperty" => {
            let v = a(4)?;
            (
                FieldKind::Int,
                i32::from_le_bytes([v[0], v[1], v[2], v[3]]).to_string(),
            )
        }
        "UInt32Property" => {
            let v = a(4)?;
            (
                FieldKind::Int,
                u32::from_le_bytes([v[0], v[1], v[2], v[3]]).to_string(),
            )
        }
        "FloatProperty" => {
            let v = a(4)?;
            (
                FieldKind::Float,
                f32::from_le_bytes([v[0], v[1], v[2], v[3]]).to_string(),
            )
        }
        "Int64Property" => (FieldKind::Int, i64::from_le_bytes(a(8)?).to_string()),
        "UInt64Property" => (FieldKind::Int, u64::from_le_bytes(a(8)?).to_string()),
        "DoubleProperty" => (FieldKind::Float, f64::from_le_bytes(a(8)?).to_string()),
        _ => return None,
    })
}

/// The bytes for `value` in a field of Unreal type `ty`.
pub(crate) fn encode(ty: &str, value: &str) -> Result<Vec<u8>, String> {
    let v = value.trim();
    let whole = || format!("{value:?} isn't a whole number that fits here");
    let num = || format!("{value:?} isn't a number");
    Ok(match ty {
        "Int8Property" => v.parse::<i8>().map_err(|_| whole())?.to_le_bytes().to_vec(),
        "ByteProperty" => v.parse::<u8>().map_err(|_| whole())?.to_le_bytes().to_vec(),
        "Int16Property" => v
            .parse::<i16>()
            .map_err(|_| whole())?
            .to_le_bytes()
            .to_vec(),
        "UInt16Property" => v
            .parse::<u16>()
            .map_err(|_| whole())?
            .to_le_bytes()
            .to_vec(),
        "IntProperty" => v
            .parse::<i32>()
            .map_err(|_| whole())?
            .to_le_bytes()
            .to_vec(),
        "UInt32Property" => v
            .parse::<u32>()
            .map_err(|_| whole())?
            .to_le_bytes()
            .to_vec(),
        "Int64Property" => v
            .parse::<i64>()
            .map_err(|_| whole())?
            .to_le_bytes()
            .to_vec(),
        "UInt64Property" => v
            .parse::<u64>()
            .map_err(|_| whole())?
            .to_le_bytes()
            .to_vec(),
        "FloatProperty" => {
            let f = v.parse::<f32>().map_err(|_| num())?;
            if !f.is_finite() {
                return Err(num());
            }
            f.to_le_bytes().to_vec()
        }
        "DoubleProperty" => {
            let f = v.parse::<f64>().map_err(|_| num())?;
            if !f.is_finite() {
                return Err(num());
            }
            f.to_le_bytes().to_vec()
        }
        "BoolProperty" => match v.to_ascii_lowercase().as_str() {
            "true" | "1" => vec![1],
            "false" | "0" => vec![0],
            _ => return Err(format!("{value:?} isn't true or false")),
        },
        _ => return Err("this value can't be edited".into()),
    })
}

struct Out {
    spots: Vec<Spot>,
    truncated: bool,
}

impl Out {
    fn push(
        &mut self,
        path: String,
        kind: FieldKind,
        value: String,
        ty: &str,
        span: Option<(usize, usize)>,
    ) {
        if self.spots.len() >= MAX_FIELDS {
            self.truncated = true;
            return;
        }
        let (start, end, editable) = match span {
            Some((s, e)) => (s, e, true),
            None => (0, 0, false),
        };
        self.spots.push(Spot {
            field: Field {
                path,
                kind,
                value,
                editable,
                note: Some(ty.to_string()),
            },
            start,
            end,
        });
    }

    fn opaque(&mut self, path: String, ty: &str, size: usize) {
        self.push(path, FieldKind::Other, format!("{size} bytes"), ty, None);
    }
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

/// Reads properties until `None` (or the end of `r`).
fn properties(r: &mut R<'_>, prefix: &str, out: &mut Out, depth: usize) -> Res<()> {
    if depth > MAX_DEPTH {
        return Err("nested too deeply".into());
    }
    while r.i < r.end {
        let name = r.fstring()?;
        if name == "None" {
            return Ok(());
        }
        let ty = r.fstring()?;
        let size = usize::try_from(r.i32()?).map_err(|_| "negative size".to_string())?;
        let index = r.i32()?;
        let mut path = join(prefix, &name);
        if index != 0 {
            path = format!("{path}[{index}]");
        }
        let mut header = String::new();
        let mut inner = String::new();
        let mut bool_at = None;
        match ty.as_str() {
            "StructProperty" => {
                header = r.fstring()?;
                r.take(16)?;
                r.guid_flag()?;
            }
            "BoolProperty" => {
                bool_at = Some(r.i);
                r.u8()?;
                r.guid_flag()?;
            }
            "ByteProperty" | "EnumProperty" => {
                header = r.fstring()?;
                r.guid_flag()?;
            }
            "ArrayProperty" | "SetProperty" => {
                inner = r.fstring()?;
                r.guid_flag()?;
            }
            "MapProperty" => {
                r.fstring()?;
                r.fstring()?;
                r.guid_flag()?;
            }
            _ => r.guid_flag()?,
        }
        let start = r.i;
        r.take(size)?;
        let end = r.i;
        if let Some(at) = bool_at {
            let v = (r.b[at] != 0).to_string();
            out.push(path, FieldKind::Bool, v, "BoolProperty", Some((at, at + 1)));
            continue;
        }
        let mark = out.spots.len();
        let mut sub = R {
            b: r.b,
            i: start,
            end,
        };
        let ok = value(&mut sub, &path, &ty, &header, &inner, size, out, depth);
        if ok.is_err() {
            out.spots.truncate(mark);
            out.opaque(path, &ty, size);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn value(
    r: &mut R<'_>,
    path: &str,
    ty: &str,
    header: &str,
    inner: &str,
    size: usize,
    out: &mut Out,
    depth: usize,
) -> Res<()> {
    let start = r.i;
    if ty == "ByteProperty" && header != "None" && !header.is_empty() {
        let v = r.fstring()?;
        out.push(path.to_string(), FieldKind::Text, v, ty, None);
        return Ok(());
    }
    if let Some(n) = fixed_size(ty) {
        if n != size {
            return Err("unexpected size".into());
        }
        let (kind, v) = read_number(ty, r.take(n)?).ok_or("bad number")?;
        out.push(path.to_string(), kind, v, ty, Some((start, start + n)));
        return Ok(());
    }
    match ty {
        "StrProperty" | "NameProperty" | "EnumProperty" | "ObjectProperty" => {
            let v = r.fstring()?;
            out.push(path.to_string(), FieldKind::Text, v, ty, None);
        }
        "StructProperty" => {
            if NATIVE_STRUCTS.contains(&header) {
                out.opaque(path.to_string(), header, size);
            } else {
                properties(r, path, out, depth + 1)?;
            }
        }
        "ArrayProperty" => array(r, path, inner, size, out, depth)?,
        _ => out.opaque(path.to_string(), ty, size),
    }
    Ok(())
}

fn array(
    r: &mut R<'_>,
    path: &str,
    inner: &str,
    size: usize,
    out: &mut Out,
    depth: usize,
) -> Res<()> {
    let count = usize::try_from(r.i32()?).map_err(|_| "negative count".to_string())?;
    if count > 1_000_000 {
        return Err("too many elements".into());
    }
    if let Some(n) = fixed_size(inner) {
        if 4 + count * n != size {
            return Err("unexpected array size".into());
        }
        for k in 0..count {
            let at = r.i;
            let (kind, v) = read_number(inner, r.take(n)?).ok_or("bad number")?;
            out.push(format!("{path}[{k}]"), kind, v, inner, Some((at, at + n)));
        }
        return Ok(());
    }
    match inner {
        "StrProperty" | "NameProperty" | "EnumProperty" | "ObjectProperty" => {
            for k in 0..count {
                let v = r.fstring()?;
                out.push(format!("{path}[{k}]"), FieldKind::Text, v, inner, None);
            }
        }
        "StructProperty" => {
            r.fstring()?; // element name
            r.fstring()?; // "StructProperty"
            r.i32()?;
            r.i32()?;
            let struct_name = r.fstring()?;
            r.take(16)?;
            r.guid_flag()?;
            if NATIVE_STRUCTS.contains(&struct_name.as_str()) {
                out.opaque(path.to_string(), &struct_name, size);
                return Ok(());
            }
            for k in 0..count {
                properties(r, &format!("{path}[{k}]"), out, depth + 1)?;
            }
        }
        _ => out.opaque(path.to_string(), inner, size),
    }
    Ok(())
}

/// Every property, or why the file can't be read.
pub(crate) fn parse(b: &[u8]) -> Result<(Vec<Spot>, bool), String> {
    let mut r = R {
        b,
        i: 0,
        end: b.len(),
    };
    if r.take(4)? != b"GVAS" {
        return Err("not an Unreal save".into());
    }
    let save_version = r.i32()?;
    r.i32()?; // package version (UE4)
    if save_version >= 3 {
        let ue5 = r.i32()?;
        if ue5 >= UE5_COMPLETE_TYPE_NAME {
            return Err("saves from Unreal Engine 5.4 and newer aren't supported yet".into());
        }
    }
    r.take(6)?; // engine major, minor, patch
    r.take(4)?; // changelist
    r.fstring()?; // branch
    r.i32()?; // custom version format
    let versions = usize::try_from(r.i32()?).map_err(|_| "bad version count".to_string())?;
    if versions > 10_000 {
        return Err("bad version count".into());
    }
    r.take(versions * 20)?;
    r.fstring()?; // save game class
    let mut out = Out {
        spots: Vec::new(),
        truncated: false,
    };
    properties(&mut r, "", &mut out, 0)?;
    Ok((out.spots, out.truncated))
}

pub(crate) fn set(b: &[u8], path: &str, value: &str) -> Result<Vec<u8>, String> {
    let (spots, _) = parse(b)?;
    let spot = spots
        .iter()
        .find(|s| s.field.path == path)
        .ok_or_else(|| format!("no value at {path}"))?;
    if !spot.field.editable {
        return Err(format!("{path} can't be edited"));
    }
    let ty = spot.field.note.as_deref().unwrap_or_default();
    let bytes = encode(ty, value)?;
    if bytes.len() != spot.end - spot.start {
        return Err("size mismatch".into());
    }
    let mut out = b.to_vec();
    out[spot.start..spot.end].copy_from_slice(&bytes);
    Ok(out)
}

/// Writes small GVAS files for tests (our own writer, as PLAN.md §7 asks).
#[cfg(test)]
pub(crate) mod writer {
    pub fn fstring(out: &mut Vec<u8>, s: &str) {
        if s.is_empty() {
            out.extend_from_slice(&0i32.to_le_bytes());
            return;
        }
        out.extend_from_slice(&((s.len() + 1) as i32).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
        out.push(0);
    }

    pub fn header(out: &mut Vec<u8>) {
        out.extend_from_slice(b"GVAS");
        out.extend_from_slice(&2i32.to_le_bytes());
        out.extend_from_slice(&522i32.to_le_bytes());
        for v in [4u16, 27, 2] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&123u32.to_le_bytes());
        fstring(out, "++UE4+Release-4.27");
        out.extend_from_slice(&3i32.to_le_bytes());
        out.extend_from_slice(&1i32.to_le_bytes());
        out.extend_from_slice(&[7u8; 16]);
        out.extend_from_slice(&1i32.to_le_bytes());
        fstring(out, "/Script/TestGame.TestSave");
    }

    /// A property with a plain `has_guid = 0` header.
    pub fn prop(out: &mut Vec<u8>, name: &str, ty: &str, value: &[u8]) {
        fstring(out, name);
        fstring(out, ty);
        out.extend_from_slice(&(value.len() as i32).to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes());
        out.push(0);
        out.extend_from_slice(value);
    }

    pub fn bool_prop(out: &mut Vec<u8>, name: &str, v: bool) {
        fstring(out, name);
        fstring(out, "BoolProperty");
        out.extend_from_slice(&0i32.to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes());
        out.push(u8::from(v));
        out.push(0);
    }

    pub fn struct_prop(out: &mut Vec<u8>, name: &str, struct_name: &str, body: &[u8]) {
        fstring(out, name);
        fstring(out, "StructProperty");
        out.extend_from_slice(&(body.len() as i32).to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes());
        fstring(out, struct_name);
        out.extend_from_slice(&[0u8; 16]);
        out.push(0);
        out.extend_from_slice(body);
    }

    pub fn int_array(out: &mut Vec<u8>, name: &str, values: &[i32]) {
        let mut body = (values.len() as i32).to_le_bytes().to_vec();
        for v in values {
            body.extend_from_slice(&v.to_le_bytes());
        }
        fstring(out, name);
        fstring(out, "ArrayProperty");
        out.extend_from_slice(&(body.len() as i32).to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes());
        fstring(out, "IntProperty");
        out.push(0);
        out.extend_from_slice(&body);
    }

    pub fn none(out: &mut Vec<u8>) {
        fstring(out, "None");
    }

    /// A save with a bit of everything.
    pub fn sample() -> Vec<u8> {
        let mut b = Vec::new();
        header(&mut b);
        prop(&mut b, "Gold", "IntProperty", &250i32.to_le_bytes());
        prop(&mut b, "Health", "FloatProperty", &75.5f32.to_le_bytes());
        bool_prop(&mut b, "HardMode", false);
        let mut name = Vec::new();
        fstring(&mut name, "Tester");
        prop(&mut b, "PlayerName", "StrProperty", &name);
        let mut stats = Vec::new();
        prop(&mut stats, "Level", "IntProperty", &7i32.to_le_bytes());
        prop(&mut stats, "Xp", "Int64Property", &123_456i64.to_le_bytes());
        none(&mut stats);
        struct_prop(&mut b, "Stats", "PlayerStats", &stats);
        let mut pos = Vec::new();
        for f in [1.0f32, 2.0, 3.0] {
            pos.extend_from_slice(&f.to_le_bytes());
        }
        struct_prop(&mut b, "Position", "Vector", &pos);
        int_array(&mut b, "Keys", &[1, 2, 3]);
        prop(&mut b, "Blob", "TextProperty", &[9, 9, 9, 9, 9]);
        none(&mut b);
        b.extend_from_slice(&[0, 0, 0, 0]);
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(b: &[u8]) -> Vec<(String, FieldKind, String, bool)> {
        parse(b)
            .unwrap()
            .0
            .into_iter()
            .map(|s| (s.field.path, s.field.kind, s.field.value, s.field.editable))
            .collect()
    }

    #[test]
    fn reads_the_property_tree() {
        let f = fields(&writer::sample());
        let names: Vec<_> = f.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Gold",
                "Health",
                "HardMode",
                "PlayerName",
                "Stats.Level",
                "Stats.Xp",
                "Position",
                "Keys[0]",
                "Keys[1]",
                "Keys[2]",
                "Blob"
            ]
        );
        assert_eq!(f[0], ("Gold".into(), FieldKind::Int, "250".into(), true));
        assert_eq!(f[1].2, "75.5");
        assert_eq!(
            f[2],
            ("HardMode".into(), FieldKind::Bool, "false".into(), true)
        );
        assert_eq!(
            f[3],
            ("PlayerName".into(), FieldKind::Text, "Tester".into(), false)
        );
        assert_eq!(f[5].2, "123456");
        assert_eq!(
            f[6],
            (
                "Position".into(),
                FieldKind::Other,
                "12 bytes".into(),
                false
            )
        );
        assert_eq!(f[9].2, "3");
        assert_eq!(f[10].1, FieldKind::Other);
    }

    #[test]
    fn edits_in_place() {
        let s = writer::sample();
        let out = set(&s, "Gold", "999999").unwrap();
        assert_eq!(out.len(), s.len());
        let diff: Vec<usize> = (0..s.len()).filter(|&i| s[i] != out[i]).collect();
        assert!(diff.len() <= 4 && !diff.is_empty());
        assert_eq!(fields(&out)[0].2, "999999");
        let out = set(&out, "Stats.Level", "99").unwrap();
        let out = set(&out, "HardMode", "true").unwrap();
        let out = set(&out, "Keys[1]", "-5").unwrap();
        let out = set(&out, "Health", "100").unwrap();
        let f = fields(&out);
        assert_eq!(f[4].2, "99");
        assert_eq!(f[2].2, "true");
        assert_eq!(f[8].2, "-5");
        assert_eq!(f[1].2, "100");
        assert!(set(&s, "PlayerName", "x").is_err());
        assert!(set(&s, "Gold", "99999999999").is_err());
        assert!(set(&s, "Health", "NaN").is_err());
    }

    #[test]
    fn damaged_or_new_saves_are_refused() {
        let s = writer::sample();
        assert!(parse(&s[..s.len() / 2]).is_err());
        assert!(parse(b"GVAX").is_err());
        let mut ue54 = b"GVAS".to_vec();
        ue54.extend_from_slice(&3i32.to_le_bytes());
        ue54.extend_from_slice(&522i32.to_le_bytes());
        ue54.extend_from_slice(&1012i32.to_le_bytes());
        assert!(parse(&ue54).unwrap_err().contains("5.4"));
    }
}
