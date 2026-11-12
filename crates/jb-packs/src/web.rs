//! Web cheat packs (v1.19.0): the cheats a player found for a game that runs
//! in a web page, as a small file (`.otweb`) to give to friends or to share on
//! the community list (`web/<id>.json`, listed in the index's `web`).
//!
//! It's only data: where each number is (a place in one of the page's
//! memories, or a JavaScript value's path) and the value to set or lock it to.
//! JuiceBox Trainer's own helper in the game page does the reading and writing.

use serde::{Deserialize, Serialize};

use crate::Hotkey;
use crate::validate::is_slug;

/// The file format this build writes and reads.
pub const FORMAT: u32 = 1;
/// The file extension for a web cheat pack.
pub const EXTENSION: &str = "juiceweb";
/// OpenTrainer's web cheats extension (before 1.23.0): still opened.
pub const LEGACY_EXTENSION: &str = "otweb";
/// A web cheat pack file refuses to load above this size.
pub const MAX_BYTES: usize = 256 * 1024;
/// Cheats a player can keep for one web game.
pub const MAX_CHEATS: usize = 200;
/// Cheats a pack on the community list may have.
pub const MAX_SHARED_CHEATS: usize = 50;
/// Longest web address in a pack.
pub const MAX_URL: usize = 1000;

/// Where a number lives in a web game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k", deny_unknown_fields)]
pub enum WebTarget {
    /// In one of the page's memories (WebAssembly or a big byte array).
    #[serde(rename = "m")]
    Memory {
        mem: u32,
        label: String,
        /// The memory's size when it was found (a hint that it's the same one).
        size: u64,
        off: u64,
        t: NumType,
        /// A group (v1.31.0): `count` places, `every` bytes apart from `off`
        /// (all the enemies in a list, say). Both or neither.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        every: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<u32>,
    },
    /// A JavaScript value, by its path from `window`. A `*` in the path
    /// stands for every item of the list or object there (v1.31.0).
    #[serde(rename = "j")]
    Js { path: Vec<String> },
}

/// The most places one group cheat may cover.
pub const MAX_GROUP: u32 = 1024;
/// A path's stand-in for every item of a list (`game.zombies.*.hp`).
pub const EVERY_ITEM: &str = "*";

