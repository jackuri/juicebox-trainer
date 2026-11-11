//! Copying a game's saves from one of your Steam accounts to another on the
//! same PC (PLAN.md §2.3 "account transfer").
//!
//! Saves belong to an account when a folder on their path is named after it:
//! Steam Cloud's `userdata\<account id>\<app id>\remote`, or a game's own
//! folder such as `Documents\My Games\Game\76561198…`. Copying writes the
//! files into the other account's folder and changes the Steam ID inside
//! them (and in file names) to the other account's, so games that check it
//! accept the save:
//! - the 17-digit SteamID64 as text, and as 8 bytes in binary files;
//! - the shorter account id as text, only in text files and only when it has
//!   at least 7 digits (shorter numbers are too common to replace safely).
//!
//! Compressed or checksummed files are unwrapped first and wrapped again
//! afterwards (see [`crate::codec::layers`]). The source saves are never
//! changed. Like a restore: the guard checks the game's folder, the game must
//! be closed, and all its save folders are backed up first.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::codec::layers;
use crate::error::{Error, Result};
use crate::library::Game;
use crate::registry;
use crate::restore::{CLOUD_WARNING, is_running};
use crate::store::{self, Limits, Reason, Store, scan, write_atomic};

/// SteamID64 of account id 0.
const ID64_BASE: u64 = 76_561_197_960_265_728;

/// A Steam account on this PC (or seen in a save folder name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamAccount {
    /// SteamID64, as text (it doesn't fit a JavaScript number exactly).
    pub id64: String,
    pub id32: u32,
    /// The name friends see.
    pub name: Option<String>,
    /// The sign-in name.
    pub login: Option<String>,
    /// Signed in on this PC before (Steam knows it).
    pub on_this_pc: bool,
}

impl SteamAccount {
    fn from_id32(id32: u32) -> Self {
        Self {
            id64: (ID64_BASE + u64::from(id32)).to_string(),
            id32,
            name: None,
            login: None,
            on_this_pc: false,
        }
    }

    fn id64_num(&self) -> u64 {
        ID64_BASE + u64::from(self.id32)
    }

    /// "Name (login)" or the id.
    pub fn label(&self) -> String {
        match (&self.name, &self.login) {
            (Some(n), Some(l)) if n != l => format!("{n} ({l})"),
            (Some(n), _) => n.clone(),
            (None, Some(l)) => l.clone(),
            (None, None) => self.id64.clone(),
        }
    }
}

fn id32_of(id64: u64) -> Option<u32> {
    id64.checked_sub(ID64_BASE)
        .and_then(|d| u32::try_from(d).ok())
        .filter(|&d| d != 0)
}

/// Steam accounts known on this PC: `config\loginusers.vdf` (names) and the
/// numbered folders in `userdata`.
pub fn accounts(steam: &Path) -> Vec<SteamAccount> {
    let mut found: BTreeMap<u32, SteamAccount> = BTreeMap::new();
    if let Ok(text) = std::fs::read_to_string(steam.join("config").join("loginusers.vdf")) {
        let mut current: Option<u32> = None;
        for line in text.lines() {
            let t = line.trim();
            let tokens: Vec<&str> = t.split('"').skip(1).step_by(2).collect();
            if tokens.len() == 1 {
                current = tokens[0].parse::<u64>().ok().and_then(id32_of);
                if let Some(id) = current {
                    let a = found
                        .entry(id)
                        .or_insert_with(|| SteamAccount::from_id32(id));
                    a.on_this_pc = true;
                }
                continue;
            }
            let (Some(id), [key, value]) = (current, tokens.as_slice()) else {
                continue;
            };
            let Some(a) = found.get_mut(&id) else {
                continue;
            };
            let value = (!value.is_empty()).then(|| value.to_string());
            if key.eq_ignore_ascii_case("PersonaName") {
                a.name = value;
            } else if key.eq_ignore_ascii_case("AccountName") {
                a.login = value;
            }
        }
    }
    if let Ok(dirs) = std::fs::read_dir(steam.join("userdata")) {
        for d in dirs.flatten() {
            let name = d.file_name().to_string_lossy().into_owned();
            if let Ok(id) = name.parse::<u32>()
                && id != 0
                && d.path().is_dir()
            {
                found
                    .entry(id)
                    .or_insert_with(|| SteamAccount::from_id32(id))
                    .on_this_pc = true;
            }
        }
    }
    found.into_values().collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Form {
    Id64,
    Id32,
}

/// A folder of one account's saves for a game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountFolder {
    pub path: String,
    /// The game's listed save folder it is in (or is).
    pub listed: String,
    /// Which component of `path` is the account (counted from the start).
    component: usize,
    form: Form,
}

