//! The community pack list (PLAN.md §8.3): a public GitHub repository with
//! `index.json` and `packs/<id>/pack.json`. The app downloads the index, shows
//! it, and on "Install" downloads one `pack.json`, checks it against the
//! index's SHA-256 and runs it through the normal validator. This module is
//! the pure part (no network); the desktop app does the downloads.

use serde::{Deserialize, Serialize};

use crate::schema::{Pack, PackArch};
use crate::store::MAX_PACK_BYTES;
use crate::validate::{Issue, is_slug, parse};
use crate::web::{WebPack, check_pack};

/// Where the app reads the list from.
pub const INDEX_URL: &str =
    "https://raw.githubusercontent.com/jackuri/opentrainer-packs/main/index.json";
/// Where people go to add a pack to the list.
pub const CONTRIBUTE_URL: &str = "https://github.com/jackuri/opentrainer-packs#add-your-pack";
/// The index format this build reads.
pub const FORMAT: u32 = 1;
/// The index refuses to load above this size.
pub const MAX_INDEX_BYTES: u64 = 2 << 20;

/// `index.json`. Unknown fields are ignored so newer lists still load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    pub format: u32,
    #[serde(default)]
    pub updated: Option<String>,
    pub packs: Vec<Entry>,
    /// Links to Cheat Engine tables on their makers' pages (v1.16.0). Only
    /// links: the tables themselves stay on the maker's site.
    #[serde(default)]
    pub tables: Vec<TableLink>,
    /// Cheats for web games (v1.19.0), whole (they're small). Read one by one
    /// so a broken or newer entry is left out instead of failing the list.
    #[serde(default, deserialize_with = "lenient_web")]
    pub web: Vec<WebPack>,
}

fn lenient_web<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<WebPack>, D::Error> {
    let raw: Vec<serde_json::Value> = Vec::deserialize(d)?;
    Ok(raw
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

/// Sites table links may point to. A link elsewhere is left out of the list.
pub const TABLE_SITES: &[&str] = &[
    "fearlessrevolution.com",
    "www.fearlessrevolution.com",
    "forum.cheatengine.org",
    "opencheattables.com",
    "www.opencheattables.com",
    "github.com",
];

/// A Cheat Engine table on its maker's page (`tables/<id>.json` in the
/// community repository).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableLink {
    /// File name in `tables/` (without `.json`).
    pub id: String,
    /// The game's name.
    pub game: String,
    /// The game's exe, when known.
    #[serde(default)]
    pub exe: Option<String>,
    /// The table's name on its page.
    pub title: String,
    /// Who made it.
    pub by: String,
    /// Its page (https, on one of [`TABLE_SITES`]).
    pub url: String,
    /// Cheats OpenTrainer could use from it when it was shared.
    #[serde(default)]
    pub cheats: Option<usize>,
    /// `YYYY-MM-DD`.
    #[serde(default)]
    pub added: Option<String>,
}

/// The site of a table link (`fearlessrevolution.com`), if it's an https
/// address on one of [`TABLE_SITES`].
pub fn table_site(url: &str) -> Option<&'static str> {
    if url.len() > crate::schema::MAX_SOURCE_URL || !crate::schema::is_source_url(url) {
        return None;
    }
    let rest = url.strip_prefix("https://")?;
    let host = rest.split(['/', '?', '#']).next()?.to_ascii_lowercase();
    TABLE_SITES.iter().copied().find(|s| *s == host)
}

/// Why a table link can't be listed, if it can't.
pub fn table_problem(t: &TableLink) -> Option<&'static str> {
    let one_line = |s: &str, max: usize| {
        !s.trim().is_empty() && s.chars().count() <= max && !s.chars().any(char::is_control)
    };
    if !is_slug(&t.id) {
        Some("bad id")
    } else if table_site(&t.url).is_none() {
        Some("not an https page on a known table site")
    } else if !one_line(&t.game, 120) || !one_line(&t.title, 160) || !one_line(&t.by, 100) {
        Some("game, title and maker must be short single lines")
    } else if t
        .exe
        .as_deref()
        .is_some_and(|e| !one_line(e, 120) || e.contains(['/', '\\', ':']))
    {
        Some("bad exe")
    } else {
        None
    }
}

