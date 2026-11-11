//! Compares two backups, or a backup with the saves as they are now.
//!
//! Text files get a line diff (long lines, like one-line JSON, are cut down to
//! the part that changed). Other files get the byte ranges that changed, with
//! the 32-bit number at that spot when the change fits in one, which is often
//! enough to spot gold or health.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use jb_core::hash::sha256_bytes;
use serde::Serialize;
use similar::{ChangeTag, TextDiff};

use crate::error::{Error, Result};
use crate::store::{Limits, Store, key, scan};

/// Files bigger than this are compared by hash only.
pub const MAX_DETAIL_BYTES: u64 = 4 << 20;
const MAX_HUNKS: usize = 50;
const MAX_RANGES: usize = 100;
const MAX_LINE: usize = 160;
const CONTEXT_CHARS: usize = 40;
const MAX_RANGE_BYTES: usize = 16;

/// One side of a comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side<'a> {
    Snapshot(&'a str),
    /// The save folders as they are now.
    Current,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineTag {
    Same,
    Removed,
    Added,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub tag: LineTag,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextHunk {
    /// 1-based first line on each side.
    pub old_line: usize,
    pub new_line: usize,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ByteRange {
    pub offset: u64,
    pub len: u64,
    /// Hex, cut to 16 bytes.
    pub old: String,
    pub new: String,
    /// The aligned little-endian 32-bit number around the change, if the
    /// change fits in one.
    pub old_i32: Option<i32>,
    pub new_i32: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Detail {
    Text {
        hunks: Vec<TextHunk>,
        more: usize,
    },
    Binary {
        ranges: Vec<ByteRange>,
        more: usize,
    },
    /// Too big to show.
    TooLarge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub folder: String,
    pub path: String,
    pub change: ChangeKind,
    pub old_size: Option<u64>,
    pub new_size: Option<u64>,
    pub detail: Option<Detail>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    pub files: Vec<FileChange>,
    pub unchanged: usize,
}

enum Source {
    Blob,
    File(PathBuf),
    Bytes(Vec<u8>),
}

struct Item {
    folder: String,
    path: String,
    size: u64,
    sha: String,
    source: Source,
}

fn side_items(store: &Store, folders: &[String], side: Side<'_>) -> Result<BTreeMap<String, Item>> {
    let mut out = BTreeMap::new();
    match side {
        Side::Snapshot(id) => {
            let m = store.manifest(id)?;
            for f in m.files {
                let folder = m.folders[f.folder].clone();
                out.insert(
                    key(&folder, &f.path),
                    Item {
                        folder,
                        path: f.path,
                        size: f.size,
                        sha: f.sha256,
                        source: Source::Blob,
                    },
                );
            }
        }
        Side::Current => {
            for s in scan(folders, Limits::default())? {
                let Ok(bytes) = s.read() else {
                    continue;
                };
                let folder = folders[s.folder].clone();
                out.insert(
                    key(&folder, &s.rel),
                    Item {
                        folder,
                        path: s.rel,
                        size: bytes.len() as u64,
                        sha: sha256_bytes(&bytes),
                        source: match s.data {
                            Some(d) => Source::Bytes(d),
                            None => Source::File(s.abs),
                        },
                    },
                );
            }
        }
    }
    Ok(out)
}

fn bytes_of(store: &Store, item: &Item) -> Result<Vec<u8>> {
    match &item.source {
        Source::Blob => store.read_blob(&item.sha),
        Source::File(p) => std::fs::read(p).map_err(|e| Error::io("can't read", p, e)),
        Source::Bytes(b) => Ok(b.clone()),
    }
}

/// Compares `from` with `to`. `folders` are the game's save folders, used for
/// [`Side::Current`].
pub fn diff(store: &Store, folders: &[String], from: Side<'_>, to: Side<'_>) -> Result<Diff> {
    let old = side_items(store, folders, from)?;
    let new = side_items(store, folders, to)?;
    let mut files = Vec::new();
    let mut unchanged = 0;
    for (k, o) in &old {
        match new.get(k) {
            None => files.push(FileChange {
                folder: o.folder.clone(),
                path: o.path.clone(),
                change: ChangeKind::Removed,
                old_size: Some(o.size),
                new_size: None,
                detail: None,
            }),
            Some(n) if n.sha == o.sha => unchanged += 1,
            Some(n) => {
                let detail = if o.size > MAX_DETAIL_BYTES || n.size > MAX_DETAIL_BYTES {
                    Detail::TooLarge
                } else {
                    compare(&bytes_of(store, o)?, &bytes_of(store, n)?)
                };
                files.push(FileChange {
                    folder: n.folder.clone(),
                    path: n.path.clone(),
                    change: ChangeKind::Changed,
                    old_size: Some(o.size),
                    new_size: Some(n.size),
                    detail: Some(detail),
                });
            }
        }
    }
    for (k, n) in &new {
        if !old.contains_key(k) {
            files.push(FileChange {
                folder: n.folder.clone(),
                path: n.path.clone(),
                change: ChangeKind::Added,
                old_size: None,
                new_size: Some(n.size),
                detail: None,
            });
        }
    }
    files.sort_by(|a, b| (&a.folder, &a.path).cmp(&(&b.folder, &b.path)));
    Ok(Diff { files, unchanged })
}

fn as_text(bytes: &[u8]) -> Option<&str> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    if bytes.contains(&0) {
        return None;
    }
    std::str::from_utf8(bytes).ok()
}

/// Compares two versions of one file.
pub fn compare(old: &[u8], new: &[u8]) -> Detail {
    match (as_text(old), as_text(new)) {
        (Some(a), Some(b)) => text_diff(a, b),
        _ => byte_diff(old, new),
    }
}

fn clip(line: &str) -> String {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.chars().count() <= MAX_LINE {
        return line.to_string();
    }
    let mut s: String = line.chars().take(MAX_LINE).collect();
    s.push('…');
    s
}

/// Cuts a changed pair of long lines down to the part that differs.
fn excerpt_pair(old: &str, new: &str) -> (String, String) {
    let old = old.trim_end_matches(['\r', '\n']);
    let new = new.trim_end_matches(['\r', '\n']);
    let a: Vec<char> = old.chars().collect();
    let b: Vec<char> = new.chars().collect();
    if a.len() <= MAX_LINE && b.len() <= MAX_LINE {
        return (old.to_string(), new.to_string());
    }
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let start = prefix.saturating_sub(CONTEXT_CHARS);
    let cut = |v: &[char]| {
        let end = (v.len() - suffix + CONTEXT_CHARS).min(v.len());
        let mut s = String::new();
        if start > 0 {
            s.push('…');
        }
        let body: String = v[start..end].iter().collect();
        s.push_str(&clip(&body));
        if end < v.len() && !s.ends_with('…') {
            s.push('…');
        }
        s
    };
    (cut(&a), cut(&b))
}

fn text_diff(old: &str, new: &str) -> Detail {
    let diff = TextDiff::configure()
        .timeout(Duration::from_secs(2))
        .diff_lines(old, new);
    let groups = diff.grouped_ops(2);
    let more = groups.len().saturating_sub(MAX_HUNKS);
    let mut hunks = Vec::new();
    for group in groups.iter().take(MAX_HUNKS) {
        let Some(first) = group.first() else { continue };
        let mut raw: Vec<(LineTag, &str)> = Vec::new();
        for op in group {
            for change in diff.iter_changes(op) {
                let tag = match change.tag() {
                    ChangeTag::Equal => LineTag::Same,
                    ChangeTag::Delete => LineTag::Removed,
                    ChangeTag::Insert => LineTag::Added,
                };
                raw.push((tag, change.value()));
            }
        }
        hunks.push(TextHunk {
            old_line: first.old_range().start + 1,
            new_line: first.new_range().start + 1,
            lines: pair_lines(&raw),
        });
    }
    Detail::Text { hunks, more }
}

/// Turns raw changes into display lines, cutting removed/added pairs of long
/// lines down to the part that changed.
fn pair_lines(raw: &[(LineTag, &str)]) -> Vec<DiffLine> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        if raw[i].0 != LineTag::Removed {
            out.push(DiffLine {
                tag: raw[i].0,
                text: clip(raw[i].1),
            });
            i += 1;
            continue;
        }
        let removed: Vec<&str> = raw[i..]
            .iter()
            .take_while(|(t, _)| *t == LineTag::Removed)
            .map(|(_, s)| *s)
            .collect();
        let j = i + removed.len();
        let added: Vec<&str> = raw[j..]
            .iter()
            .take_while(|(t, _)| *t == LineTag::Added)
            .map(|(_, s)| *s)
            .collect();
        let mut old_lines = Vec::new();
        let mut new_lines = Vec::new();
        for k in 0..removed.len().max(added.len()) {
            match (removed.get(k), added.get(k)) {
                (Some(o), Some(n)) => {
                    let (o, n) = excerpt_pair(o, n);
                    old_lines.push(o);
                    new_lines.push(n);
                }
                (Some(o), None) => old_lines.push(clip(o)),
                (None, Some(n)) => new_lines.push(clip(n)),
                (None, None) => {}
            }
        }
        out.extend(old_lines.into_iter().map(|text| DiffLine {
            tag: LineTag::Removed,
            text,
        }));
        out.extend(new_lines.into_iter().map(|text| DiffLine {
            tag: LineTag::Added,
            text,
        }));
        i = j + added.len();
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    let mut s: Vec<String> = bytes
        .iter()
        .take(MAX_RANGE_BYTES)
        .map(|b| format!("{b:02X}"))
        .collect();
    if bytes.len() > MAX_RANGE_BYTES {
        s.push("…".into());
    }
    s.join(" ")
}

fn word(bytes: &[u8], at: usize) -> Option<i32> {
    let w = bytes.get(at..at + 4)?;
    Some(i32::from_le_bytes([w[0], w[1], w[2], w[3]]))
}

fn byte_diff(old: &[u8], new: &[u8]) -> Detail {
    let common = old.len().min(new.len());
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < common {
        if old[i] == new[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < common && old[i] != new[i] {
            i += 1;
        }
        match spans.last_mut() {
            // Join changes a few bytes apart.
            Some(last) if start - last.1 <= 4 => last.1 = i,
            _ => spans.push((start, i)),
        }
    }
    let more = spans.len().saturating_sub(MAX_RANGES);
    let ranges = spans
        .into_iter()
        .take(MAX_RANGES)
        .map(|(s, e)| {
            let aligned = s / 4 * 4;
            let fits = (e - 1) / 4 * 4 == aligned;
            ByteRange {
                offset: s as u64,
                len: (e - s) as u64,
                old: hex(&old[s..e]),
                new: hex(&new[s..e]),
                old_i32: fits.then(|| word(old, aligned)).flatten(),
                new_i32: fits.then(|| word(new, aligned)).flatten(),
            }
        })
        .collect();
    Detail::Binary { ranges, more }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Reason;

    #[test]
    fn text_changes_show_the_changed_lines() {
        let old = "{\n  \"gold\": 100,\n  \"health\": 50\n}\n";
        let new = "{\n  \"gold\": 1100,\n  \"health\": 50\n}\n";
        let Detail::Text { hunks, more } = compare(old.as_bytes(), new.as_bytes()) else {
            panic!("expected text");
        };
        assert_eq!(more, 0);
        assert_eq!(hunks.len(), 1);
        let changed: Vec<_> = hunks[0]
            .lines
            .iter()
            .filter(|l| l.tag != LineTag::Same)
            .map(|l| (l.tag, l.text.as_str()))
            .collect();
        assert_eq!(
            changed,
            vec![
                (LineTag::Removed, "  \"gold\": 100,"),
                (LineTag::Added, "  \"gold\": 1100,")
            ]
        );
    }

    #[test]
    fn long_lines_are_cut_to_the_change() {
        let pad = "x".repeat(500);
        let old = format!("{{\"a\":\"{pad}\",\"gold\":100,\"b\":\"{pad}\"}}");
        let new = format!("{{\"a\":\"{pad}\",\"gold\":1100,\"b\":\"{pad}\"}}");
        let Detail::Text { hunks, .. } = compare(old.as_bytes(), new.as_bytes()) else {
            panic!("expected text");
        };
        let lines = &hunks[0].lines;
        assert_eq!(lines.len(), 2);
        assert!(lines[0].text.contains("\"gold\":100,"), "{}", lines[0].text);
        assert!(
            lines[1].text.contains("\"gold\":1100,"),
            "{}",
            lines[1].text
        );
        assert!(lines[0].text.starts_with('…') && lines[0].text.ends_with('…'));
        assert!(lines[1].text.chars().count() < 200);
    }

    #[test]
    fn binary_changes_show_ranges_and_numbers() {
        let mut old = vec![0u8; 64];
        old[8..12].copy_from_slice(&100i32.to_le_bytes());
        let mut new = old.clone();
        new[8..12].copy_from_slice(&1100i32.to_le_bytes());
        new[40] = 0xFF;
        let Detail::Binary { ranges, more } = compare(&old, &new) else {
            panic!("expected binary");
        };
        assert_eq!(more, 0);
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].offset, 8);
        assert_eq!(ranges[0].old_i32, Some(100));
        assert_eq!(ranges[0].new_i32, Some(1100));
        assert_eq!(ranges[1].offset, 40);
        assert_eq!(ranges[1].old, "00");
        assert_eq!(ranges[1].new, "FF");
    }

    #[test]
    fn compares_backups_with_each_other_and_with_now() {
        let base = crate::test_dir("diff");
        let saves = base.join("saves");
        std::fs::create_dir_all(&saves).unwrap();
        std::fs::write(saves.join("keep.txt"), "same").unwrap();
        std::fs::write(saves.join("gone.txt"), "bye").unwrap();
        std::fs::write(saves.join("save.json"), "{\"gold\":1}").unwrap();
        let folders = vec![saves.to_string_lossy().into_owned()];
        let store = Store::new(base.join("store"));
        let a = store
            .create(&folders, Reason::Manual, "", Limits::default())
            .unwrap()
            .snapshot;
        std::fs::remove_file(saves.join("gone.txt")).unwrap();
        std::fs::write(saves.join("new.txt"), "hi").unwrap();
        std::fs::write(saves.join("save.json"), "{\"gold\":2}").unwrap();

        let now = diff(&store, &folders, Side::Snapshot(&a.id), Side::Current).unwrap();
        let kinds: Vec<_> = now
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.change))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("gone.txt", ChangeKind::Removed),
                ("new.txt", ChangeKind::Added),
                ("save.json", ChangeKind::Changed)
            ]
        );
        assert_eq!(now.unchanged, 1);

        let b = store
            .create(&folders, Reason::Manual, "", Limits::default())
            .unwrap()
            .snapshot;
        let between = diff(
            &store,
            &folders,
            Side::Snapshot(&a.id),
            Side::Snapshot(&b.id),
        )
        .unwrap();
        assert_eq!(between.files, now.files);
        let none = diff(&store, &folders, Side::Snapshot(&b.id), Side::Current).unwrap();
        assert!(none.files.is_empty());
        std::fs::remove_dir_all(&base).unwrap();
    }
}