impl WebTarget {
    /// How many places it covers (`None` for "every item", which depends on the game).
    pub fn places(&self) -> Option<u32> {
        match self {
            WebTarget::Memory { count, .. } => Some(count.unwrap_or(1)),
            WebTarget::Js { path } if path.iter().any(|p| p == EVERY_ITEM) => None,
            WebTarget::Js { .. } => Some(1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NumType {
    U8,
    I16,
    I32,
    F32,
    F64,
}

/// What a cheat's key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum KeyDoes {
    /// Locks the number (at its value), or unlocks it.
    #[default]
    Lock,
    /// Sets the number to its value once.
    Set,
}

/// How a lock keeps its number (v1.31.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Keep {
    /// At the value.
    #[default]
    At,
    /// Never above it (lowers it, never raises it: enemies' health).
    AtMost,
    /// Never below it (raises it, never lowers it).
    AtLeast,
}

/// A cheat for a web game. Unknown fields are ignored, so files from newer
/// versions still open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebCheat {
    pub name: String,
    pub target: WebTarget,
    /// The value it's usually set to.
    #[serde(default)]
    pub value: Option<f64>,
    /// A key that works while the game window is in front (`F1`, `Ctrl+1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_does: Option<KeyDoes>,
    /// How its lock keeps the number (at it when absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep: Option<Keep>,
}

fn one_line(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.chars().count() <= max && !s.chars().any(char::is_control)
}

/// Checks a cheat and returns it tidied (name trimmed, key in its usual spelling).
pub fn check_cheat(c: &WebCheat) -> Result<WebCheat, String> {
    let mut c = c.clone();
    c.name = c.name.trim().to_string();
    if !one_line(&c.name, 80) {
        return Err("give each cheat a name (up to 80 characters)".into());
    }
    let bad_place = || format!("{}: that cheat's place isn't valid", c.name);
    match &c.target {
        WebTarget::Memory {
            label,
            off,
            every,
            count,
            ..
        } => {
            if label.chars().count() > 200
                || label.chars().any(char::is_control)
                || *off > (1 << 40)
            {
                return Err(bad_place());
            }
            match (every, count) {
                (None, None) => {}
                (Some(e), Some(n))
                    if (1..=(1 << 20)).contains(e)
                        && (2..=MAX_GROUP).contains(n)
                        && off + e * u64::from(*n) <= (1 << 40) => {}
                _ => {
                    return Err(format!(
                        "{}: a group needs \"every\" (1 to 1048576 bytes) and \"count\" (2 to {MAX_GROUP} places)",
                        c.name
                    ));
                }
            }
        }
        WebTarget::Js { path } => {
            let stars = path.iter().filter(|p| *p == EVERY_ITEM).count();
            if path.is_empty()
                || path.len() > 16
                || path.iter().any(|p| {
                    p.is_empty() || p.chars().count() > 120 || p.chars().any(char::is_control)
                })
                || stars > 2
                || path.first().is_some_and(|p| p == EVERY_ITEM)
                || path.last().is_some_and(|p| p == EVERY_ITEM)
            {
                return Err(bad_place());
            }
        }
    }
    if c.value.is_some_and(|v| !v.is_finite()) {
        return Err(format!("{}: that value isn't a number", c.name));
    }
    match c.key.as_deref().map(str::trim) {
        None | Some("") => {
            c.key = None;
            c.key_does = None;
        }
        Some(k) => {
            let hk = Hotkey::parse(k).map_err(|e| format!("{}: {e}", c.name))?;
            c.key = Some(hk.to_string());
            if c.key_does == Some(KeyDoes::Set) && c.value.is_none() {
                return Err(format!(
                    "{}: its key sets a value, so it needs one (set it once first)",
                    c.name
                ));
            }
        }
    }
    Ok(c)
}

/// The host of a web game's address (`outbreakweb.com`), lower case and
/// without a port, or `None` if it isn't an http(s) address without sign-in
/// details.
pub fn host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let host = if let Some(v6) = authority.strip_prefix('[') {
        format!("[{}]", v6.split(']').next()?)
    } else {
        let (h, port) = authority.split_once(':').unwrap_or((authority, ""));
        if !port.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        h.to_string()
    }
    .to_ascii_lowercase();
    let ok = host == "[::1]"
        || (!host.is_empty()
            && !host.starts_with(['.', '-'])
            && host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-'));
    ok.then_some(host)
}

/// The game runs on this PC (`localhost`, `127.0.0.1`, `[::1]`).
pub fn is_local(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

/// Why a web game's address can't be in a pack, if it can't: https, or http on
/// this PC (not for the community list).
pub fn url_problem(url: &str, shared: bool) -> Option<&'static str> {
    if url.len() > MAX_URL
        || url
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '"' | '<' | '>'))
    {
        return Some("the web address isn't valid");
    }
    let Some(h) = host(url) else {
        return Some("the web address isn't valid");
    };
    if shared && is_local(&h) {
        return Some("a game on your own PC can't be shared with everyone");
    }
    if !url.starts_with("https://") && !is_local(&h) {
        return Some("the web address must start with https://");
    }
    None
}

/// A web cheat pack (an `.otweb` file, or an entry in the community list).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPack {
    pub format: u32,
    /// A slug: the game's name and a short hash of its address.
    pub id: String,
    /// The game's name.
    pub game: String,
    /// The game's web address.
    pub url: String,
    /// Who found the cheats.
    pub by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<String>,
    pub cheats: Vec<WebCheat>,
}

/// An id for a game's pack: its name and a short hash of its address.
pub fn pack_id(game: &str, url: &str) -> String {
    crate::community::table_id(game, url)
}

