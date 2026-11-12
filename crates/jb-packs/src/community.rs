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
    "https://raw.githubusercontent.com/jackuri/juicebox-packs/main/index.json";
/// Where people go to add a pack to the list.
pub const CONTRIBUTE_URL: &str = "https://github.com/jackuri/juicebox-packs#add-your-pack";
/// The index format this build reads.
pub const FORMAT: u32 = 1;
/// The index refuses to load above this size.
pub const MAX_INDEX_BYTES: u64 = 2 << 20;

/// Where the app keeps the last community list it downloaded
/// (`%LOCALAPPDATA%\JuiceBox Trainer\community-index.json`, v1.21.0), so the AI
/// helper can see what's already shared for a game without going online.
pub fn cache_path() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| {
        std::path::PathBuf::from(d)
            .join(jb_core::names::FOLDER)
            .join("community-index.json")
    })
}

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
    /// Cheats JuiceBox Trainer could use from it when it was shared.
    #[serde(default)]
    pub cheats: Option<usize>,
    /// `YYYY-MM-DD`.
    #[serde(default)]
    pub added: Option<String>,
    /// The table's terms (v1.27.0). Needed when the table itself is on the
    /// list ([`file`](Self::file)): one of [`HOSTING_LICENCES`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licence: Option<String>,
    /// The table itself, on the list (`tables/<id>.CT`), when its maker gave
    /// it an open licence or said it may be shared there. Otherwise the table
    /// stays on its maker's page only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// SHA-256 of [`file`](Self::file).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// Terms under which a table itself may be on the community list (v1.27.0):
/// open licences that allow sharing copies, or the maker's permission.
pub const HOSTING_LICENCES: &[&str] = &[
    "CC-BY-4.0",
    "CC0-1.0",
    "CC-BY-SA-4.0",
    "MIT",
    "Apache-2.0",
    "GPL-3.0-or-later",
    crate::schema::LICENCE_PERMISSION,
];

/// The biggest table file the list may carry.
pub const MAX_TABLE_FILE: u64 = 2 << 20;

impl TableLink {
    /// Where the table itself would be on the list.
    pub fn file_path(&self) -> String {
        format!("tables/{}.CT", self.id)
    }
}

/// The address of a listed table file, next to the index.
pub fn table_file_url(index_url: &str, link: &TableLink) -> Option<String> {
    let file = link.file.as_deref()?;
    let base = index_url.rsplit_once('/').map_or(index_url, |(b, _)| b);
    Some(format!("{base}/{file}"))
}

/// Checks a downloaded table file against its link.
pub fn check_table_file(link: &TableLink, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 > MAX_TABLE_FILE {
        return Err("the table is too big".into());
    }
    if link.sha256.as_deref() != Some(jb_core::hash::sha256_bytes(bytes).as_str()) {
        return Err(
            "the downloaded table doesn't match the list (it may have just been updated; refresh and try again)"
                .into(),
        );
    }
    Ok(())
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
    } else if t
        .licence
        .as_deref()
        .is_some_and(|l| !one_line(l, crate::schema::MAX_SOURCE_LICENCE))
    {
        Some("bad licence")
    } else if let Some(file) = &t.file {
        if *file != t.file_path() {
            Some("the table file must be tables/<id>.CT")
        } else if !t.sha256.as_deref().is_some_and(is_sha256) {
            Some("the table file needs its SHA-256")
        } else if !t
            .licence
            .as_deref()
            .is_some_and(|l| HOSTING_LICENCES.contains(&l))
        {
            Some("a table on the list needs an open licence or its maker's permission")
        } else {
            None
        }
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
    let hash = &jb_core::hash::sha256_bytes(url.as_bytes())[..10];
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
            "the pack list needs a newer JuiceBox Trainer; update JuiceBox Trainer".into()
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
    let sha = jb_core::hash::sha256_bytes(bytes);
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
        sha256: jb_core::hash::sha256_bytes(text.as_bytes()),
        size: text.len() as u64,
    }
}

// ----- most wanted (v1.25.0) ------------------------------------------------------------------

/// Where players ask for cheats: the community repository's issues.
pub const REQUESTS_URL: &str = "https://github.com/jackuri/juicebox-packs/issues";
/// `wanted.json` refuses to load above this size.
pub const MAX_WANTED_BYTES: u64 = 512 << 10;
/// At most this many wanted games are shown.
pub const MAX_WANTED: usize = 200;

