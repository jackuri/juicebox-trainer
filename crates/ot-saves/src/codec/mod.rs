//! Save file formats ("codecs"), read structurally (PLAN.md §2.3).
//!
//! A file is opened by removing its wrappers (checksums, gzip, zlib, Base64,
//! lz-string, see [`layers`]) and recognising what's inside: JSON, XML, INI
//! text (also GameMaker and Godot), YAML, an Unreal save, a SQLite database,
//! or the summary of a Bethesda save (read-only). Anything else is shown as hex. Nothing is ever deserialised into
//! objects or run (no BinaryFormatter, no pickle).
//!
//! Saving applies [`Edit`]s to the inside and puts the wrappers back, with
//! checksums recomputed. Opening and saving with no edits gives back exactly
//! the same bytes.

pub mod ess;
pub mod gvas;
pub mod ini;
pub mod json;
pub mod layers;
pub mod lzstring;
pub mod sqlite;
pub mod xml;
pub mod yaml;

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
    Xml,
    Ini,
    Yaml,
    Gvas,
    Sqlite,
    /// Skyrim / Fallout 4: a read-only summary.
    Bethesda,
    Binary,
}

impl BodyKind {
    pub fn label(self) -> &'static str {
        match self {
            BodyKind::Json => "JSON",
            BodyKind::Xml => "XML",
            BodyKind::Ini => "INI text",
            BodyKind::Yaml => "YAML",
            BodyKind::Gvas => "Unreal save (GVAS)",
            BodyKind::Sqlite => "SQLite database",
            BodyKind::Bethesda => "Bethesda save",
            BodyKind::Binary => "Binary (hex)",
        }
    }

    /// The name save codes use (`"codec": "json"`).
    pub fn codec_name(self) -> &'static str {
        match self {
            BodyKind::Json => "json",
            BodyKind::Xml => "xml",
            BodyKind::Ini => "ini",
            BodyKind::Yaml => "yaml",
            BodyKind::Gvas => "gvas",
            BodyKind::Sqlite => "sqlite",
            BodyKind::Bethesda => "ess",
            BodyKind::Binary => "hex",
        }
    }
}

