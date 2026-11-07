//! Which engine a game is built with, from the files around its exe
//! (PLAN.md §5.3 "Recon"). Only file names and a few bytes of known engine
//! files are read; nothing is run or unpacked.

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Engine {
    UnityMono,
    UnityIl2cpp,
    Unreal,
    Godot,
    GameMaker,
    RenPy,
    RpgMakerMv,
    RpgMakerVx,
    Xna,
    Native,
}

impl Engine {
    pub fn name(self) -> &'static str {
        match self {
            Engine::UnityMono => "Unity (Mono)",
            Engine::UnityIl2cpp => "Unity (IL2CPP)",
            Engine::Unreal => "Unreal Engine",
            Engine::Godot => "Godot",
            Engine::GameMaker => "GameMaker",
            Engine::RenPy => "Ren'Py",
            Engine::RpgMakerMv => "RPG Maker MV/MZ",
            Engine::RpgMakerVx => "RPG Maker XP/VX/VX Ace",
            Engine::Xna => "XNA / FNA / MonoGame (.NET)",
            Engine::Native => "native (no known engine)",
        }
    }

    /// Tips for finding cheats in games made with this engine.
    pub fn hints(self) -> &'static [&'static str] {
        match self {
            Engine::UnityMono => &[
                "Game logic is .NET code (Assembly-CSharp.dll) compiled at run time; values live on the managed heap and move between runs.",
                "Call game_classes (e.g. query \"health\") to see the game's own field names and types before scanning.",
                "Prefer a code patch at the instruction found by find_what_writes over pointer paths; pointer paths usually start in mono-2.0-bdwgc.dll or UnityPlayer.dll if needed.",
                "Values are often float (health, speed) or int (money).",
            ],
            Engine::UnityIl2cpp => &[
                "Game code is compiled into GameAssembly.dll; signatures (AOBs) there are stable for one build.",
                "Pointer paths often start in GameAssembly.dll or UnityPlayer.dll.",
                "Call game_classes (e.g. query \"health\") to see the game's own field names (types aren't known for IL2CPP; health is usually float).",
                "Values are often float (health, speed) or int (money).",
            ],
            Engine::Unreal => &[
                "Call unreal_find (e.g. query \"Health\") to see the game's classes and properties, then with class and only to read a live object's values and addresses.",
                "Pointer paths usually start in the game's exe (the engine's GWorld / GEngine globals).",
                "Health and stamina are usually float; money usually int.",
            ],
            Engine::Godot => {
                &["Script values (GDScript) are Variants: numbers are often 64-bit (i64 or f64)."]
            }
            Engine::GameMaker => &["GameMaker stores most numbers as 64-bit doubles (f64)."],
            Engine::RenPy => &[
                "Ren'Py runs Python; numbers in memory are Python objects, which memory scans handle poorly. Its saves are pickle files, which OpenTrainer never opens (hex view only).",
            ],
            Engine::RpgMakerMv => &[
                "RPG Maker MV/MZ runs JavaScript: numbers are usually 64-bit doubles (f64). Its saves (.rpgsave / .rmmzsave) are compressed JSON.",
            ],
            Engine::RpgMakerVx => &[
                "RPG Maker XP/VX runs Ruby: an int n is often stored as 2n+1 in memory. Its saves are Ruby Marshal data (hex view only).",
            ],
            Engine::Xna => &[
                ".NET game: values live on the managed heap and move; prefer code patches found with find_what_writes.",
                "Call game_classes (e.g. query \"health\") to see the game's own field names and types before scanning.",
            ],
            Engine::Native => &[
                "Pointer paths usually start in the game's exe; code patches at the instruction found by find_what_writes are often the most stable.",
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    pub engine: Engine,
    pub name: &'static str,
    pub version: Option<String>,
    /// The files that gave it away.
    pub evidence: Vec<String>,
    pub hints: Vec<&'static str>,
}

fn has(dir: &Path, rel: &str) -> bool {
    dir.join(rel).exists()
}

fn any_ext(dir: &Path, ext: &str) -> Option<String> {
    std::fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        name.to_ascii_lowercase().ends_with(ext).then_some(name)
    })
}

