//! Save file formats ("codecs"), read structurally (PLAN.md §2.3).
//!
//! A file is opened by removing its wrappers (checksums, gzip, zlib, Base64,
//! see [`layers`]) and recognising what's inside: JSON, INI text or an Unreal
//! save. Anything else is shown as hex. Nothing is ever deserialised into
//! objects or run (no BinaryFormatter, no pickle).
//!
//! Saving applies [`Edit`]s to the inside and puts the wrappers back, with
//! checksums recomputed. Opening and saving with no edits gives back exactly
//! the same bytes.

pub mod gvas;
pub mod ini;
pub mod json;
pub mod layers;

use serde::{Deserialize, Serialize};

pub use layers::Layer;

/// Fields listed per file at most.
pub const MAX_FIELDS: usize = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldKind {
    Int,
    Float,
    Bool,
    Text,
    Null,
    /// Shown but not editable (e.g. a struct stored as raw bytes).
    Other,
}

/// One value in a save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    /// `player.gold`, `items[2].count`, `Stats.Level`.
    pub path: String,
    pub kind: FieldKind,
    pub value: String,
    pub editable: bool,
    /// Format detail, e.g. the Unreal property type.
    pub note: Option<String>,
}

/// A change to make.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Edit {
    /// Set the field at `path`.
    Set { path: String, value: String },
    /// Overwrite bytes at `offset` (of the unwrapped contents) with `hex`.
    Patch { offset: u64, hex: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BodyKind {
    Json,
    Ini,
    Gvas,
    Binary,
}

impl BodyKind {
    pub fn label(self) -> &'static str {
        match self {
            BodyKind::Json => "JSON",
            BodyKind::Ini => "INI text",
            BodyKind::Gvas => "Unreal save (GVAS)",
            BodyKind::Binary => "Binary (hex)",
        }
    }

    /// The name save codes use (`"codec": "json"`).
    pub fn codec_name(self) -> &'static str {
        match self {
            BodyKind::Json => "json",
            BodyKind::Ini => "ini",
            BodyKind::Gvas => "gvas",
            BodyKind::Binary => "hex",
        }
    }
}

pub(crate) fn body_kind(b: &[u8]) -> BodyKind {
    if b.starts_with(b"GVAS") {
        return BodyKind::Gvas;
    }
    if json::parse(b).is_ok() {
        return BodyKind::Json;
    }
    if ini::parse(b).is_some() {
        return BodyKind::Ini;
    }
    BodyKind::Binary
}

/// A file as the editor shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    pub layers: Vec<Layer>,
    pub body: BodyKind,
    pub fields: Vec<Field>,
    pub truncated: bool,
    /// Size of the unwrapped contents (what hex offsets refer to).
    pub body_len: u64,
    /// Why the file is shown as hex, if it looked like something else.
    pub note: Option<String>,
    #[serde(skip)]
    pub contents: Vec<u8>,
}

/// Opens a save from its bytes. Never fails: unknown data is shown as hex.
pub fn open(bytes: &[u8]) -> Opened {
    let (layers, contents) = layers::peel(bytes);
    let mut note = None;
    let (body, spots, truncated) = match body_kind(&contents) {
        BodyKind::Json => match json::parse(&contents) {
            Ok((s, t)) => (BodyKind::Json, s, t),
            Err(e) => {
                note = Some(e);
                (BodyKind::Binary, Vec::new(), false)
            }
        },
        BodyKind::Ini => match ini::parse(&contents) {
            Some((s, t)) => (BodyKind::Ini, s, t),
            None => (BodyKind::Binary, Vec::new(), false),
        },
        BodyKind::Gvas => match gvas::parse(&contents) {
            Ok((s, t)) => (BodyKind::Gvas, s, t),
            Err(e) => {
                note = Some(format!("Unreal save that can't be read: {e}"));
                (BodyKind::Binary, Vec::new(), false)
            }
        },
        BodyKind::Binary => (BodyKind::Binary, Vec::new(), false),
    };
    Opened {
        layers,
        body,
        fields: spots.into_iter().map(|s| s.field).collect(),
        truncated,
        body_len: contents.len() as u64,
        note,
        contents,
    }
}