pub(crate) fn body_kind(b: &[u8]) -> BodyKind {
    if b.starts_with(b"GVAS") {
        return BodyKind::Gvas;
    }
    if ess::is_bethesda(b) {
        return BodyKind::Bethesda;
    }
    if sqlite::is_sqlite(b) {
        return BodyKind::Sqlite;
    }
    if json::parse(b).is_ok() {
        return BodyKind::Json;
    }
    if xml::parse(b).is_some() {
        return BodyKind::Xml;
    }
    if ini::parse(b).is_some() {
        return BodyKind::Ini;
    }
    if yaml::parse(b).is_some() {
        return BodyKind::Yaml;
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
    let mut direct: Option<Vec<Field>> = None;
    let (body, spots, truncated) = match body_kind(&contents) {
        BodyKind::Sqlite => match sqlite::parse(&contents) {
            Ok((fields, t)) => {
                direct = Some(fields);
                (BodyKind::Sqlite, Vec::new(), t)
            }
            Err(e) => {
                note = Some(format!("SQLite database that can't be read: {e}"));
                (BodyKind::Binary, Vec::new(), false)
            }
        },
        BodyKind::Json => match json::parse(&contents) {
            Ok((s, t)) => (BodyKind::Json, s, t),
            Err(e) => {
                note = Some(e);
                (BodyKind::Binary, Vec::new(), false)
            }
        },
        BodyKind::Xml => match xml::parse(&contents) {
            Some((s, t)) => (BodyKind::Xml, s, t),
            None => (BodyKind::Binary, Vec::new(), false),
        },
        BodyKind::Ini => match ini::parse(&contents) {
            Some((s, t)) => (BodyKind::Ini, s, t),
            None => (BodyKind::Binary, Vec::new(), false),
        },
        BodyKind::Yaml => match yaml::parse(&contents) {
            Some((s, t)) => (BodyKind::Yaml, s, t),
            None => (BodyKind::Binary, Vec::new(), false),
        },
        BodyKind::Bethesda => match ess::parse(&contents) {
            Some(summary) => {
                note = Some(format!(
                    "Only the summary at the start of this {} save is shown. OpenTrainer can't \
                     change Bethesda saves.",
                    summary.game
                ));
                (BodyKind::Bethesda, summary.spots, false)
            }
            None => {
                note = Some("Bethesda save whose summary can't be read".into());
                (BodyKind::Binary, Vec::new(), false)
            }
        },
        BodyKind::Gvas => match gvas::parse(&contents) {
            Ok((s, t)) => (BodyKind::Gvas, s, t),
            Err(e) => {
                note = Some(format!("Unreal save that can't be read: {e}"));
                (BodyKind::Binary, Vec::new(), false)
            }
        },
        BodyKind::Binary => {
            if contents.starts_with(b"GDEC") {
                note = Some(
                    "This Godot save is encrypted by the game. OpenTrainer doesn't decrypt saves."
                        .into(),
                );
            }
            (BodyKind::Binary, Vec::new(), false)
        }
    };
    Opened {
        layers,
        body,
        fields: direct.unwrap_or_else(|| spots.into_iter().map(|s| s.field).collect()),
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
                BodyKind::Xml => xml::set(&contents, path, value)?,
                BodyKind::Ini => ini::set(&contents, path, value)?,
                BodyKind::Yaml => yaml::set(&contents, path, value)?,
                BodyKind::Gvas => gvas::set(&contents, path, value)?,
                BodyKind::Sqlite => sqlite::set(&contents, path, value)?,
                BodyKind::Bethesda => {
                    return Err("Bethesda saves are shown read-only".into());
                }
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
    fn opens_the_new_formats() {
        // RPG Maker MV: JSON in lz-string Base64.
        let json = r#"{"gold":250,"party":{"_actors":[1,2]}}"#;
        let file = lzstring::compress_to_base64(json).into_bytes();
        let o = open(&file);
        assert_eq!(o.body, BodyKind::Json);
        assert_eq!(o.layers.len(), 1);
        let out = apply(
            &file,
            &[Edit::Set {
                path: "gold".into(),
                value: "9999".into(),
            }],
        )
        .unwrap();
        assert_eq!(
            lzstring::decompress_from_base64(std::str::from_utf8(&out).unwrap()).unwrap(),
            json.replace("250", "9999")
        );
        // RPG Maker MZ: zlib JSON (already handled).
        let mz = layers::wrap(&[Layer::Zlib], json.as_bytes().to_vec()).unwrap();
        assert_eq!(open(&mz).body, BodyKind::Json);

        let xml = b"<Save><Gold>5</Gold></Save>";
        assert_eq!(open(xml).body, BodyKind::Xml);
        assert_eq!(open(xml).fields[0].path, "Save.Gold");
        let out = apply(
            xml,
            &[Edit::Set {
                path: "Save.Gold".into(),
                value: "6".into(),
            }],
        )
        .unwrap();
        assert_eq!(out, b"<Save><Gold>6</Gold></Save>");

        let yaml = b"gold: 5\nname: Hero\n";
        assert_eq!(open(yaml).body, BodyKind::Yaml);
        assert_eq!(open(yaml).body.codec_name(), "yaml");

        let ess = ess::tests::sample(b"TESV_SAVEGAME");
        let o = open(&ess);
        assert_eq!(o.body, BodyKind::Bethesda);
        assert!(o.note.unwrap().contains("Skyrim"));
        assert!(
            o.fields
                .iter()
                .any(|f| f.path == "Character" && f.value == "Lydia")
        );
        assert!(
            apply(
                &ess,
                &[Edit::Set {
                    path: "Level".into(),
                    value: "80".into()
                }]
            )
            .is_err()
        );
        // Byte edits of the rest still work.
        assert!(
            apply(
                &ess,
                &[Edit::Patch {
                    offset: 0,
                    hex: "54".into()
                }]
            )
            .is_ok()
        );

        let db = sqlite::tests::sample(false);
        let o = open(&db);
        assert_eq!(o.body, BodyKind::Sqlite);
        assert_eq!(o.body.codec_name(), "sqlite");
        assert!(o.fields.iter().any(|f| f.path == "Player[1].Gold"));
        let out = apply(
            &db,
            &[Edit::Set {
                path: "Player[1].Gold".into(),
                value: "7".into(),
            }],
        )
        .unwrap();
        assert!(
            open(&out)
                .fields
                .iter()
                .any(|f| f.path == "Player[1].Gold" && f.value == "7")
        );
        // A zlib-compressed database is unwrapped and wrapped again.
        let packed = layers::wrap(&[Layer::Zlib], db.clone()).unwrap();
        assert_eq!(open(&packed).body, BodyKind::Sqlite);

        let godot = b"GDEC\x01\x02\x03\x04 encrypted";
        let o = open(godot);
        assert_eq!(o.body, BodyKind::Binary);
        assert!(o.note.unwrap().contains("Godot"));
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