/// An account with saves of this game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSaves {
    pub account: SteamAccount,
    pub folders: Vec<AccountFolder>,
    pub files: usize,
    pub bytes: u64,
}

/// What the "copy to another account" panel shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsView {
    /// Accounts with saves of this game.
    pub with_saves: Vec<AccountSaves>,
    /// Every account saves can be copied to.
    pub accounts: Vec<SteamAccount>,
}

fn account_in(name: &str, known: &[SteamAccount]) -> Option<(SteamAccount, Form)> {
    if !name.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if name.len() == 17 {
        let id64: u64 = name.parse().ok()?;
        let id32 = id32_of(id64)?;
        let a = known
            .iter()
            .find(|a| a.id32 == id32)
            .cloned()
            .unwrap_or_else(|| SteamAccount::from_id32(id32));
        return Some((a, Form::Id64));
    }
    // Short ids only for accounts this PC knows, and not tiny numbers (a
    // folder called "2" is not an account).
    if name.len() < MIN_FOLDER_ID32_DIGITS {
        return None;
    }
    let id32: u32 = name.parse().ok()?;
    known
        .iter()
        .find(|a| a.id32 == id32 && a.id32.to_string() == name)
        .map(|a| (a.clone(), Form::Id32))
}

fn components(p: &Path) -> Vec<String> {
    p.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect()
}

fn with_component(path: &str, index: usize, value: &str) -> String {
    let mut out = PathBuf::new();
    for (i, c) in Path::new(path).components().enumerate() {
        if i == index {
            out.push(value);
        } else {
            out.push(c.as_os_str());
        }
    }
    out.to_string_lossy().into_owned()
}

/// The account folders inside (or at) the game's listed save folders.
fn account_folders(game: &Game, known: &[SteamAccount]) -> Vec<(SteamAccount, AccountFolder)> {
    let mut out = Vec::new();
    for listed in &game.folders {
        if registry::is_registry(&listed.path) {
            continue;
        }
        let base = Path::new(&listed.path);
        let parts = components(base);
        // The listed folder's own path names an account (userdata\<id>\...).
        if let Some((i, (a, form))) = parts
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, c)| account_in(c, known).map(|x| (i, x)))
        {
            out.push((
                a,
                AccountFolder {
                    path: listed.path.clone(),
                    listed: listed.path.clone(),
                    component: i,
                    form,
                },
            ));
            continue;
        }
        // Account folders inside it, one or two levels down.
        let mut dirs = vec![(base.to_path_buf(), 0)];
        while let Some((dir, depth)) = dirs.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.flatten() {
                let Ok(meta) = std::fs::symlink_metadata(e.path()) else {
                    continue;
                };
                if !meta.is_dir() {
                    continue;
                }
                let name = e.file_name().to_string_lossy().into_owned();
                if let Some((a, form)) = account_in(&name, known) {
                    let path = e.path();
                    out.push((
                        a,
                        AccountFolder {
                            component: components(&path).len() - 1,
                            path: path.to_string_lossy().into_owned(),
                            listed: listed.path.clone(),
                            form,
                        },
                    ));
                } else if depth < 1 {
                    dirs.push((e.path(), depth + 1));
                }
            }
        }
    }
    out.sort_by(|a, b| a.1.path.cmp(&b.1.path));
    out
}

