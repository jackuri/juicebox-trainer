//! Cover pictures for the games library, taken only from files already on this
//! PC (nothing is downloaded):
//!
//! - Steam: the artwork the Steam client caches for its own library, in
//!   `<Steam>\appcache\librarycache` (the wide `header.jpg` or
//!   `library_header.jpg`, else the hero banner, else the tall capsule).
//! - GOG: the `goggame-<id>.ico` GOG puts in the game folder.
//! - Xbox: the logo named in the game's `appxmanifest.xml`.
//!
//! The app shows these pictures; they are never copied or saved anywhere.

use std::path::{Path, PathBuf};

use crate::installs::{Install, StoreKind};

/// Biggest cover file that is read (a Steam hero banner is about 1–3 MB).
pub const MAX_COVER_BYTES: u64 = 8 << 20;

/// How a cover should be drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CoverKind {
    /// Wide artwork that fills the card.
    Art,
    /// A square icon or logo, drawn in the middle of the card.
    Icon,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cover {
    pub path: PathBuf,
    pub kind: CoverKind,
}

/// The cover file for a game, if one is on this PC. `steam` is the Steam
/// folder (for Steam games).
pub fn find(install: &Install, steam: Option<&Path>) -> Option<Cover> {
    let (store_id, folder) = (install.id.split_once(':')?.1, Path::new(&install.folder));
    match install.store {
        StoreKind::Steam => {
            let app: u32 = store_id.parse().ok()?;
            steam_art(steam?, app).map(|(path, kind)| Cover { path, kind })
        }
        StoreKind::Gog => icon(folder.join(format!("goggame-{store_id}.ico"))),
        StoreKind::Xbox => xbox_logo(folder).and_then(icon),
        StoreKind::Epic | StoreKind::Own => None,
    }
}

fn icon(path: PathBuf) -> Option<Cover> {
    is_small_file(&path).then_some(Cover {
        path,
        kind: CoverKind::Icon,
    })
}

fn is_small_file(p: &Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() > 0 && m.len() <= MAX_COVER_BYTES)
}

/// Steam's cached library artwork for `app`. Newer Steam clients keep it in
/// `librarycache\<app>\` (sometimes one folder deeper), older ones as
/// `librarycache\<app>_header.jpg`.
fn steam_art(steam: &Path, app: u32) -> Option<(PathBuf, CoverKind)> {
    let cache = steam.join("appcache").join("librarycache");
    let dir = cache.join(app.to_string());
    let mut places = vec![dir.clone()];
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let mut subs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        subs.sort();
        places.extend(subs);
    }
    for name in [
        "header.jpg",
        "library_header.jpg",
        "library_hero.jpg",
        "library_600x900.jpg",
    ] {
        for place in &places {
            let p = place.join(name);
            if is_small_file(&p) {
                return Some((p, CoverKind::Art));
            }
        }
        let old = cache.join(format!("{app}_{name}"));
        if is_small_file(&old) {
            return Some((old, CoverKind::Art));
        }
    }
    None
}

/// The logo an Xbox (MSIX) game names in `appxmanifest.xml`, e.g.
/// `<Logo>Assets\StoreLogo.png</Logo>`. Windows may store it only in scaled
/// copies (`StoreLogo.scale-200.png`); the biggest one is used.
fn xbox_logo(folder: &Path) -> Option<PathBuf> {
    let manifest = std::fs::read_to_string(folder.join("appxmanifest.xml")).ok()?;
    let names = [
        tag_text(&manifest, "Logo"),
        attr(&manifest, "Square150x150Logo"),
        attr(&manifest, "Square44x44Logo"),
    ];
    names.into_iter().flatten().find_map(|rel| {
        let rel = rel.replace('/', "\\");
        // Only a picture inside the game's own folder: plain folder and file
        // names, no `..`, no drive (`D:x`), no root (`\Users\…`).
        if !inside(Path::new(&rel)) {
            return None;
        }
        let p = folder.join(&rel);
        if is_small_file(&p) {
            return Some(p);
        }
        scaled(&p)
    })
}