/// Checks a pack (`shared`: the community list's stricter rules) and returns
/// it tidied.
pub fn check_pack(p: &WebPack, shared: bool) -> Result<WebPack, String> {
    let mut p = p.clone();
    if p.format != FORMAT {
        return Err(if p.format > FORMAT {
            "these cheats need a newer JuiceBox Trainer; update JuiceBox Trainer".into()
        } else {
            format!("unknown web cheats format ({})", p.format)
        });
    }
    if !is_slug(&p.id) {
        return Err("the pack's id isn't valid".into());
    }
    p.game = p.game.trim().to_string();
    p.by = p.by.trim().to_string();
    if !one_line(&p.game, 120) {
        return Err("the game needs a name (up to 120 characters)".into());
    }
    if !one_line(&p.by, 100) {
        return Err("say who found the cheats (up to 100 characters)".into());
    }
    p.notes = p
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(String::from);
    if p.notes.as_deref().is_some_and(|n| !one_line(n, 500)) {
        return Err("the notes must be one line (up to 500 characters)".into());
    }
    if p.added.as_deref().is_some_and(|d| {
        d.len() != 10
            || !d.char_indices().all(|(i, c)| {
                if i == 4 || i == 7 {
                    c == '-'
                } else {
                    c.is_ascii_digit()
                }
            })
    }) {
        return Err("the date isn't valid".into());
    }
    if let Some(why) = url_problem(&p.url, shared) {
        return Err(why.into());
    }
    let max = if shared {
        MAX_SHARED_CHEATS
    } else {
        MAX_CHEATS
    };
    if p.cheats.is_empty() {
        return Err("there are no cheats in it".into());
    }
    if p.cheats.len() > max {
        return Err(format!("too many cheats (at most {max})"));
    }
    p.cheats = p.cheats.iter().map(check_cheat).collect::<Result<_, _>>()?;
    Ok(p)
}

/// Reads an `.otweb` file.
pub fn parse_pack(bytes: &[u8]) -> Result<WebPack, String> {
    if bytes.len() > MAX_BYTES {
        return Err("the file is too big to be web cheats".into());
    }
    let pack: WebPack = serde_json::from_slice(bytes)
        .map_err(|_| "this file isn't JuiceBox Trainer web cheats".to_string())?;
    check_pack(&pack, false)
}

