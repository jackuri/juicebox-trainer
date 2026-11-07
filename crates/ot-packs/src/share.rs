//! `.otpack` files: one pack, ready to share (PLAN.md §2.5).
//!
//! An `.otpack` is a zip with exactly two files: `otpack.json` (what it is)
//! and `pack.json` (the pack, as plain JSON). Opening one reads only those
//! two entries into memory, with size limits; nothing is unpacked to disk and
//! nothing in it is run. The pack goes through the same validator as every
//! other pack, so an imported pack can't do more than a hand-written one.
//!
//! A plain `pack.json` can be opened the same way ([`open_path`]).

use std::io::{Cursor, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::schema::Pack;
use crate::store::MAX_PACK_BYTES;
use crate::validate::{Issue, LoadError, parse};

/// File extension, without the dot.
pub const EXTENSION: &str = "otpack";
/// The only `.otpack` format this build writes; it reads this or older.
pub const FORMAT: u32 = 1;
/// `.otpack` files refuse to open above this size.
pub const MAX_FILE_BYTES: u64 = 8 << 20;

const MANIFEST: &str = "otpack.json";
const PACK: &str = "pack.json";

/// `otpack.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: u32,
    /// Must match the pack's `game.id`.
    pub pack_id: String,
    pub pack_version: String,
    /// `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported: Option<String>,
    /// The OpenTrainer version that wrote it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ShareError {
    #[error("{0}")]
    Io(String),
    #[error("this isn't an .otpack file ({0})")]
    NotOtpack(String),
    #[error("the file is {0} bytes, too big to be a pack")]
    TooBig(u64),
    #[error("it was made by a newer OpenTrainer (format {0}); update OpenTrainer to open it")]
    Newer(u32),
    #[error("the pack inside isn't valid: {0}")]
    Invalid(#[from] LoadError),
}

/// An opened `.otpack`: the pack, its exact text and its warnings.
#[derive(Debug, Clone)]
pub struct Opened {
    pub manifest: Manifest,
    pub text: String,
    pub pack: Pack,
    pub warnings: Vec<Issue>,
}

/// Builds an `.otpack` from pack text. Only valid packs are exported.
/// `today` is `YYYY-MM-DD`.
pub fn export(pack_text: &str, app_version: &str, today: &str) -> Result<Vec<u8>, ShareError> {
    let (pack, _) = parse(pack_text)?;
    let manifest = Manifest {
        format: FORMAT,
        pack_id: pack.game.id.clone(),
        pack_version: pack.meta.pack_version.clone(),
        exported: Some(today.to_string()),
        app: Some(app_version.to_string()),
    };
    let manifest =
        serde_json::to_string_pretty(&manifest).map_err(|e| ShareError::Io(e.to_string()))?;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let io = |e: std::io::Error| ShareError::Io(e.to_string());
    let z = |e: zip::result::ZipError| ShareError::Io(e.to_string());
    zip.start_file(MANIFEST, options).map_err(z)?;
    zip.write_all(manifest.as_bytes()).map_err(io)?;
    zip.write_all(b"\n").map_err(io)?;
    zip.start_file(PACK, options).map_err(z)?;
    zip.write_all(pack_text.as_bytes()).map_err(io)?;
    Ok(zip.finish().map_err(z)?.into_inner())
}

/// Opens `.otpack` bytes.
pub fn open(bytes: &[u8]) -> Result<Opened, ShareError> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(ShareError::TooBig(bytes.len() as u64));
    }
    let not = |why: String| ShareError::NotOtpack(why);
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| not(e.to_string()))?;
    let mut manifest = None;
    let mut text = None;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| not(e.to_string()))?;
        let name = entry.name().map_err(|e| not(e.to_string()))?.to_string();
        let slot = match name.as_str() {
            MANIFEST => &mut manifest,
            PACK => &mut text,
            _ => return Err(not(format!("it holds an unexpected file: {name}"))),
        };
        if slot.is_some() {
            return Err(not(format!("{name} is in it twice")));
        }
        if entry.size() > MAX_PACK_BYTES {
            return Err(not(format!("{name} is too big")));
        }
        // Read at most the limit plus one byte, whatever the header claims.
        let mut buf = Vec::new();
        (&mut entry)
            .take(MAX_PACK_BYTES + 1)
            .read_to_end(&mut buf)
            .map_err(|e| not(format!("{name}: {e}")))?;
        if buf.len() as u64 > MAX_PACK_BYTES {
            return Err(not(format!("{name} is too big")));
        }
        let s = String::from_utf8(buf).map_err(|_| not(format!("{name} isn't text")))?;
        *slot = Some(s);
    }
    let manifest = manifest.ok_or_else(|| not(format!("{MANIFEST} is missing")))?;
    let text = text.ok_or_else(|| not(format!("{PACK} is missing")))?;
    let manifest: Manifest =
        serde_json::from_str(&manifest).map_err(|e| not(format!("{MANIFEST}: {e}")))?;
    if manifest.format > FORMAT {
        return Err(ShareError::Newer(manifest.format));
    }
    if manifest.format == 0 {
        return Err(not(format!("{MANIFEST} has format 0")));
    }
    let (pack, warnings) = parse(&text)?;
    if pack.game.id != manifest.pack_id {
        return Err(not(format!(
            "{MANIFEST} names pack {} but the pack is {}",
            manifest.pack_id, pack.game.id
        )));
    }
    Ok(Opened {
        manifest,
        text,
        pack,
        warnings,
    })
}

