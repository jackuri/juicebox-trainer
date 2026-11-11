//! Wrappers around a save's real contents: checksums, gzip, zlib and Base64.
//! Each is detected from the bytes, removed for viewing and editing, and put
//! back (recomputed or recompressed) when edits are saved.

use std::io::Read;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use md5::Md5;
use serde::Serialize;
use sha1::Sha1;
use sha2::{Digest, Sha256};

use super::BodyKind;

/// Decompressed data larger than this is not opened.
pub const MAX_INFLATED: u64 = 128 << 20;
const MAX_LAYERS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Algo {
    Crc32Le,
    Crc32Be,
    Md5,
    Sha1,
    Sha256,
}

impl Algo {
    const ALL: [Algo; 5] = [
        Algo::Sha256,
        Algo::Sha1,
        Algo::Md5,
        Algo::Crc32Le,
        Algo::Crc32Be,
    ];

    fn len(self) -> usize {
        match self {
            Algo::Crc32Le | Algo::Crc32Be => 4,
            Algo::Md5 => 16,
            Algo::Sha1 => 20,
            Algo::Sha256 => 32,
        }
    }

    fn digest(self, data: &[u8]) -> Vec<u8> {
        match self {
            Algo::Crc32Le => crc32fast::hash(data).to_le_bytes().to_vec(),
            Algo::Crc32Be => crc32fast::hash(data).to_be_bytes().to_vec(),
            Algo::Md5 => Md5::digest(data).to_vec(),
            Algo::Sha1 => Sha1::digest(data).to_vec(),
            Algo::Sha256 => Sha256::digest(data).to_vec(),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Algo::Crc32Le | Algo::Crc32Be => "CRC32",
            Algo::Md5 => "MD5",
            Algo::Sha1 => "SHA-1",
            Algo::Sha256 => "SHA-256",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum At {
    Header,
    Trailer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum B64 {
    Standard,
    StandardNoPad,
    UrlSafe,
    UrlSafeNoPad,
}

impl B64 {
    const ALL: [B64; 4] = [
        B64::Standard,
        B64::UrlSafe,
        B64::StandardNoPad,
        B64::UrlSafeNoPad,
    ];

    fn decode(self, text: &[u8]) -> Option<Vec<u8>> {
        match self {
            B64::Standard => STANDARD.decode(text),
            B64::StandardNoPad => STANDARD_NO_PAD.decode(text),
            B64::UrlSafe => URL_SAFE.decode(text),
            B64::UrlSafeNoPad => URL_SAFE_NO_PAD.decode(text),
        }
        .ok()
    }

    fn encode(self, data: &[u8]) -> String {
        match self {
            B64::Standard => STANDARD.encode(data),
            B64::StandardNoPad => STANDARD_NO_PAD.encode(data),
            B64::UrlSafe => URL_SAFE.encode(data),
            B64::UrlSafeNoPad => URL_SAFE_NO_PAD.encode(data),
        }
    }
}

/// One wrapper, outermost first in a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Layer {
    Checksum {
        algo: Algo,
        at: At,
    },
    Gzip,
    Zlib,
    Base64 {
        variant: B64,
        trailing: String,
    },
    /// lz-string's Base64 form (RPG Maker MV `.rpgsave`).
    LzString {
        trailing: String,
    },
}

impl Layer {
    /// For players: "CRC32 checksum at the end", "gzip-compressed".
    pub fn label(&self) -> String {
        match self {
            Layer::Checksum { algo, at } => format!(
                "{} checksum at the {}",
                algo.name(),
                if *at == At::Header { "start" } else { "end" }
            ),
            Layer::Gzip => "gzip-compressed".into(),
            Layer::Zlib => "zlib-compressed".into(),
            Layer::Base64 { .. } => "Base64 text".into(),
            Layer::LzString { .. } => "lz-string compressed (RPG Maker MV)".into(),
        }
    }

    /// Wraps `inner` again (after an edit).
    pub fn wrap(&self, inner: &[u8]) -> std::io::Result<Vec<u8>> {
        use std::io::Write;
        Ok(match self {
            Layer::Checksum { algo, at } => {
                let d = algo.digest(inner);
                match at {
                    At::Header => [d.as_slice(), inner].concat(),
                    At::Trailer => [inner, d.as_slice()].concat(),
                }
            }
            Layer::Gzip => {
                let mut e =
                    flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
                e.write_all(inner)?;
                e.finish()?
            }
            Layer::Zlib => {
                let mut e =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                e.write_all(inner)?;
                e.finish()?
            }
            Layer::Base64 { variant, trailing } => {
                let mut s = variant.encode(inner);
                s.push_str(trailing);
                s.into_bytes()
            }
            Layer::LzString { trailing } => {
                let text = std::str::from_utf8(inner).map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "lz-string saves hold text; the edit made it invalid",
                    )
                })?;
                let mut s = super::lzstring::compress_to_base64(text);
                s.push_str(trailing);
                s.into_bytes()
            }
        })
    }
}