/// A relative path made only of normal names (so joining it can't leave the folder).
fn inside(rel: &Path) -> bool {
    use std::path::Component;
    rel.components().next().is_some()
        && rel
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// `Logo.scale-400.png` … for `Logo.png`: the biggest scale that exists.
fn scaled(p: &Path) -> Option<PathBuf> {
    let (dir, stem, ext) = (
        p.parent()?,
        p.file_stem()?.to_str()?,
        p.extension()?.to_str()?,
    );
    ["400", "200", "150", "125", "100"]
        .iter()
        .map(|s| dir.join(format!("{stem}.scale-{s}.{ext}")))
        .find(|c| is_small_file(c))
}

/// Text of the first `<name>…</name>` element.
fn tag_text(xml: &str, name: &str) -> Option<String> {
    let start = xml.find(&format!("<{name}>"))? + name.len() + 2;
    let end = start + xml[start..].find(&format!("</{name}>"))?;
    let t = xml[start..end].trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// Value of the first `name="…"` attribute.
fn attr(xml: &str, name: &str) -> Option<String> {
    let start = xml.find(&format!("{name}=\""))? + name.len() + 2;
    let end = start + xml[start..].find('"')?;
    let t = xml[start..end].trim();
    (!t.is_empty()).then(|| t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installs::Launch;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ot-covers-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn install(id: &str, store: StoreKind, folder: &Path) -> Install {
        Install {
            id: id.into(),
            name: "Game".into(),
            store,
            folder: folder.display().to_string(),
            exe: None,
            launch: Launch::Manual,
        }
    }

    fn put(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"\xFF\xD8\xFFpicture").unwrap();
    }

    #[test]
    fn steam_prefers_the_wide_header_in_any_layout() {
        let steam = temp("steam");
        let cache = steam.join(r"appcache\librarycache");
        let game = install("steam:620", StoreKind::Steam, &steam);
        assert_eq!(find(&game, Some(&steam)), None);

        // Old layout.
        put(&cache.join("620_library_600x900.jpg"));
        assert!(
            find(&game, Some(&steam))
                .unwrap()
                .path
                .ends_with("620_library_600x900.jpg")
        );
        // New layout: a hero banner beats the tall capsule…
        put(&cache.join(r"620\library_hero.jpg"));
        assert!(
            find(&game, Some(&steam))
                .unwrap()
                .path
                .ends_with(r"620\library_hero.jpg")
        );
        // …the newer header name beats the banner…
        put(&cache.join(r"620\1b061b11\library_header.jpg"));
        assert!(
            find(&game, Some(&steam))
                .unwrap()
                .path
                .ends_with(r"620\1b061b11\library_header.jpg")
        );
        // …and the header, even one folder deeper, beats them all.
        put(&cache.join(r"620\48a2fcbd\header.jpg"));
        let c = find(&game, Some(&steam)).unwrap();
        assert!(c.path.ends_with(r"620\48a2fcbd\header.jpg"));
        assert_eq!(c.kind, CoverKind::Art);

        // Empty files and other games don't count; no Steam folder, no cover.
        let other = install("steam:730", StoreKind::Steam, &steam);
        std::fs::create_dir_all(cache.join("730")).unwrap();
        std::fs::write(cache.join(r"730\header.jpg"), b"").unwrap();
        assert_eq!(find(&other, Some(&steam)), None);
        assert_eq!(find(&game, None), None);
        std::fs::remove_dir_all(&steam).unwrap();
    }

    #[test]
    fn gog_uses_its_icon() {
        let dir = temp("gog");
        let game = install("gog:1778420505", StoreKind::Gog, &dir);
        assert_eq!(find(&game, None), None);
        put(&dir.join("goggame-1778420505.ico"));
        assert_eq!(
            find(&game, None),
            Some(Cover {
                path: dir.join("goggame-1778420505.ico"),
                kind: CoverKind::Icon
            })
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn xbox_uses_the_manifest_logo_and_its_scaled_copies() {
        let dir = temp("xbox");
        let game = install("xbox:Game", StoreKind::Xbox, &dir);
        std::fs::write(
            dir.join("appxmanifest.xml"),
            r#"<Package><Properties><Logo>Assets\StoreLogo.png</Logo></Properties>
               <uap:VisualElements Square150x150Logo="Assets\Square150.png"/></Package>"#,
        )
        .unwrap();
        assert_eq!(find(&game, None), None);
        put(&dir.join(r"Assets\Square150.png"));
        assert!(
            find(&game, None)
                .unwrap()
                .path
                .ends_with(r"Assets\Square150.png")
        );
        put(&dir.join(r"Assets\StoreLogo.scale-200.png"));
        let c = find(&game, None).unwrap();
        assert!(c.path.ends_with(r"Assets\StoreLogo.scale-200.png"));
        assert_eq!(c.kind, CoverKind::Icon);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn xbox_logo_must_stay_inside_the_game_folder() {
        let dir = temp("xbox-escape");
        put(&dir.join("outside.png"));
        let game_dir = dir.join("Content");
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::write(
            game_dir.join("appxmanifest.xml"),
            r"<Logo>..\outside.png</Logo>",
        )
        .unwrap();
        assert_eq!(
            find(&install("xbox:x", StoreKind::Xbox, &game_dir), None),
            None
        );
        // Root-relative (`\Users\…`) and drive-relative (`C:x`) paths aren't "absolute" on
        // Windows but would still leave the folder.
        let outside = dir.join("outside.png").to_string_lossy().into_owned();
        let (drive, rest) = outside.split_at(2); // "C:", "\Users\…\outside.png"
        for logo in [rest.to_string(), format!("{drive}outside.png")] {
            std::fs::write(
                game_dir.join("appxmanifest.xml"),
                format!("<Logo>{logo}</Logo>"),
            )
            .unwrap();
            assert_eq!(
                find(&install("xbox:x", StoreKind::Xbox, &game_dir), None),
                None,
                "{logo}"
            );
        }
        assert!(inside(Path::new(r"Assets\Logo.png")));
        assert!(!inside(Path::new(r"\Assets\Logo.png")));
        assert!(!inside(Path::new("C:Logo.png")));
        assert!(!inside(Path::new("")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn epic_and_own_games_have_no_store_cover() {
        let dir = temp("epic");
        assert_eq!(find(&install("epic:x", StoreKind::Epic, &dir), None), None);
        assert_eq!(find(&install("own:x", StoreKind::Own, &dir), None), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

/// Manual check on a real PC (not run in CI): `cargo test -p ot-saves covers_on_this_pc -- --ignored --nocapture`.
#[cfg(test)]
mod this_pc {
    #[test]
    #[ignore = "reads this PC's game list"]
    fn covers_on_this_pc() {
        let steam = crate::steam::steam_dir();
        let games = crate::installs::detect();
        let mut found = 0;
        for g in &games {
            let c = super::find(g, steam.as_deref());
            found += usize::from(c.is_some());
            println!("{:<45} {:?}", g.name, c.map(|c| (c.kind, c.path)));
        }
        println!("{found} of {} games have a store cover", games.len());
    }
}