/// A Unity version like `2021.3.5f1` in the first bytes of a data file.
fn unity_version(data: &Path) -> Option<String> {
    for file in ["globalgamemanagers", "data.unity3d", "mainData"] {
        let Ok(bytes) = std::fs::read(data.join(file)).map(|mut b| {
            b.truncate(4096);
            b
        }) else {
            continue;
        };
        let text: String = bytes
            .iter()
            .map(|&b| if b.is_ascii_graphic() { b as char } else { ' ' })
            .collect();
        for word in text.split(' ') {
            let mut parts = word.split('.');
            let (Some(year), Some(minor), Some(rest)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let ok = year.len() == 4
                && year.bytes().all(|b| b.is_ascii_digit())
                && !minor.is_empty()
                && minor.bytes().all(|b| b.is_ascii_digit())
                && rest.chars().next().is_some_and(|c| c.is_ascii_digit())
                && rest.chars().any(|c| matches!(c, 'a' | 'b' | 'f' | 'p'));
            if ok {
                return Some(word.to_string());
            }
        }
    }
    None
}

/// Detects the engine of the game at `exe`.
pub fn detect(exe: &Path) -> EngineInfo {
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = exe
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let data: PathBuf = dir.join(format!("{stem}_Data"));
    let mut evidence = Vec::new();
    let mut version = None;
    let found = |e: &mut Vec<String>, what: &str| e.push(what.to_string());

    let unity = has(&dir, "UnityPlayer.dll")
        || has(&data, "Managed")
        || has(&data, "globalgamemanagers")
        || has(&data, "il2cpp_data");
    let engine = if unity {
        version = unity_version(&data);
        if has(&dir, "GameAssembly.dll") {
            found(&mut evidence, "GameAssembly.dll");
            if has(&data, "il2cpp_data/Metadata/global-metadata.dat") {
                found(
                    &mut evidence,
                    &format!("{stem}_Data/il2cpp_data/Metadata/global-metadata.dat"),
                );
            }
            Engine::UnityIl2cpp
        } else {
            if has(&data, "Managed/Assembly-CSharp.dll") {
                found(
                    &mut evidence,
                    &format!("{stem}_Data/Managed/Assembly-CSharp.dll"),
                );
            }
            if has(&dir, "MonoBleedingEdge") {
                found(&mut evidence, "MonoBleedingEdge");
            }
            Engine::UnityMono
        }
    } else if stem.to_ascii_lowercase().ends_with("-shipping")
        || dir.ancestors().take(4).any(|a| {
            a.file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("Binaries"))
        }) && dir.ancestors().take(5).any(|a| has(a, "Engine"))
    {
        if stem.to_ascii_lowercase().ends_with("-shipping") {
            found(
                &mut evidence,
                &format!("{stem}.exe (Unreal shipping build)"),
            );
        }
        if let Some(root) = dir.ancestors().take(5).find(|a| has(a, "Engine")) {
            found(&mut evidence, &format!("{}\\Engine", root.display()));
        }
        version = unreal_version(exe);
        Engine::Unreal
    } else if has(&dir, &format!("{stem}.pck")) {
        found(&mut evidence, &format!("{stem}.pck"));
        Engine::Godot
    } else if has(&dir, "data.win") {
        found(&mut evidence, "data.win");
        Engine::GameMaker
    } else if has(&dir, "renpy")
        || (has(&dir, "game") && any_ext(&dir.join("game"), ".rpa").is_some())
    {
        found(&mut evidence, "renpy / game/*.rpa");
        Engine::RenPy
    } else if has(&dir, "www/js/rpg_core.js")
        || has(&dir, "js/rmmz_core.js")
        || has(&dir, "www/js/rmmz_core.js")
    {
        found(&mut evidence, "rpg_core.js / rmmz_core.js");
        Engine::RpgMakerMv
    } else if let Some(archive) = any_ext(&dir, ".rgss3a")
        .or_else(|| any_ext(&dir, ".rgss2a"))
        .or_else(|| any_ext(&dir, ".rgssad"))
    {
        found(&mut evidence, &archive);
        Engine::RpgMakerVx
    } else if let Some(fw) = [
        "MonoGame.Framework.dll",
        "FNA.dll",
        "Microsoft.Xna.Framework.dll",
    ]
    .into_iter()
    .find(|f| has(&dir, f))
    {
        found(&mut evidence, fw);
        Engine::Xna
    } else {
        Engine::Native
    };
    let mut hints = engine.hints().to_vec();
    if engine == Engine::Unreal {
        match version.as_deref() {
            Some(v) if v.starts_with('5') => hints.push(
                "Unreal Engine 5: positions and other vectors are 64-bit doubles (f64); health and money are usually still f32 / i32.",
            ),
            Some(_) => hints.push("Unreal Engine 4: positions and vectors are 32-bit floats (f32)."),
            None => {}
        }
    }
    EngineInfo {
        engine,
        name: engine.name(),
        version,
        evidence,
        hints,
    }
}