fn checksum(bytes: &[u8]) -> Option<(Layer, Vec<u8>)> {
    for algo in Algo::ALL {
        let n = algo.len();
        if bytes.len() <= n + 1 {
            continue;
        }
        let (body, tail) = bytes.split_at(bytes.len() - n);
        if algo.digest(body) == tail {
            return Some((
                Layer::Checksum {
                    algo,
                    at: At::Trailer,
                },
                body.to_vec(),
            ));
        }
        let (head, body) = bytes.split_at(n);
        if algo.digest(body) == head {
            return Some((
                Layer::Checksum {
                    algo,
                    at: At::Header,
                },
                body.to_vec(),
            ));
        }
    }
    None
}

/// Reads a whole stream, refusing more than [`MAX_INFLATED`] bytes.
fn read_limited(r: impl Read) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    r.take(MAX_INFLATED + 1).read_to_end(&mut out).ok()?;
    (out.len() as u64 <= MAX_INFLATED).then_some(out)
}

fn gzip(bytes: &[u8]) -> Option<(Layer, Vec<u8>)> {
    if !bytes.starts_with(&[0x1F, 0x8B]) {
        return None;
    }
    let mut d = flate2::bufread::GzDecoder::new(bytes);
    let out = read_limited(&mut d)?;
    // Only the whole file: trailing data would be lost on saving.
    d.into_inner().is_empty().then_some((Layer::Gzip, out))
}

fn zlib(bytes: &[u8]) -> Option<(Layer, Vec<u8>)> {
    let [a, b, ..] = *bytes else { return None };
    if a & 0x0F != 8 || (u16::from(a) << 8 | u16::from(b)) % 31 != 0 {
        return None;
    }
    let mut d = flate2::bufread::ZlibDecoder::new(bytes);
    let out = read_limited(&mut d)?;
    d.into_inner().is_empty().then_some((Layer::Zlib, out))
}

fn base64(bytes: &[u8]) -> Option<(Layer, Vec<u8>)> {
    let text = std::str::from_utf8(bytes).ok()?;
    let body = text.trim_end();
    let trailing = text[body.len()..].to_string();
    let valid = body.len() >= 8
        && body
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'-' | b'_' | b'='));
    if !valid {
        return None;
    }
    for variant in B64::ALL {
        let Some(decoded) = variant.decode(body.as_bytes()) else {
            continue;
        };
        // Plain text that happens to be valid Base64 is left alone: only
        // accept it if what's inside is recognisably a save.
        let structured = peel_one(&decoded)
            .is_some_and(|(l, _)| !matches!(l, Layer::Checksum { .. }))
            || matches!(
                super::body_kind(&decoded),
                BodyKind::Json | BodyKind::Xml | BodyKind::Gvas | BodyKind::Sqlite
            );
        if structured && variant.encode(&decoded) == body {
            return Some((Layer::Base64 { variant, trailing }, decoded));
        }
    }
    None
}