/// Which accounts have saves of `game`, and every account to copy to.
pub fn view(game: &Game, known: &[SteamAccount]) -> Result<AccountsView> {
    let mut with: BTreeMap<u32, AccountSaves> = BTreeMap::new();
    for (a, f) in account_folders(game, known) {
        let files = scan(std::slice::from_ref(&f.path), Limits::default())?;
        if files.is_empty() {
            continue;
        }
        let bytes = files
            .iter()
            .map(|s| std::fs::metadata(&s.abs).map_or(0, |m| m.len()))
            .sum::<u64>();
        let entry = with.entry(a.id32).or_insert_with(|| AccountSaves {
            account: a,
            folders: Vec::new(),
            files: 0,
            bytes: 0,
        });
        entry.files += files.len();
        entry.bytes += bytes;
        entry.folders.push(f);
    }
    let mut accounts = known.to_vec();
    for s in with.values() {
        if !accounts.iter().any(|a| a.id32 == s.account.id32) {
            accounts.push(s.account.clone());
        }
    }
    Ok(AccountsView {
        with_saves: with.into_values().collect(),
        accounts,
    })
}

/// Replaces `from` with `to` where it stands alone (not part of a longer
/// number). Returns how many were replaced.
fn replace_number(data: &mut Vec<u8>, from: &[u8], to: &[u8]) -> usize {
    if from.is_empty() || data.len() < from.len() {
        return 0;
    }
    let mut out = Vec::with_capacity(data.len());
    let mut count = 0;
    let mut i = 0;
    while i < data.len() {
        let here = data[i..].starts_with(from)
            && (i == 0 || !data[i - 1].is_ascii_digit())
            && data.get(i + from.len()).is_none_or(|b| !b.is_ascii_digit());
        if here {
            out.extend_from_slice(to);
            i += from.len();
            count += 1;
        } else {
            out.push(data[i]);
            i += 1;
        }
    }
    *data = out;
    count
}

fn replace_bytes(data: &mut [u8], from: &[u8], to: &[u8]) -> usize {
    let mut count = 0;
    let mut i = 0;
    while i + from.len() <= data.len() {
        if &data[i..i + from.len()] == from {
            data[i..i + from.len()].copy_from_slice(to);
            count += 1;
            i += from.len();
        } else {
            i += 1;
        }
    }
    count
}

/// Short account ids are only replaced when they have this many digits.
const MIN_ID32_DIGITS: usize = 7;
/// A folder named with a short account id needs at least this many digits.
const MIN_FOLDER_ID32_DIGITS: usize = 5;

/// The file's bytes with `from`'s Steam ID changed to `to`'s.
pub fn rewrite_ids(
    bytes: &[u8],
    from: &SteamAccount,
    to: &SteamAccount,
) -> Result<(Vec<u8>, usize)> {
    let (wrappers, mut contents) = layers::peel(bytes);
    let mut count = replace_number(
        &mut contents,
        from.id64_num().to_string().as_bytes(),
        to.id64_num().to_string().as_bytes(),
    );
    let text = std::str::from_utf8(&contents).is_ok();
    if text {
        let short = from.id32.to_string();
        if short.len() >= MIN_ID32_DIGITS {
            count += replace_number(
                &mut contents,
                short.as_bytes(),
                to.id32.to_string().as_bytes(),
            );
        }
    } else {
        count += replace_bytes(
            &mut contents,
            &from.id64_num().to_le_bytes(),
            &to.id64_num().to_le_bytes(),
        );
    }
    if count == 0 {
        return Ok((bytes.to_vec(), 0));
    }
    let out = layers::wrap(&wrappers, contents)
        .map_err(|e| Error::Edit(format!("can't write the file back: {e}")))?;
    Ok((out, count))
}