/// A file name for a pack (`<id>.otweb`).
pub fn file_name(p: &WebPack) -> String {
    format!("{}.{EXTENSION}", p.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cheat(name: &str) -> WebCheat {
        WebCheat {
            name: name.into(),
            target: WebTarget::Js {
                path: vec!["game".into(), "gold".into()],
            },
            value: Some(999.0),
            key: None,
            key_does: None,
            keep: None,
        }
    }

    fn pack() -> WebPack {
        WebPack {
            format: FORMAT,
            id: pack_id("Outbreak", "https://outbreakweb.com/"),
            game: "Outbreak".into(),
            url: "https://outbreakweb.com/".into(),
            by: "Sam".into(),
            notes: None,
            added: Some("2026-11-10".into()),
            cheats: vec![cheat("Gold")],
        }
    }

    #[test]
    fn hosts_and_addresses() {
        assert_eq!(
            host("https://OutbreakWeb.com/file1/?a=1").as_deref(),
            Some("outbreakweb.com")
        );
        assert_eq!(host("http://127.0.0.1:8080/").as_deref(), Some("127.0.0.1"));
        assert_eq!(host("http://[::1]:80/x").as_deref(), Some("[::1]"));
        assert_eq!(host("https://user@site.com/"), None);
        assert_eq!(host("https://site.com:x/"), None);
        assert_eq!(host("ftp://site.com/"), None);
        assert_eq!(host("https:///x"), None);
        assert_eq!(url_problem("https://site.com/game", true), None);
        assert_eq!(url_problem("http://localhost:8080/", false), None);
        assert!(url_problem("http://localhost:8080/", true).is_some());
        assert!(url_problem("http://site.com/", false).is_some());
        assert!(url_problem("https://site.com/a b", false).is_some());
        assert!(url_problem("javascript:alert(1)", false).is_some());
    }

    #[test]
    fn cheats_are_checked_and_tidied() {
        let mut c = cheat("  Gold ");
        c.key = Some("ctrl+f1".into());
        let tidy = check_cheat(&c).unwrap();
        assert_eq!(tidy.name, "Gold");
        assert_eq!(tidy.key.as_deref(), Some("Ctrl+F1"));

        c.key = Some("Hyper+Q".into());
        assert!(check_cheat(&c).is_err());
        c.key = Some(" ".into());
        c.key_does = Some(KeyDoes::Set);
        let none = check_cheat(&c).unwrap();
        assert_eq!((none.key, none.key_does), (None, None));

        // A key that sets a value needs the value.
        c.key = Some("F2".into());
        c.value = None;
        assert!(check_cheat(&c).is_err());
        c.key_does = Some(KeyDoes::Lock);
        assert!(check_cheat(&c).is_ok());

        c.value = Some(f64::NAN);
        assert!(check_cheat(&c).is_err());
        let mut bad = cheat("x");
        bad.target = WebTarget::Js { path: vec![] };
        assert!(check_cheat(&bad).is_err());
        bad.name = "a\nb".into();
        assert!(check_cheat(&bad).is_err());
    }

    #[test]
    fn group_places_are_checked() {
        let mut c = cheat("Weak enemies");
        c.target = serde_json::from_str(
            r#"{"k":"m","mem":0,"label":"memory","size":16777216,"off":7124308,"t":"i16","every":3472,"count":5}"#,
        )
        .unwrap();
        assert_eq!(c.target.places(), Some(5));
        let tidy = check_cheat(&c).unwrap();
        let text = serde_json::to_string(&tidy.target).unwrap();
        assert!(text.contains(r#""every":3472,"count":5"#), "{text}");
        // A single place is written as before.
        let one = serde_json::to_string(&cheat("Gold").target).unwrap();
        assert!(!one.contains("every"));
        let group = |every: Option<u64>, count: Option<u32>| {
            let mut c = cheat("x");
            c.target = WebTarget::Memory {
                mem: 0,
                label: "m".into(),
                size: 1,
                off: 0,
                t: NumType::I16,
                every,
                count,
            };
            check_cheat(&c)
        };
        assert!(group(Some(2), Some(MAX_GROUP)).is_ok());
        assert!(group(Some(2), None).is_err());
        assert!(group(None, Some(3)).is_err());
        assert!(group(Some(0), Some(3)).is_err());
        assert!(group(Some(2), Some(1)).is_err());
        assert!(group(Some(2), Some(MAX_GROUP + 1)).is_err());

        let js = |path: &[&str]| {
            let mut c = cheat("x");
            c.target = WebTarget::Js {
                path: path.iter().map(|s| (*s).to_string()).collect(),
            };
            check_cheat(&c).map(|c| c.target.places())
        };
        assert_eq!(js(&["game", "zombies", "*", "hp"]), Ok(None));
        assert!(js(&["*", "hp"]).is_err());
        assert!(js(&["game", "*"]).is_err());
        assert!(js(&["a", "*", "*", "*", "hp"]).is_err());
    }

    #[test]
    fn packs_round_trip_and_are_checked() {
        let p = pack();
        let text = serde_json::to_string(&p).unwrap();
        assert!(
            !text.contains("\"key\""),
            "no key fields when there's no key: {text}"
        );
        assert_eq!(parse_pack(text.as_bytes()).unwrap(), p);
        assert!(check_pack(&p, true).is_ok());

        // Newer files with more fields still open.
        let newer = text.replacen("\"name\":\"Gold\"", "\"name\":\"Gold\",\"later\":1", 1);
        assert_eq!(parse_pack(newer.as_bytes()).unwrap(), p);

        let mut local = p.clone();
        local.url = "http://127.0.0.1:8080/".into();
        assert!(check_pack(&local, false).is_ok());
        assert!(check_pack(&local, true).is_err());

        let mut q = p.clone();
        q.format = 2;
        assert!(check_pack(&q, false).unwrap_err().contains("newer"));
        let mut q = p.clone();
        q.cheats = vec![cheat("c"); MAX_SHARED_CHEATS + 1];
        assert!(check_pack(&q, false).is_ok());
        assert!(check_pack(&q, true).is_err());
        let mut q = p.clone();
        q.cheats.clear();
        assert!(check_pack(&q, false).is_err());
        let mut q = p.clone();
        q.by = " ".into();
        assert!(check_pack(&q, false).is_err());
        let mut q = p.clone();
        q.id = "../x".into();
        assert!(check_pack(&q, false).is_err());
        let mut q = p.clone();
        q.added = Some("10/11/2026".into());
        assert!(check_pack(&q, false).is_err());

        assert!(parse_pack(b"{\"schema\":1}").is_err());
        assert!(parse_pack(&vec![b' '; MAX_BYTES + 1]).is_err());
        assert_eq!(file_name(&p), format!("{}.juiceweb", p.id));
    }
}