fn lzstring(bytes: &[u8]) -> Option<(Layer, Vec<u8>)> {
    let text = std::str::from_utf8(bytes).ok()?;
    let body = text.trim_end();
    if body.len() < 8 {
        return None;
    }
    let inner = super::lzstring::decompress_from_base64(body)?;
    // Only JSON inside (RPG Maker), and only if writing it back gives the
    // same text, so nothing is lost on saving.
    if !matches!(super::body_kind(inner.as_bytes()), BodyKind::Json)
        || super::lzstring::compress_to_base64(&inner) != body
    {
        return None;
    }
    Some((
        Layer::LzString {
            trailing: text[body.len()..].to_string(),
        },
        inner.into_bytes(),
    ))
}

fn peel_one(bytes: &[u8]) -> Option<(Layer, Vec<u8>)> {
    gzip(bytes)
        .or_else(|| zlib(bytes))
        .or_else(|| base64(bytes))
        .or_else(|| lzstring(bytes))
        .or_else(|| checksum(bytes))
}

/// Removes every wrapper it recognises. Returns them outermost first, and the
/// contents inside.
pub fn peel(bytes: &[u8]) -> (Vec<Layer>, Vec<u8>) {
    let mut layers = Vec::new();
    let mut current = bytes.to_vec();
    while layers.len() < MAX_LAYERS {
        // Plain JSON, XML or an Unreal or Bethesda save is never unwrapped
        // further.
        if matches!(
            super::body_kind(&current),
            BodyKind::Json | BodyKind::Xml | BodyKind::Gvas | BodyKind::Sqlite | BodyKind::Bethesda
        ) {
            break;
        }
        let Some((layer, inner)) = peel_one(&current) else {
            break;
        };
        layers.push(layer);
        current = inner;
    }
    (layers, current)
}

/// Puts `inner` back into `layers` (outermost first).
pub fn wrap(layers: &[Layer], inner: Vec<u8>) -> std::io::Result<Vec<u8>> {
    let mut current = inner;
    for layer in layers.iter().rev() {
        current = layer.wrap(&current)?;
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &[u8] = b"{\"gold\": 100}";

    #[test]
    fn peels_and_wraps_every_layer() {
        for layers in [
            vec![Layer::Gzip],
            vec![Layer::Zlib],
            vec![Layer::Base64 {
                variant: B64::Standard,
                trailing: "\n".into(),
            }],
            vec![Layer::Checksum {
                algo: Algo::Crc32Le,
                at: At::Trailer,
            }],
            vec![Layer::Checksum {
                algo: Algo::Sha256,
                at: At::Header,
            }],
            vec![
                Layer::Checksum {
                    algo: Algo::Md5,
                    at: At::Trailer,
                },
                Layer::Zlib,
            ],
            vec![
                Layer::Base64 {
                    variant: B64::UrlSafeNoPad,
                    trailing: String::new(),
                },
                Layer::Gzip,
            ],
            vec![Layer::LzString {
                trailing: String::new(),
            }],
        ] {
            let wrapped = wrap(&layers, JSON.to_vec()).unwrap();
            let (found, inner) = peel(&wrapped);
            // Padded and unpadded Base64 can be the same text; what matters
            // is that wrapping again gives the same bytes.
            assert_eq!(found.len(), layers.len());
            assert_eq!(inner, JSON);
            assert_eq!(wrap(&found, inner).unwrap(), wrapped);
        }
    }

    #[test]
    fn plain_data_is_left_alone() {
        assert_eq!(peel(JSON).0, vec![]);
        assert_eq!(peel(b"hello world, this is text").0, vec![]);
        // Valid Base64, but just text inside.
        assert_eq!(peel(b"SGVsbG8gd29ybGQh").0, vec![]);
        // A zlib-looking header that isn't zlib.
        assert_eq!(peel(&[0x78, 0x9C, 1, 2, 3, 4, 5, 6]).0, vec![]);
        // Trailing junk after gzip data.
        let mut g = wrap(&[Layer::Gzip], JSON.to_vec()).unwrap();
        g.extend_from_slice(b"junk");
        assert_eq!(peel(&g).0, vec![]);
    }

    #[test]
    fn labels() {
        assert_eq!(
            Layer::Checksum {
                algo: Algo::Crc32Be,
                at: At::Trailer
            }
            .label(),
            "CRC32 checksum at the end"
        );
        assert_eq!(Layer::Gzip.label(), "gzip-compressed");
    }
}