/// Exe files bigger than this aren't searched for a version.
const MAX_VERSION_SCAN: u64 = 512 << 20;

/// The engine version an Unreal build carries, e.g. `4.27` from
/// `++UE4+Release-4.27` (stored as ASCII or UTF-16 in the exe).
pub fn unreal_version(exe: &Path) -> Option<String> {
    use std::io::Read;
    if std::fs::metadata(exe).ok()?.len() > MAX_VERSION_SCAN {
        return None;
    }
    let mut file = std::fs::File::open(exe).ok()?;
    let mut buf = vec![0u8; 8 << 20];
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            return None;
        }
        let mut chunk = std::mem::take(&mut carry);
        chunk.extend_from_slice(&buf[..n]);
        if let Some(v) = find_release(&chunk) {
            return Some(v);
        }
        // Keep the tail so a string split across reads is still found.
        carry = chunk[chunk.len().saturating_sub(96)..].to_vec();
    }
}

/// `++UE4+Release-4.27` / `++UE5+Release-5.4` in ASCII or UTF-16LE.
fn find_release(b: &[u8]) -> Option<String> {
    let version_after = |text: &str| -> Option<String> {
        let rest = text.strip_prefix("Release-")?;
        let v: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let v = v.trim_end_matches('.').to_string();
        (v.contains('.')).then_some(v)
    };
    for major in *b"45" {
        let ascii = [b"++UE".as_slice(), &[major], b"+"].concat();
        let mut from = 0;
        while let Some(i) = b[from..]
            .windows(ascii.len())
            .position(|w| w == ascii.as_slice())
        {
            let at = from + i + ascii.len();
            let text = String::from_utf8_lossy(&b[at..(at + 24).min(b.len())]).into_owned();
            if let Some(v) = version_after(&text) {
                return Some(v);
            }
            from = at;
        }
        let wide: Vec<u8> = ascii.iter().flat_map(|&c| [c, 0]).collect();
        let mut from = 0;
        while let Some(i) = b[from..]
            .windows(wide.len())
            .position(|w| w == wide.as_slice())
        {
            let at = from + i + wide.len();
            let end = (at + 48).min(b.len());
            let units: Vec<u16> = b[at..end]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect();
            let text = String::from_utf16_lossy(&units);
            if let Some(v) = version_after(&text) {
                return Some(v);
            }
            from = at;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(files: &[&str], exe: &str) -> (PathBuf, EngineInfo) {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/tmp")
            .join(format!(
                "ot-engine-{}-{}",
                std::process::id(),
                exe.replace(['\\', '/'], "_")
            ));
        let _ = std::fs::remove_dir_all(&base);
        for f in files {
            let p = base.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            if f.ends_with('/') {
                std::fs::create_dir_all(&p).unwrap();
            } else {
                std::fs::write(&p, b"x").unwrap();
            }
        }
        let exe = base.join(exe);
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"").unwrap();
        let info = detect(&exe);
        (base, info)
    }

    #[test]
    fn detects_engines_from_files() {
        let cases: &[(&[&str], &str, Engine)] = &[
            (
                &[
                    "GameAssembly.dll",
                    "UnityPlayer.dll",
                    "Hero_Data/il2cpp_data/Metadata/global-metadata.dat",
                ],
                "Hero.exe",
                Engine::UnityIl2cpp,
            ),
            (
                &[
                    "UnityPlayer.dll",
                    "Hero_Data/Managed/Assembly-CSharp.dll",
                    "MonoBleedingEdge/",
                ],
                "Hero.exe",
                Engine::UnityMono,
            ),
            (
                &["Engine/Binaries/x", "Space/Binaries/Win64/x"],
                "Space/Binaries/Win64/Space-Win64-Shipping.exe",
                Engine::Unreal,
            ),
            (&["game.pck"], "game.exe", Engine::Godot),
            (&["data.win"], "game.exe", Engine::GameMaker),
            (&["renpy/", "game/archive.rpa"], "vn.exe", Engine::RenPy),
            (&["www/js/rpg_core.js"], "Game.exe", Engine::RpgMakerMv),
            (&["Game.rgss3a"], "Game.exe", Engine::RpgMakerVx),
            (&["FNA.dll"], "game.exe", Engine::Xna),
            (&["readme.txt"], "game.exe", Engine::Native),
        ];
        for (files, exe, want) in cases {
            let (base, info) = game(files, exe);
            assert_eq!(info.engine, *want, "{exe}: {info:?}");
            assert!(!info.hints.is_empty());
            let _ = std::fs::remove_dir_all(base);
        }
    }

    #[test]
    fn finds_unreal_versions_in_ascii_and_utf16() {
        let mut b = vec![0u8; 100];
        b.extend_from_slice(b"++UE4+Release-4.27\0junk");
        assert_eq!(find_release(&b).as_deref(), Some("4.27"));
        let wide: Vec<u8> = "xx++UE5+Release-5.4\0"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(find_release(&wide).as_deref(), Some("5.4"));
        assert_eq!(find_release(b"++UE4+Release-"), None);
        assert_eq!(find_release(b"nothing here"), None);

        let (base, _) = game(
            &["Engine/Binaries/x", "Space/Binaries/Win64/x"],
            "Space/Binaries/Win64/Space-Win64-Shipping.exe",
        );
        let exe = base.join("Space/Binaries/Win64/Space-Win64-Shipping.exe");
        std::fs::write(&exe, wide).unwrap();
        let info = detect(&exe);
        assert_eq!(info.version.as_deref(), Some("5.4"));
        assert!(info.hints.iter().any(|h| h.contains("64-bit doubles")));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn a_data_folder_alone_is_not_unity() {
        // The testbed writes <exe>_Data/app.info for its saves.
        let (base, info) = game(&["testbed_native_Data/app.info"], "testbed_native.exe");
        assert_eq!(info.engine, Engine::Native);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn reads_the_unity_version() {
        let (base, _) = game(&["UnityPlayer.dll"], "u.exe");
        let data = base.join("u_Data");
        std::fs::create_dir_all(&data).unwrap();
        let mut bytes = vec![0u8; 20];
        bytes.extend_from_slice(b"2021.3.5f1\0\0rest");
        std::fs::write(data.join("globalgamemanagers"), bytes).unwrap();
        assert_eq!(
            detect(&base.join("u.exe")).version.as_deref(),
            Some("2021.3.5f1")
        );
        let _ = std::fs::remove_dir_all(base);
    }
}