/// A file name with the account's ids changed.
fn rename(rel: &str, from: &SteamAccount, to: &SteamAccount) -> String {
    let mut bytes = rel.as_bytes().to_vec();
    replace_number(
        &mut bytes,
        from.id64_num().to_string().as_bytes(),
        to.id64_num().to_string().as_bytes(),
    );
    let short = from.id32.to_string();
    if short.len() >= MIN_ID32_DIGITS {
        replace_number(&mut bytes, short.as_bytes(), to.id32.to_string().as_bytes());
    }
    String::from_utf8(bytes).unwrap_or_else(|_| rel.to_string())
}

/// One file to copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedCopy {
    pub from: String,
    pub to: String,
    pub size: u64,
    /// The other account already has a file there; it is replaced.
    pub replaces: bool,
    /// How many Steam IDs inside are changed.
    pub ids: usize,
}

/// What copying would do, for the player to check first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyPlan {
    pub from: SteamAccount,
    pub to: SteamAccount,
    pub files: Vec<PlannedCopy>,
    /// Folders that don't exist yet and will be made (and added to the game).
    pub new_folders: Vec<String>,
    pub cloud_warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyReport {
    /// The backup made first (empty if nothing was written).
    pub backup: String,
    pub copied: usize,
    pub replaced: usize,
    pub ids: usize,
    /// Folders to add to the game's save folders (they weren't covered yet).
    pub add_folders: Vec<String>,
    pub cloud_warning: Option<String>,
}

fn find(known: &[SteamAccount], id64: &str, what: &str) -> Result<SteamAccount> {
    known
        .iter()
        .find(|a| a.id64 == id64.trim())
        .cloned()
        .ok_or_else(|| Error::Edit(format!("unknown Steam account for {what}: {id64}")))
}

struct Work {
    plan: CopyPlan,
    writes: Vec<(PathBuf, Vec<u8>)>,
    /// The other account's folders, one per source folder.
    targets: Vec<String>,
}

fn covered(game: &Game, folder: &str) -> bool {
    let f = folder.to_lowercase();
    game.folders.iter().any(|l| {
        let l = l.path.to_lowercase();
        f == l || f.starts_with(&format!("{l}\\"))
    })
}

fn work(game: &Game, view: &AccountsView, from: &str, to: &str) -> Result<Work> {
    let source = view
        .with_saves
        .iter()
        .find(|s| s.account.id64 == from.trim())
        .ok_or_else(|| Error::Edit("that account has no saves of this game".into()))?;
    let from = source.account.clone();
    let to = find(&view.accounts, to, "the copy")?;
    if from.id32 == to.id32 {
        return Err(Error::Edit("pick a different account to copy to".into()));
    }
    let mut files = Vec::new();
    let mut writes = Vec::new();
    let mut new_folders = Vec::new();
    let mut targets = Vec::new();
    for f in &source.folders {
        let id = match f.form {
            Form::Id64 => to.id64.clone(),
            Form::Id32 => to.id32.to_string(),
        };
        let target = with_component(&f.path, f.component, &id);
        if !Path::new(&target).is_dir() && !new_folders.contains(&target) {
            new_folders.push(target.clone());
        }
        if !targets.contains(&target) {
            targets.push(target.clone());
        }
        for s in scan(std::slice::from_ref(&f.path), Limits::default())? {
            let bytes = s.read().map_err(|e| Error::io("can't read", &s.abs, e))?;
            let (out, ids) = rewrite_ids(&bytes, &from, &to)?;
            let rel = rename(&s.rel, &from, &to);
            let rel_path = store::safe_rel(&rel)
                .filter(|p| p.components().all(|c| matches!(c, Component::Normal(_))))
                .ok_or_else(|| Error::Edit(format!("bad file name {rel:?}")))?;
            let dest = Path::new(&target).join(rel_path);
            files.push(PlannedCopy {
                from: s.abs.to_string_lossy().into_owned(),
                to: dest.to_string_lossy().into_owned(),
                size: bytes.len() as u64,
                replaces: dest.is_file(),
                ids,
            });
            writes.push((dest, out));
        }
    }
    if files.is_empty() {
        return Err(Error::Edit("that account has no saves of this game".into()));
    }
    Ok(Work {
        plan: CopyPlan {
            from,
            to,
            files,
            new_folders,
            cloud_warning: game.cloud_synced().then(|| CLOUD_WARNING.to_string()),
        },
        writes,
        targets,
    })
}