/// A game players asked cheats for (from `wanted.json`, built from the
/// community repository's "request" issues). Written by anyone: only checked,
/// short text is kept, and links are made from the issue number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wanted {
    pub game: String,
    #[serde(default)]
    pub exe: Option<String>,
    /// Players who asked (the request and its thumbs-up).
    pub votes: u32,
    /// The request's issue number.
    pub number: u64,
    /// When it was asked (YYYY-MM-DD).
    #[serde(default)]
    pub asked: Option<String>,
}

/// `wanted.json` next to the `index.json` at `index_url`.
/// How players rate a community pack (v1.29.0), from `ratings.json` next to
/// the index: 1 star broken, 2 works with bugs, 3 works great.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rating {
    /// The pack's rating issue (a reaction on it is a rating).
    pub issue: u64,
    /// Ratings counted (one per GitHub account).
    pub count: u32,
    /// The average, 1 to 3 (0 without ratings).
    pub stars: f32,
    /// How many gave 1, 2 and 3 stars.
    pub votes: [u32; 3],
}

/// The ratings list refuses to load above this size.
pub const MAX_RATINGS_BYTES: u64 = 1 << 20;

/// `ratings.json`, next to `index.json`.
pub fn ratings_url(index_url: &str) -> String {
    match index_url.strip_suffix("index.json") {
        Some(base) => format!("{base}ratings.json"),
        None => format!("{}/ratings.json", index_url.trim_end_matches('/')),
    }
}

/// Reads `ratings.json` leniently: an entry that doesn't add up is left out.
pub fn parse_ratings(bytes: &[u8]) -> Result<std::collections::BTreeMap<String, Rating>, String> {
    if bytes.len() as u64 > MAX_RATINGS_BYTES {
        return Err("the ratings list is too big".into());
    }
    let v: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("the ratings list isn't valid: {e}"))?;
    if v["format"].as_u64() != Some(1) {
        return Err("the ratings list needs a newer JuiceBox Trainer".into());
    }
    let mut out = std::collections::BTreeMap::new();
    if let Some(map) = v["ratings"].as_object() {
        for (id, r) in map {
            let Ok(r) = serde_json::from_value::<Rating>(r.clone()) else {
                continue;
            };
            let total: u32 = r.votes.iter().sum();
            let ok = is_slug(id)
                && r.issue > 0
                && total == r.count
                && (r.count == 0 || (1.0..=3.0).contains(&r.stars));
            if ok {
                out.insert(id.clone(), r);
            }
        }
    }
    Ok(out)
}

/// A player's packer XP (v1.32.0), from `xp.json` next to the index: counted
/// by the community list's robot from GitHub (accepted packs, ratings).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    /// Their GitHub name.
    pub login: String,
    pub xp: u64,
    /// Packs and web cheats of theirs accepted into the list.
    pub packs: u32,
    /// Packs they rated.
    pub ratings: u32,
}

/// The XP list refuses to load above this size.
pub const MAX_XP_BYTES: u64 = 1 << 20;
/// The most XP anyone can have (well past level 99).
const MAX_XP: u64 = 10_000_000;

/// `xp.json`, next to `index.json`.
pub fn xp_url(index_url: &str) -> String {
    match index_url.strip_suffix("index.json") {
        Some(base) => format!("{base}xp.json"),
        None => format!("{}/xp.json", index_url.trim_end_matches('/')),
    }
}

/// A GitHub name: letters, digits and dashes, up to 39, not starting with a dash.
fn is_login(s: &str) -> bool {
    (1..=39).contains(&s.len())
        && !s.starts_with('-')
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Reads `xp.json` leniently: an entry that doesn't look right is left out.
/// Most XP first.
pub fn parse_xp(bytes: &[u8]) -> Result<Vec<Player>, String> {
    if bytes.len() as u64 > MAX_XP_BYTES {
        return Err("the XP list is too big".into());
    }
    let v: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("the XP list isn't valid: {e}"))?;
    if v["format"].as_u64() != Some(1) {
        return Err("the XP list needs a newer JuiceBox Trainer".into());
    }
    let mut out: Vec<Player> = v["players"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| serde_json::from_value::<Player>(p.clone()).ok())
        .filter(|p| is_login(&p.login) && p.xp <= MAX_XP)
        .collect();
    out.sort_by(|a, b| b.xp.cmp(&a.xp).then_with(|| a.login.cmp(&b.login)));
    out.dedup_by(|a, b| a.login.eq_ignore_ascii_case(&b.login));
    out.truncate(1000);
    Ok(out)
}

pub fn wanted_url(index_url: &str) -> String {
    match index_url.strip_suffix("index.json") {
        Some(base) => format!("{base}wanted.json"),
        None => format!("{}/wanted.json", index_url.trim_end_matches('/')),
    }
}