/// Opens an `.otpack` file.
pub fn open_file(path: &Path) -> Result<Opened, ShareError> {
    let io = |e: std::io::Error| ShareError::Io(format!("{}: {e}", path.display()));
    let len = std::fs::metadata(path).map_err(io)?.len();
    if len > MAX_FILE_BYTES {
        return Err(ShareError::TooBig(len));
    }
    open(&std::fs::read(path).map_err(io)?)
}

/// Opens a plain `pack.json` (the file a pack folder holds, as shared on
/// forums or in the community repository). Same size limit and validator as
/// the `pack.json` inside an `.otpack`; the manifest is made up from the pack.
pub fn open_pack_json(bytes: &[u8]) -> Result<Opened, ShareError> {
    if bytes.len() as u64 > MAX_PACK_BYTES {
        return Err(ShareError::TooBig(bytes.len() as u64));
    }
    // Notepad and some editors start UTF-8 files with a byte-order mark.
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = String::from_utf8(bytes.to_vec())
        .map_err(|_| ShareError::NotOtpack("the file isn't text".into()))?;
    let (pack, warnings) = parse(&text)?;
    let manifest = Manifest {
        format: FORMAT,
        pack_id: pack.game.id.clone(),
        pack_version: pack.meta.pack_version.clone(),
        exported: None,
        app: None,
    };
    Ok(Opened {
        manifest,
        text,
        pack,
        warnings,
    })
}

/// Opens an `.otpack` or a plain `.json` pack file, by extension.
pub fn open_path(path: &Path) -> Result<Opened, ShareError> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    match ext.as_deref() {
        Some(EXTENSION) => open_file(path),
        Some("json") => {
            let io = |e: std::io::Error| ShareError::Io(format!("{}: {e}", path.display()));
            let len = std::fs::metadata(path).map_err(io)?.len();
            if len > MAX_PACK_BYTES {
                return Err(ShareError::TooBig(len));
            }
            open_pack_json(&std::fs::read(path).map_err(io)?)
        }
        _ => Err(ShareError::NotOtpack(format!(
            "{} isn't an .otpack or pack.json file",
            path.display()
        ))),
    }
}

/// A file name for an exported pack: `<id>-<version>.otpack`.
pub fn file_name(pack: &Pack) -> String {
    let version: String = pack
        .meta
        .pack_version
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{}-{version}.{EXTENSION}", pack.game.id)
}

/// Today's date (UTC) as `YYYY-MM-DD`.
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    date_of_days((secs / 86_400) as i64)
}