/// What copying `from`'s saves to `to` would do. Changes nothing.
pub fn plan(game: &Game, known: &[SteamAccount], from: &str, to: &str) -> Result<CopyPlan> {
    let v = view(game, known)?;
    Ok(work(game, &v, from, to)?.plan)
}

/// Copies the saves, after the guard and running checks and a backup of all
/// the game's save folders.
pub fn copy(
    game: &Game,
    store: &Store,
    known: &[SteamAccount],
    from: &str,
    to: &str,
) -> Result<CopyReport> {
    let verdict = jb_core::guard::check_install(Path::new(&game.exe));
    if !verdict.allowed {
        return Err(Error::Blocked(Box::new(verdict)));
    }
    if is_running(&game.exe)? {
        return Err(Error::GameRunning(game.name.clone()));
    }
    copy_unchecked(game, store, known, from, to)
}

/// [`copy`] without the guard and running checks; for tests.
pub(crate) fn copy_unchecked(
    game: &Game,
    store: &Store,
    known: &[SteamAccount],
    from: &str,
    to: &str,
) -> Result<CopyReport> {
    let _lock = store::lock();
    let v = view(game, known)?;
    let Work {
        plan,
        writes,
        targets,
    } = work(game, &v, from, to)?;
    let backup = store.create_locked(
        &game.folder_paths(),
        Reason::BeforeAccountCopy,
        &format!("before copying saves to {}", plan.to.label()),
        Limits::default(),
    )?;
    let mut copied = 0;
    let mut replaced = 0;
    for ((dest, bytes), p) in writes.iter().zip(&plan.files) {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::io("can't create folder", parent, e))?;
        }
        if std::fs::read(dest).is_ok_and(|old| &old == bytes) {
            continue;
        }
        write_atomic(dest, bytes)?;
        copied += 1;
        if p.replaces {
            replaced += 1;
        }
    }
    let add_folders = targets.into_iter().filter(|f| !covered(game, f)).collect();
    tracing::info!(game = %game.name, to = %plan.to.id64, copied, "saves copied to another account");
    Ok(CopyReport {
        backup: backup.snapshot.id,
        copied,
        replaced,
        ids: plan.files.iter().map(|f| f.ids).sum(),
        add_folders,
        cloud_warning: plan.cloud_warning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::layers::{Algo, At, Layer};
    use crate::locate::{CloudInfo, SaveFolder};

    const A: u32 = 11_223_344;
    const B: u32 = 55_667_788;

    fn id64(id32: u32) -> String {
        (ID64_BASE + u64::from(id32)).to_string()
    }

    fn steam(base: &Path) -> PathBuf {
        let steam = base.join("Steam");
        std::fs::create_dir_all(steam.join("config")).unwrap();
        std::fs::write(
            steam.join("config").join("loginusers.vdf"),
            format!(
                "\"users\"\n{{\n\t\"{}\"\n\t{{\n\t\t\"AccountName\"\t\t\"main_login\"\n\t\t\"PersonaName\"\t\t\"Main\"\n\t\t\"MostRecent\"\t\t\"1\"\n\t}}\n\t\"{}\"\n\t{{\n\t\t\"AccountName\"\t\t\"alt_login\"\n\t\t\"PersonaName\"\t\t\"Alt\"\n\t}}\n}}\n",
                id64(A),
                id64(B)
            ),
        )
        .unwrap();
        std::fs::create_dir_all(steam.join("userdata").join(A.to_string())).unwrap();
        std::fs::create_dir_all(steam.join("userdata").join("99")).unwrap();
        steam
    }

    fn game(folders: &[&Path]) -> Game {
        Game {
            id: "g".into(),
            name: "Game".into(),
            exe: r"C:\nowhere\game.exe".into(),
            steam: None,
            cloud: CloudInfo::default(),
            folders: folders
                .iter()
                .map(|p| SaveFolder {
                    path: p.to_string_lossy().into_owned(),
                    reason: String::new(),
                    user_added: false,
                    steam_cloud: false,
                })
                .collect(),
            added_ms: 0,
        }
    }

    #[test]
    fn lists_accounts_with_names() {
        let base = crate::test_dir("accounts-list");
        let list = accounts(&steam(&base));
        let ids: Vec<_> = list.iter().map(|a| a.id32).collect();
        assert_eq!(ids, vec![99, A, B]);
        let main = list.iter().find(|a| a.id32 == A).unwrap();
        assert_eq!(main.label(), "Main (main_login)");
        assert_eq!(main.id64, id64(A));
        assert!(main.on_this_pc);
        assert_eq!(list[0].label(), id64(99));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn rewrites_ids_in_text_binary_and_wrapped_files() {
        let a = SteamAccount::from_id32(A);
        let b = SteamAccount::from_id32(B);
        let text = format!("{{\"owner\": \"{}\", \"acc\": {A}, \"x\": 1{A}}}", id64(A));
        let (out, n) = rewrite_ids(text.as_bytes(), &a, &b).unwrap();
        assert_eq!(n, 2, "the longer number is left alone");
        assert_eq!(
            String::from_utf8(out).unwrap(),
            format!("{{\"owner\": \"{}\", \"acc\": {B}, \"x\": 1{A}}}", id64(B))
        );

        let mut bin = vec![0xFFu8, 0, 1];
        bin.extend_from_slice(&(ID64_BASE + u64::from(A)).to_le_bytes());
        bin.extend_from_slice(&A.to_le_bytes());
        let (out, n) = rewrite_ids(&bin, &a, &b).unwrap();
        assert_eq!(n, 1);
        assert_eq!(&out[3..11], &(ID64_BASE + u64::from(B)).to_le_bytes());
        assert_eq!(
            &out[11..15],
            &A.to_le_bytes(),
            "short binary ids aren't touched"
        );

        let wrapped = layers::wrap(
            &[
                Layer::Checksum {
                    algo: Algo::Crc32Le,
                    at: At::Trailer,
                },
                Layer::Gzip,
            ],
            text.clone().into_bytes(),
        )
        .unwrap();
        let (out, n) = rewrite_ids(&wrapped, &a, &b).unwrap();
        assert_eq!(n, 2);
        let (found, inner) = layers::peel(&out);
        assert_eq!(found.len(), 2, "wrapped again, checksum recomputed");
        assert!(String::from_utf8(inner).unwrap().contains(&id64(B)));

        let (out, n) = rewrite_ids(b"nothing here", &a, &b).unwrap();
        assert_eq!((out.as_slice(), n), (&b"nothing here"[..], 0));
    }

    #[test]
    fn copies_steam_cloud_and_game_folders() {
        let base = crate::test_dir("accounts-copy");
        let steam = steam(&base);
        let known = accounts(&steam);
        // Steam Cloud folder of account A.
        let remote_a = steam
            .join("userdata")
            .join(A.to_string())
            .join("4242")
            .join("remote");
        std::fs::create_dir_all(remote_a.join("slots")).unwrap();
        std::fs::write(
            remote_a.join("slots").join("slot1.json"),
            format!("{{\"steamId\": \"{}\", \"gold\": 5}}", id64(A)),
        )
        .unwrap();
        // A game folder with one subfolder per account.
        let docs = base.join("Documents").join("My Games").join("Game");
        let docs_a = docs.join(id64(A));
        let docs_b = docs.join(id64(B));
        std::fs::create_dir_all(&docs_a).unwrap();
        std::fs::create_dir_all(&docs_b).unwrap();
        std::fs::write(docs_a.join(format!("profile_{}.dat", id64(A))), b"\x01\x02").unwrap();
        std::fs::write(docs_a.join("same.sav"), b"plain").unwrap();
        std::fs::write(docs_b.join("same.sav"), b"old").unwrap();
        let g = game(&[&remote_a, &docs]);
        let store = Store::new(base.join("store"));

        let v = view(&g, &known).unwrap();
        assert_eq!(v.with_saves.len(), 2, "B has one save already");
        assert_eq!(v.with_saves[1].account.id32, B);
        assert_eq!(v.with_saves[1].files, 1);
        assert_eq!(v.with_saves[0].account.id32, A);
        assert_eq!(v.with_saves[0].files, 3);
        assert_eq!(v.with_saves[0].folders.len(), 2);

        let p = plan(&g, &known, &id64(A), &id64(B)).unwrap();
        assert_eq!(p.files.len(), 3);
        let remote_b = steam
            .join("userdata")
            .join(B.to_string())
            .join("4242")
            .join("remote");
        assert_eq!(p.new_folders, vec![remote_b.to_string_lossy().into_owned()]);
        assert!(
            p.files
                .iter()
                .any(|f| f.replaces && f.to.ends_with("same.sav"))
        );
        assert!(
            p.files
                .iter()
                .any(|f| f.to.ends_with(&format!("profile_{}.dat", id64(B))))
        );
        assert!(!remote_b.exists(), "planning changes nothing");

        let r = copy_unchecked(&g, &store, &known, &id64(A), &id64(B)).unwrap();
        assert_eq!((r.copied, r.replaced, r.ids), (3, 1, 1));
        assert_eq!(r.add_folders, vec![remote_b.to_string_lossy().into_owned()]);
        assert_eq!(
            store.manifest(&r.backup).unwrap().reason,
            Reason::BeforeAccountCopy
        );
        let copied = std::fs::read_to_string(remote_b.join("slots").join("slot1.json")).unwrap();
        assert!(copied.contains(&id64(B)) && copied.contains("\"gold\": 5"));
        assert_eq!(std::fs::read(docs_b.join("same.sav")).unwrap(), b"plain");
        assert!(docs_b.join(format!("profile_{}.dat", id64(B))).is_file());
        // The source is untouched.
        assert!(
            std::fs::read_to_string(remote_a.join("slots").join("slot1.json"))
                .unwrap()
                .contains(&id64(A))
        );
        // The old file of B is in the backup.
        let m = store.manifest(&r.backup).unwrap();
        assert!(
            m.files
                .iter()
                .any(|f| f.path.ends_with("same.sav")
                    && store.read_blob(&f.sha256).unwrap() == b"old")
        );

        assert!(plan(&g, &known, &id64(A), &id64(A)).is_err());
        assert!(
            plan(&g, &known, &id64(99), &id64(A)).is_err(),
            "account 99 has no saves"
        );
        assert!(plan(&g, &known, &id64(A), "123").is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn copying_needs_the_guard_and_a_closed_game() {
        let base = crate::test_dir("accounts-guard");
        let steam = steam(&base);
        let known = accounts(&steam);
        let docs = base.join("Saves").join(id64(A));
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(docs.join("a.sav"), b"x").unwrap();
        let mut g = game(&[&docs]);
        g.exe = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let store = Store::new(base.join("store"));
        let r = copy(&g, &store, &known, &id64(A), &id64(B));
        assert!(
            matches!(r, Err(Error::GameRunning(_)) | Err(Error::Blocked(_))),
            "{r:?}"
        );
        assert!(!base.join("Saves").join(id64(B)).exists());
        std::fs::remove_dir_all(&base).unwrap();
    }
}