/// A game name safe to show: one line, no control or direction characters,
/// 2 to 80 characters.
fn clean_game(s: &str) -> Option<String> {
    let t: String = s
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    (2..=80).contains(&t.chars().count()).then_some(t)
}

/// A plain exe file name, or `None`.
fn clean_exe(s: &str) -> Option<String> {
    let t = s.trim();
    let ok = t.len() <= 84
        && t.to_ascii_lowercase().ends_with(".exe")
        && t.len() > 4
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || " ._()+-".contains(c));
    ok.then(|| t.to_string())
}

/// Parses `wanted.json` leniently: bad entries are left out, the rest are
/// cleaned and sorted by votes (most first).
pub fn parse_wanted(bytes: &[u8]) -> Result<Vec<Wanted>, String> {
    if bytes.len() as u64 > MAX_WANTED_BYTES {
        return Err("the most-wanted list is too big".into());
    }
    let v: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| format!("the most-wanted list isn't valid: {e}"))?;
    if v.get("format").and_then(serde_json::Value::as_u64) != Some(1) {
        return Err("the most-wanted list has an unknown format".into());
    }
    let mut list: Vec<Wanted> = v
        .get("wanted")
        .and_then(serde_json::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| serde_json::from_value::<Wanted>(e.clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    list.retain_mut(|w| {
        let Some(game) = clean_game(&w.game) else {
            return false;
        };
        w.game = game;
        w.exe = w.exe.as_deref().and_then(clean_exe);
        w.asked = w
            .asked
            .take()
            .filter(|d| d.len() == 10 && d.bytes().all(|b| b.is_ascii_digit() || b == b'-'));
        w.votes = w.votes.max(1);
        w.number > 0
    });
    list.sort_by(|a, b| b.votes.cmp(&a.votes).then(a.number.cmp(&b.number)));
    list.truncate(MAX_WANTED);
    Ok(list)
}

/// Percent-encodes a URL query value (UTF-8).
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The page that asks the community for cheats for a game: the request form,
/// filled in with the game's name (and exe). `None` for a name that can't be
/// a game's.
pub fn request_url(game: &str, exe: Option<&str>) -> Option<String> {
    let game = clean_game(game)?;
    let mut url = format!(
        "{REQUESTS_URL}/new?template=cheat-request.yml&labels=request&title={}&game={}",
        encode(&format!("Cheats for: {game}")),
        encode(&game)
    );
    if let Some(exe) = exe.and_then(clean_exe) {
        url.push_str(&format!("&exe={}", encode(&exe)));
    }
    Some(url)
}

/// A request's page (made from its number, never from what people wrote).
pub fn request_page(number: u64) -> String {
    format!("{REQUESTS_URL}/{number}")
}

#[cfg(test)]
mod wanted_tests {
    use super::*;

    #[test]
    fn reads_the_most_wanted_list_carefully() {
        let json = br#"{"format":1,"updated":"2026-11-11","wanted":[
            {"game":"Coin Quest","exe":"coin.exe","votes":1,"number":5,"url":"https://evil.example/","asked":"2026-11-10"},
            {"game":"Hollow\u202e Knight","exe":"C:\\evil\\x.exe","votes":7,"number":4},
            {"game":"x","votes":9,"number":6},
            {"nonsense":true},
            {"game":"No Number","votes":3,"number":0}
        ]}"#;
        let list = parse_wanted(json).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].game, "Hollow Knight");
        assert_eq!(list[0].votes, 7);
        assert_eq!(list[0].exe, None);
        assert_eq!(list[1].exe.as_deref(), Some("coin.exe"));
        assert_eq!(
            request_page(list[1].number),
            "https://github.com/jackuri/juicebox-packs/issues/5"
        );
        assert!(parse_wanted(br#"{"format":2,"wanted":[]}"#).is_err());
    }

    #[test]
    fn asks_with_a_filled_in_form() {
        let url = request_url("Hollow Knight & Co", Some("hollow_knight.exe")).unwrap();
        assert_eq!(
            url,
            "https://github.com/jackuri/juicebox-packs/issues/new?template=cheat-request.yml&labels=request&title=Cheats%20for%3A%20Hollow%20Knight%20%26%20Co&game=Hollow%20Knight%20%26%20Co&exe=hollow_knight.exe"
        );
        assert!(request_url("x", None).is_none());
        assert!(
            !request_url("Game", Some("..\\a.bat"))
                .unwrap()
                .contains("exe=")
        );
        assert_eq!(
            wanted_url(INDEX_URL),
            "https://raw.githubusercontent.com/jackuri/juicebox-packs/main/wanted.json"
        );
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn reads_the_xp_list_leniently() {
        assert_eq!(
            xp_url("https://raw.example/main/index.json"),
            "https://raw.example/main/xp.json"
        );
        let list = parse_xp(
            br#"{"format":1,"updated":"2026-11-12","rules":{"pack":10,"rating":2},"players":[
                {"login":"ann","xp":14,"packs":1,"ratings":2},
                {"login":"jackuri","xp":20,"packs":2,"ratings":0},
                {"login":"-bad","xp":5,"packs":0,"ratings":0},
                {"login":"huge","xp":99999999999,"packs":0,"ratings":0},
                {"login":"Ann","xp":1,"packs":0,"ratings":0},
                {"name":"no login"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(
            list.iter()
                .map(|p| (p.login.as_str(), p.xp))
                .collect::<Vec<_>>(),
            [("jackuri", 20), ("ann", 14)]
        );
        assert!(parse_xp(br#"{"format":2,"players":[]}"#).is_err());
        assert!(parse_xp(b"nope").is_err());
    }
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
                .contains("newer JuiceBox Trainer")
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
            licence: None,
            file: None,
            sha256: None,
        }
    }

    #[test]
    fn reads_ratings_and_skips_ones_that_dont_add_up() {
        assert_eq!(
            ratings_url("https://raw.example/main/index.json"),
            "https://raw.example/main/ratings.json"
        );
        let json = br#"{"format":1,"updated":"2026-11-12","ratings":{
            "coin-quest":{"issue":12,"count":3,"stars":2.3,"votes":[1,0,2]},
            "new-pack":{"issue":13,"count":0,"stars":0,"votes":[0,0,0]},
            "bad-sum":{"issue":14,"count":5,"stars":2,"votes":[1,1,1]},
            "too-many-stars":{"issue":15,"count":1,"stars":9,"votes":[0,0,1]},
            "Bad Id":{"issue":16,"count":1,"stars":3,"votes":[0,0,1]},
            "no-issue":{"issue":0,"count":0,"stars":0,"votes":[0,0,0]},
            "junk":"x"}}"#;
        let r = parse_ratings(json).unwrap();
        assert_eq!(r.keys().collect::<Vec<_>>(), ["coin-quest", "new-pack"]);
        assert_eq!(r["coin-quest"].votes, [1, 0, 2]);
        assert!(parse_ratings(br#"{"format":2,"ratings":{}}"#).is_err());
        assert!(parse_ratings(b"<html>").is_err());
    }

    #[test]
    fn table_files_need_an_open_licence_and_their_hash() {
        let table = b"<CheatTable></CheatTable>";
        let mut t = link("hk-1", "https://fearlessrevolution.com/t");
        t.file = Some("tables/hk-1.CT".into());
        t.sha256 = Some(jb_core::hash::sha256_bytes(table));
        assert!(table_problem(&t).unwrap().contains("open licence"));
        t.licence = Some("All rights reserved".into());
        assert!(table_problem(&t).unwrap().contains("open licence"));
        t.licence = Some("CC-BY-4.0".into());
        assert_eq!(table_problem(&t), None);
        t.licence = Some(crate::schema::LICENCE_PERMISSION.into());
        assert_eq!(table_problem(&t), None);

        let mut elsewhere = t.clone();
        elsewhere.file = Some("packs/x/pack.json".into());
        assert!(
            table_problem(&elsewhere)
                .unwrap()
                .contains("tables/<id>.CT")
        );
        let mut no_hash = t.clone();
        no_hash.sha256 = None;
        assert!(table_problem(&no_hash).unwrap().contains("SHA-256"));

        assert_eq!(
            table_file_url("https://raw.example/main/index.json", &t).as_deref(),
            Some("https://raw.example/main/tables/hk-1.CT")
        );
        assert_eq!(
            table_file_url("https://raw.example/index.json", &link("a", "b")),
            None
        );
        assert!(check_table_file(&t, table).is_ok());
        assert!(check_table_file(&t, b"<CheatTable>changed</CheatTable>").is_err());

        // A licence alone (the table stays on its maker's page) is fine.
        let mut terms_only = link("hk-2", "https://fearlessrevolution.com/t2");
        terms_only.licence = Some("Ask before sharing".into());
        assert_eq!(table_problem(&terms_only), None);
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
            "https://raw.githubusercontent.com/jackuri/juicebox-packs/main/packs/testbed-native/pack.json"
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