/// An id for a link: the game's name and a short hash of the page, so one
/// game can have several tables.
pub fn table_id(game: &str, url: &str) -> String {
    let mut slug: String = game
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug.trim_matches('-');
    let slug: String = slug.chars().take(48).collect();
    let hash = &ot_core::hash::sha256_bytes(url.as_bytes())[..10];
    if slug.is_empty() {
        format!("table-{hash}")
    } else {
        format!("{}-{hash}", slug.trim_end_matches('-'))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// The pack's `game.id`; also its folder in the repository.
    pub id: String,
    /// The game's name.
    pub name: String,
    pub exe: String,
    #[serde(default)]
    pub arch: Option<PackArch>,
    #[serde(default)]
    pub engine: Option<String>,
    pub author: String,
    pub pack_version: String,
    pub cheats: usize,
    #[serde(default)]
    pub notes: Option<String>,
    /// Who made the cheats, their terms and support page (v1.17.0; the pack's
    /// `meta.source` without the file name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<crate::schema::Source>,
    /// SHA-256 of `packs/<id>/pack.json`.
    pub sha256: String,
    /// Bytes of `pack.json`.
    pub size: u64,
}

impl Entry {
    /// `packs/<id>/pack.json`, relative to the index.
    pub fn path(&self) -> String {
        format!("packs/{}/pack.json", self.id)
    }
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Parses and checks `index.json`. Entries with a bad id or hash are left
/// out (listed in the second value) rather than failing the whole list.
pub fn parse_index(bytes: &[u8]) -> Result<(Index, Vec<String>), String> {
    if bytes.len() as u64 > MAX_INDEX_BYTES {
        return Err(format!(
            "the pack list is {} bytes; the limit is {MAX_INDEX_BYTES}",
            bytes.len()
        ));
    }
    let mut index: Index =
        serde_json::from_slice(bytes).map_err(|e| format!("the pack list isn't valid: {e}"))?;
    if index.format != FORMAT {
        return Err(if index.format > FORMAT {
            "the pack list needs a newer OpenTrainer; update OpenTrainer".into()
        } else {
            format!("the pack list has an unknown format ({})", index.format)
        });
    }
    let mut skipped = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    index.packs.retain(|e| {
        let why = if !is_slug(&e.id) {
            Some("bad id")
        } else if !is_sha256(&e.sha256) {
            Some("bad SHA-256")
        } else if e.size > MAX_PACK_BYTES {
            Some("too big")
        } else if !seen.insert(e.id.clone()) {
            Some("listed twice")
        } else {
            None
        };
        if let Some(why) = why {
            skipped.push(format!("{}: {why}", e.id));
        }
        why.is_none()
    });
    index.packs.sort_by_key(|e| e.name.to_lowercase());
    let mut seen = std::collections::BTreeSet::new();
    index.tables.retain(|t| {
        let why =
            table_problem(t).or_else(|| (!seen.insert(t.url.clone())).then_some("listed twice"));
        if let Some(why) = why {
            skipped.push(format!("table {}: {why}", t.id));
        }
        why.is_none()
    });
    index.tables.sort_by(|a, b| {
        a.game
            .to_lowercase()
            .cmp(&b.game.to_lowercase())
            .then_with(|| a.title.cmp(&b.title))
    });
    let mut seen = std::collections::BTreeSet::new();
    index.web = std::mem::take(&mut index.web)
        .into_iter()
        .filter_map(|p| {
            let checked = check_pack(&p, true).and_then(|c| {
                if seen.insert(c.id.clone()) {
                    Ok(c)
                } else {
                    Err("listed twice".into())
                }
            });
            checked
                .map_err(|why| skipped.push(format!("web {}: {why}", p.id)))
                .ok()
        })
        .collect();
    index.web.sort_by(|a, b| {
        a.game
            .to_lowercase()
            .cmp(&b.game.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok((index, skipped))
}

/// The address of an entry's `pack.json`, next to the index.
pub fn pack_url(index_url: &str, entry: &Entry) -> String {
    let base = index_url.rsplit_once('/').map_or(index_url, |(b, _)| b);
    format!("{base}/{}", entry.path())
}

/// Checks a downloaded `pack.json` against its index entry and validates it.
pub fn check_download(entry: &Entry, bytes: &[u8]) -> Result<(Pack, String, Vec<Issue>), String> {
    if bytes.len() as u64 > MAX_PACK_BYTES {
        return Err("the pack is too big".into());
    }
    let sha = ot_core::hash::sha256_bytes(bytes);
    if sha != entry.sha256 {
        return Err(
            "the downloaded pack doesn't match the list (it may have just been updated; refresh and try again)"
                .into(),
        );
    }
    let text = String::from_utf8(bytes.to_vec()).map_err(|_| "the pack isn't text".to_string())?;
    let (pack, warnings) = parse(&text).map_err(|e| format!("the pack isn't valid: {e}"))?;
    if pack.game.id != entry.id {
        return Err(format!(
            "the list says {} but the pack is {}",
            entry.id, pack.game.id
        ));
    }
    Ok((pack, text, warnings))
}

/// An index entry describing a pack (for building `index.json`).
pub fn entry_for(pack: &Pack, text: &str) -> Entry {
    Entry {
        id: pack.game.id.clone(),
        name: pack.game.name.clone(),
        exe: pack.game.exe.clone(),
        arch: pack.game.arch,
        engine: pack.game.engine.clone(),
        author: pack.meta.author.clone(),
        pack_version: pack.meta.pack_version.clone(),
        cheats: pack.cheats.len(),
        notes: pack.meta.notes.clone(),
        source: pack
            .meta
            .source
            .clone()
            .map(|s| crate::schema::Source { file: None, ..s }),
        sha256: ot_core::hash::sha256_bytes(text.as_bytes()),
        size: text.len() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::BUNDLED;

    fn sample() -> (Entry, &'static str) {
        let text = BUNDLED[0].1;
        let (pack, _) = parse(text).unwrap();
        (entry_for(&pack, text), text)
    }

    #[test]
    fn reads_an_index_and_skips_bad_entries() {
        let (good, _) = sample();
        let mut bad_id = good.clone();
        bad_id.id = "../../evil".into();
        let mut bad_sha = good.clone();
        bad_sha.id = "other".into();
        bad_sha.sha256 = "nope".into();
        let twice = good.clone();
        let index = Index {
            format: 1,
            updated: Some("2026-11-07".into()),
            packs: vec![good.clone(), bad_id, bad_sha, twice],
            tables: Vec::new(),
            web: Vec::new(),
        };
        let mut json = serde_json::to_value(&index).unwrap();
        json["futureField"] = serde_json::json!(true);
        let (read, skipped) = parse_index(json.to_string().as_bytes()).unwrap();
        assert_eq!(read.packs, vec![good]);
        assert_eq!(skipped.len(), 3, "{skipped:?}");
        assert!(skipped[0].contains("bad id"));

        let newer = br#"{"format":2,"packs":[]}"#;
        assert!(
            parse_index(newer)
                .unwrap_err()
                .contains("newer OpenTrainer")
        );
        assert!(parse_index(b"<html>").is_err());
    }

    fn link(id: &str, url: &str) -> TableLink {
        TableLink {
            id: id.into(),
            game: "Hollow Knight".into(),
            exe: Some("hollow_knight.exe".into()),
            title: "Hollow Knight v1.5 table".into(),
            by: "TableMaker".into(),
            url: url.into(),
            cheats: Some(12),
            added: Some("2026-11-09".into()),
        }
    }

    #[test]
    fn table_links_only_to_known_sites_over_https() {
        assert_eq!(
            table_site("https://fearlessrevolution.com/viewtopic.php?t=1"),
            Some("fearlessrevolution.com")
        );
        assert_eq!(
            table_site("https://FORUM.CheatEngine.org/viewtopic.php?t=1"),
            Some("forum.cheatengine.org")
        );
        for bad in [
            "http://fearlessrevolution.com/viewtopic.php?t=1",
            "https://fearlessrevolution.com.evil.example/x",
            "https://evil.example/fearlessrevolution.com",
            "https://user@fearlessrevolution.com/x",
            "javascript:alert(1)",
            "https://fearlessrevolution.com/a b",
        ] {
            assert_eq!(table_site(bad), None, "{bad}");
        }
        assert_eq!(
            table_problem(&link("hk-1", "https://fearlessrevolution.com/t")),
            None
        );
        let mut bad_exe = link("hk-2", "https://fearlessrevolution.com/t");
        bad_exe.exe = Some(r"C:\x\hk.exe".into());
        assert_eq!(table_problem(&bad_exe), Some("bad exe"));
        let mut two_lines = link("hk-3", "https://fearlessrevolution.com/t");
        two_lines.by = "a\nb".into();
        assert!(table_problem(&two_lines).is_some());
    }

    #[test]
    fn reads_table_links_and_skips_bad_ones() {
        let json = serde_json::json!({
            "format": 1,
            "packs": [],
            "tables": [
                link("zz-1", "https://fearlessrevolution.com/viewtopic.php?t=2"),
                link("aa-1", "https://evil.example/t"),
                link("Bad Id", "https://fearlessrevolution.com/viewtopic.php?t=3"),
                link("zz-2", "https://fearlessrevolution.com/viewtopic.php?t=2"),
            ],
        });
        let (read, skipped) = parse_index(json.to_string().as_bytes()).unwrap();
        assert_eq!(read.tables.len(), 1);
        assert_eq!(read.tables[0].id, "zz-1");
        assert_eq!(skipped.len(), 3, "{skipped:?}");
        // An older list without links still loads.
        let (old, _) = parse_index(br#"{"format":1,"packs":[]}"#).unwrap();
        assert!(old.tables.is_empty());
        assert!(old.web.is_empty());
    }

    #[test]
    fn reads_web_cheats_and_skips_bad_ones() {
        let web = |id: &str, game: &str, url: &str| {
            serde_json::json!({
                "format": 1, "id": id, "game": game, "url": url, "by": "Sam",
                "cheats": [{ "name": "Gold", "target": { "k": "j", "path": ["game", "gold"] }, "value": 999 }],
            })
        };
        let json = serde_json::json!({
            "format": 1,
            "packs": [],
            "web": [
                web("zed-1", "Zed", "https://zed.example/"),
                web("alpha-1", "alpha", "https://alpha.example/play"),
                web("local-1", "Local", "http://127.0.0.1:8080/"),
                web("zed-1", "Zed again", "https://zed.example/2"),
                { "format": 1, "id": "broken" },
                web("future-1", "Future", "https://future.example/").as_object().map(|o| {
                    let mut o = o.clone();
                    o.insert("format".into(), 2.into());
                    serde_json::Value::Object(o)
                }).unwrap(),
            ],
        });
        let (read, skipped) = parse_index(json.to_string().as_bytes()).unwrap();
        let ids: Vec<&str> = read.web.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["alpha-1", "zed-1"]);
        assert_eq!(read.web[1].game, "Zed");
        // The local game, the second zed-1 and the newer format are named; the broken one is just left out.
        assert_eq!(skipped.len(), 3, "{skipped:?}");
    }

    #[test]
    fn table_ids_are_slugs_unique_per_page() {
        let a = table_id(
            "Baldur's Gate 3",
            "https://fearlessrevolution.com/viewtopic.php?t=1",
        );
        let b = table_id(
            "Baldur's Gate 3",
            "https://fearlessrevolution.com/viewtopic.php?t=2",
        );
        assert!(a.starts_with("baldur-s-gate-3-"), "{a}");
        assert_ne!(a, b);
        assert!(is_slug(&a) && is_slug(&b));
        assert!(table_id("???", "https://x").starts_with("table-"));
        assert!(is_slug(&table_id(&"x".repeat(200), "https://x")));
    }

    #[test]
    fn builds_pack_urls_next_to_the_index() {
        let (e, _) = sample();
        assert_eq!(
            pack_url(INDEX_URL, &e),
            "https://raw.githubusercontent.com/jackuri/opentrainer-packs/main/packs/testbed-native/pack.json"
        );
        assert_eq!(
            pack_url("http://127.0.0.1:9/x/index.json", &e),
            "http://127.0.0.1:9/x/packs/testbed-native/pack.json"
        );
    }

    #[test]
    fn checks_downloads_against_the_index() {
        let (e, text) = sample();
        let (pack, got, _) = check_download(&e, text.as_bytes()).unwrap();
        assert_eq!(pack.game.id, "testbed-native");
        assert_eq!(got, text);
        let tampered = text.replacen("Infinite", "Infinitе", 1);
        assert!(
            check_download(&e, tampered.as_bytes())
                .unwrap_err()
                .contains("doesn't match")
        );
        let mut other = e.clone();
        other.id = "other".into();
        assert!(
            check_download(&other, text.as_bytes())
                .unwrap_err()
                .contains("the list says other")
        );
    }
}