fn parse_hex(hex: &str) -> Result<Vec<u8>, String> {
    let clean: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
    if clean.is_empty() || !clean.len().is_multiple_of(2) {
        return Err("enter whole bytes, like 0A FF 10".into());
    }
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).map_err(|_| format!("{hex:?} isn't hex")))
        .collect()
}

/// Applies `edits` and returns the new file bytes. With no edits, returns
/// the original bytes unchanged.
pub fn apply(bytes: &[u8], edits: &[Edit]) -> Result<Vec<u8>, String> {
    if edits.is_empty() {
        return Ok(bytes.to_vec());
    }
    let (layers, mut contents) = layers::peel(bytes);
    for edit in edits {
        contents = match edit {
            Edit::Set { path, value } => match body_kind(&contents) {
                BodyKind::Json => json::set(&contents, path, value)?,
                BodyKind::Ini => ini::set(&contents, path, value)?,
                BodyKind::Gvas => gvas::set(&contents, path, value)?,
                BodyKind::Binary => {
                    return Err("this file has no named values; edit its bytes instead".into());
                }
            },
            Edit::Patch { offset, hex } => {
                let new = parse_hex(hex)?;
                let start = usize::try_from(*offset).map_err(|_| "offset too large")?;
                let end = start
                    .checked_add(new.len())
                    .filter(|&e| e <= contents.len())
                    .ok_or("the bytes go past the end of the file")?;
                contents[start..end].copy_from_slice(&new);
                contents
            }
        };
    }
    layers::wrap(&layers, contents).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use layers::{Algo, At};

    #[test]
    fn opens_wrapped_json_and_saves_it_back_wrapped() {
        let json = b"{\"player\": {\"gold\": 250}}".to_vec();
        let file = layers::wrap(
            &[
                Layer::Checksum {
                    algo: Algo::Crc32Le,
                    at: At::Trailer,
                },
                Layer::Gzip,
            ],
            json.clone(),
        )
        .unwrap();
        let o = open(&file);
        assert_eq!(o.body, BodyKind::Json);
        assert_eq!(o.layers.len(), 2);
        assert_eq!(o.fields[0].path, "player.gold");
        assert_eq!(
            apply(&file, &[]).unwrap(),
            file,
            "unedited saves are untouched"
        );

        let out = apply(
            &file,
            &[Edit::Set {
                path: "player.gold".into(),
                value: "999".into(),
            }],
        )
        .unwrap();
        let again = open(&out);
        assert_eq!(again.layers, o.layers, "same wrappers, checksum recomputed");
        assert_eq!(again.fields[0].value, "999");
    }

    #[test]
    fn unknown_files_are_hex_and_can_be_patched() {
        let mut bin = vec![0u8; 32];
        bin[0] = 0xFE;
        let o = open(&bin);
        assert_eq!(o.body, BodyKind::Binary);
        assert!(o.fields.is_empty());
        let out = apply(
            &bin,
            &[Edit::Patch {
                offset: 8,
                hex: "e8 03 00 00".into(),
            }],
        )
        .unwrap();
        assert_eq!(&out[8..12], &1000i32.to_le_bytes());
        assert!(
            apply(
                &bin,
                &[Edit::Patch {
                    offset: 30,
                    hex: "010203".into()
                }]
            )
            .is_err()
        );
        assert!(
            apply(
                &bin,
                &[Edit::Patch {
                    offset: 0,
                    hex: "zz".into()
                }]
            )
            .is_err()
        );
        assert!(
            apply(
                &bin,
                &[Edit::Set {
                    path: "a".into(),
                    value: "1".into()
                }]
            )
            .is_err()
        );
    }

    #[test]
    fn opens_unreal_saves() {
        let o = open(&gvas::writer::sample());
        assert_eq!(o.body, BodyKind::Gvas);
        assert!(
            o.fields
                .iter()
                .any(|f| f.path == "Stats.Level" && f.editable)
        );
        let mut bad = gvas::writer::sample();
        bad.truncate(40);
        let o = open(&bad);
        assert_eq!(o.body, BodyKind::Binary);
        assert!(o.note.is_some());
    }
}