/// Days since 1970-01-01 → `YYYY-MM-DD` (proleptic Gregorian).
fn date_of_days(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::BUNDLED;

    #[test]
    fn dates() {
        assert_eq!(date_of_days(0), "1970-01-01");
        assert_eq!(date_of_days(20_399), "2025-11-07");
        assert_eq!(date_of_days(20_764), "2026-11-07");
        assert_eq!(date_of_days(11_016), "2000-02-29");
        assert_eq!(today().len(), 10);
    }

    fn sample() -> &'static str {
        BUNDLED[0].1
    }

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn opens_plain_pack_json() {
        let opened = open_pack_json(sample().as_bytes()).unwrap();
        assert_eq!(opened.text, sample());
        assert_eq!(opened.manifest.pack_id, "testbed-native");
        assert!(opened.manifest.app.is_none());

        let mut bom = b"\xEF\xBB\xBF".to_vec();
        bom.extend_from_slice(sample().as_bytes());
        assert_eq!(open_pack_json(&bom).unwrap().text, sample());

        assert!(matches!(
            open_pack_json(b"{\"schema\":1}"),
            Err(ShareError::Invalid(_))
        ));
        assert!(matches!(
            open_pack_json(&[0xFF, 0xFE, 0x00]),
            Err(ShareError::NotOtpack(_))
        ));
        let big = vec![b' '; (MAX_PACK_BYTES + 1) as usize];
        assert!(matches!(open_pack_json(&big), Err(ShareError::TooBig(_))));
    }

    #[test]
    fn opens_files_by_extension() {
        let dir = std::env::temp_dir().join(format!("ot-share-path-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let json = dir.join("pack.JSON");
        std::fs::write(&json, sample()).unwrap();
        let otpack = dir.join("x.otpack");
        std::fs::write(&otpack, export(sample(), "1.1.0", "2026-11-07").unwrap()).unwrap();
        let other = dir.join("x.txt");
        std::fs::write(&other, sample()).unwrap();

        assert_eq!(open_path(&json).unwrap().text, sample());
        assert_eq!(open_path(&otpack).unwrap().text, sample());
        assert!(matches!(open_path(&other), Err(ShareError::NotOtpack(_))));
        // A pack.json renamed to .otpack is still refused (it isn't a zip).
        std::fs::write(&otpack, sample()).unwrap();
        assert!(matches!(open_path(&otpack), Err(ShareError::NotOtpack(_))));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn round_trips_a_pack_exactly() {
        let bytes = export(sample(), "1.0.0", "2026-11-07").unwrap();
        let opened = open(&bytes).unwrap();
        assert_eq!(opened.text, sample());
        assert_eq!(opened.manifest.pack_id, "testbed-native");
        assert_eq!(opened.manifest.format, FORMAT);
        assert_eq!(opened.manifest.app.as_deref(), Some("1.0.0"));
        assert_eq!(opened.manifest.exported.as_deref(), Some("2026-11-07"));
        assert_eq!(file_name(&opened.pack), "testbed-native-1.0.0.otpack");
    }

    #[test]
    fn refuses_invalid_packs_both_ways() {
        assert!(matches!(
            export("{}", "1.0.0", "2026-11-07"),
            Err(ShareError::Invalid(_))
        ));
        let manifest = br#"{"format":1,"packId":"x","packVersion":"1"}"#;
        let bytes = zip_of(&[("otpack.json", manifest), ("pack.json", b"{\"schema\":1}")]);
        assert!(matches!(open(&bytes), Err(ShareError::Invalid(_))));
    }

    #[test]
    fn refuses_anything_but_the_two_files() {
        let manifest = br#"{"format":1,"packId":"testbed-native","packVersion":"1.0.0"}"#;
        let pack = sample().as_bytes();
        let extra = zip_of(&[
            ("otpack.json", manifest),
            ("pack.json", pack),
            ("run-me.exe", b"MZ"),
        ]);
        let e = open(&extra).unwrap_err().to_string();
        assert!(e.contains("unexpected file: run-me.exe"), "{e}");
        let sneaky = zip_of(&[("otpack.json", manifest), ("../pack.json", pack)]);
        assert!(open(&sneaky).is_err());
        let missing = zip_of(&[("otpack.json", manifest)]);
        assert!(
            open(&missing)
                .unwrap_err()
                .to_string()
                .contains("pack.json is missing")
        );
        assert!(matches!(
            open(b"PK not a zip"),
            Err(ShareError::NotOtpack(_))
        ));
        let ok = zip_of(&[("otpack.json", manifest), ("pack.json", pack)]);
        assert!(open(&ok).is_ok());
    }

    #[test]
    fn refuses_newer_formats_and_mismatched_ids() {
        let pack = sample().as_bytes();
        let newer = zip_of(&[
            (
                "otpack.json",
                br#"{"format":2,"packId":"testbed-native","packVersion":"1"}"#,
            ),
            ("pack.json", pack),
        ]);
        assert!(matches!(open(&newer), Err(ShareError::Newer(2))));
        let other = zip_of(&[
            (
                "otpack.json",
                br#"{"format":1,"packId":"other","packVersion":"1"}"#,
            ),
            ("pack.json", pack),
        ]);
        assert!(
            open(&other)
                .unwrap_err()
                .to_string()
                .contains("names pack other")
        );
    }

    #[test]
    fn refuses_oversized_entries_and_files() {
        let manifest = br#"{"format":1,"packId":"testbed-native","packVersion":"1.0.0"}"#;
        let big = vec![b' '; (MAX_PACK_BYTES + 1) as usize];
        // Highly compressible, so the zip itself is small (a zip bomb).
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let deflate = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("otpack.json", deflate).unwrap();
        zip.write_all(manifest).unwrap();
        zip.start_file("pack.json", deflate).unwrap();
        zip.write_all(&big).unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        assert!(bytes.len() < 100_000);
        assert!(open(&bytes).unwrap_err().to_string().contains("too big"));
        let huge = vec![0u8; (MAX_FILE_BYTES + 1) as usize];
        assert!(matches!(open(&huge), Err(ShareError::TooBig(_))));
    }
}
