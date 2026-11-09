//! Bethesda saves (`.ess` of Skyrim and Skyrim Special Edition, `.fos` of
//! Fallout 4): the summary at the start of the file (character name, level,
//! place, race), shown read-only. The rest of the file (compressed game
//! state) isn't read or changed.

use super::json::Spot;
use super::{Field, FieldKind};

/// A recognised save: its game and its summary.
pub(crate) struct Summary {
    pub game: &'static str,
    pub spots: Vec<Spot>,
}

const MAGICS: &[(&[u8], &str)] = &[(b"TESV_SAVEGAME", "Skyrim"), (b"FO4_SAVEGAME", "Fallout 4")];

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let end = self.i.checked_add(n)?;
        let s = self.b.get(self.i..end)?;
        self.i = end;
        Some(s)
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    /// A `wstring`: a 16-bit length, then that many bytes of text.
    fn text(&mut self) -> Option<String> {
        let n = usize::from(self.u16()?);
        let raw = self.take(n)?;
        Some(String::from_utf8_lossy(raw).into_owned())
    }
}

pub(crate) fn is_bethesda(b: &[u8]) -> bool {
    MAGICS.iter().any(|(m, _)| b.starts_with(m))
}

/// The summary, or `None` if this isn't a Bethesda save we know.
pub(crate) fn parse(b: &[u8]) -> Option<Summary> {
    let (magic, game) = MAGICS.iter().find(|(m, _)| b.starts_with(m))?;
    let mut r = Reader { b, i: magic.len() };
    let header_size = r.u32()? as usize;
    let header_end = r.i.checked_add(header_size)?;
    if header_end > b.len() {
        return None;
    }
    let version = r.u32()?;
    let save_number = r.u32()?;
    let name = r.text()?;
    let level = r.u32()?;
    let location = r.text()?;
    let play_time = r.text()?;
    let race = r.text()?;
    let sex = r.u16()?;
    let xp = r.f32()?;
    let next_xp = r.f32()?;
    if r.i > header_end {
        return None;
    }
    let field = |path: &str, kind: FieldKind, value: String| Spot {
        field: Field {
            path: path.into(),
            kind,
            value,
            editable: false,
            note: Some("summary (read-only)".into()),
        },
        start: 0,
        end: 0,
    };
    let int = FieldKind::Int;
    let text = FieldKind::Text;
    let spots = vec![
        field("Character", text, name),
        field("Level", int, level.to_string()),
        field("Location", text, location),
        field("PlayTime", text, play_time),
        field("Race", text, race),
        field(
            "Sex",
            text,
            match sex {
                0 => "male".into(),
                1 => "female".into(),
                n => n.to_string(),
            },
        ),
        field("Experience", FieldKind::Float, format!("{xp}")),
        field(
            "ExperienceForNextLevel",
            FieldKind::Float,
            format!("{next_xp}"),
        ),
        field("SaveNumber", int, save_number.to_string()),
        field("FormatVersion", int, version.to_string()),
    ];
    Some(Summary { game, spots })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A save header as Skyrim writes it (made here, not from a real save).
    pub(crate) fn sample(magic: &[u8]) -> Vec<u8> {
        let mut h = Vec::new();
        let ws = |h: &mut Vec<u8>, s: &str| {
            h.extend_from_slice(&(s.len() as u16).to_le_bytes());
            h.extend_from_slice(s.as_bytes());
        };
        h.extend_from_slice(&12u32.to_le_bytes());
        h.extend_from_slice(&42u32.to_le_bytes());
        ws(&mut h, "Lydia");
        h.extend_from_slice(&31u32.to_le_bytes());
        ws(&mut h, "Whiterun");
        ws(&mut h, "012.34.56");
        ws(&mut h, "NordRace");
        h.extend_from_slice(&1u16.to_le_bytes());
        h.extend_from_slice(&120.5f32.to_le_bytes());
        h.extend_from_slice(&900f32.to_le_bytes());
        h.extend_from_slice(&0u64.to_le_bytes());
        h.extend_from_slice(&[0; 10]);
        let mut out = magic.to_vec();
        out.extend_from_slice(&(h.len() as u32).to_le_bytes());
        out.extend_from_slice(&h);
        out.extend_from_slice(&[0xAB; 64]);
        out
    }

    #[test]
    fn reads_the_summary() {
        let s = parse(&sample(b"TESV_SAVEGAME")).unwrap();
        assert_eq!(s.game, "Skyrim");
        let get = |p: &str| {
            s.spots
                .iter()
                .find(|x| x.field.path == p)
                .unwrap()
                .field
                .value
                .clone()
        };
        assert_eq!(get("Character"), "Lydia");
        assert_eq!(get("Level"), "31");
        assert_eq!(get("Location"), "Whiterun");
        assert_eq!(get("Race"), "NordRace");
        assert_eq!(get("Sex"), "female");
        assert!(s.spots.iter().all(|x| !x.field.editable));
        assert_eq!(parse(&sample(b"FO4_SAVEGAME")).unwrap().game, "Fallout 4");
    }

    #[test]
    fn broken_headers_are_refused() {
        let mut s = sample(b"TESV_SAVEGAME");
        s.truncate(30);
        assert!(parse(&s).is_none());
        assert!(parse(b"TESV_SAVEGAME\xff\xff\xff\xff").is_none());
        assert!(parse(b"GVAS").is_none());
    }
}
