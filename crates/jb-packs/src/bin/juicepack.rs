//! `juicepack` — check and share cheat packs from the command line.
//!
//! ```text
//! juicepack validate <pack.json>...         validate packs (exit 1 on any error)
//! juicepack export <pack.json> [out]        write a .juice file
//! juicepack inspect <file.juice>...      check .juice (or .otpack) files
//! juicepack index <packs-dir> [out.json]    build a community index.json
//! juicepack schema                          print the pack v1 JSON Schema
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use jb_packs::{LoadError, Severity, community, parse, share, store};

const USAGE: &str = "usage: juicepack validate <pack.json>... | export <pack.json> [out.juice] | \
inspect <file.juice>... | index <packs-dir> [index.json] | schema";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("schema") => {
            print!("{}", jb_packs::SCHEMA_JSON);
            ExitCode::SUCCESS
        }
        Some("validate") if args.len() > 1 => {
            let mut ok = true;
            for file in &args[1..] {
                ok &= validate(Path::new(file));
            }
            exit(ok)
        }
        Some("export") if (2..=3).contains(&args.len()) => {
            exit(export(Path::new(&args[1]), args.get(2).map(PathBuf::from)))
        }
        Some("inspect") if args.len() > 1 => {
            let mut ok = true;
            for file in &args[1..] {
                ok &= inspect(Path::new(file));
            }
            exit(ok)
        }
        Some("index") if (2..=3).contains(&args.len()) => {
            let dir = Path::new(&args[1]);
            let out = args.get(2).map_or_else(
                || dir.parent().unwrap_or(Path::new(".")).join("index.json"),
                PathBuf::from,
            );
            exit(index(dir, &out))
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn exit(ok: bool) -> ExitCode {
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn export(pack: &Path, out: Option<PathBuf>) -> bool {
    let result = store::read_file(pack).and_then(|text| {
        let (p, _) = parse(&text).map_err(|e| e.to_string())?;
        let bytes = share::export(&text, env!("CARGO_PKG_VERSION"), &share::today())
            .map_err(|e| e.to_string())?;
        let out = out.unwrap_or_else(|| PathBuf::from(share::file_name(&p)));
        std::fs::write(&out, bytes).map_err(|e| format!("{}: {e}", out.display()))?;
        Ok(out)
    });
    match result {
        Ok(out) => {
            println!("wrote {}", out.display());
            true
        }
        Err(e) => {
            println!("{}: error: {e}", pack.display());
            false
        }
    }
}

fn inspect(path: &Path) -> bool {
    match share::open_file(path) {
        Ok(o) => {
            for w in &o.warnings {
                println!("{}: {w}", path.display());
            }
            println!(
                "{}: ok — {} ({}), pack {} v{}, {} cheat(s), exported {} by JuiceBox Trainer {}",
                path.display(),
                o.pack.game.name,
                o.pack.game.exe,
                o.pack.game.id,
                o.pack.meta.pack_version,
                o.pack.cheats.len(),
                o.manifest.exported.as_deref().unwrap_or("?"),
                o.manifest.app.as_deref().unwrap_or("?"),
            );
            true
        }
        Err(e) => {
            println!("{}: error: {e}", path.display());
            false
        }
    }
}

/// Builds `index.json` from `<dir>/<id>/pack.json`. Every pack must be valid
/// and sit in the folder named by its `game.id`.
fn index(dir: &Path, out: &Path) -> bool {
    let mut ok = true;
    let mut packs = Vec::new();
    for found in store::scan_dir(dir) {
        let path = found.path.clone().unwrap_or_default();
        let folder = path
            .parent()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = match found.text {
            Ok(t) => t,
            Err(e) => {
                println!("{}: error: {e}", path.display());
                ok = false;
                continue;
            }
        };
        match parse(&text) {
            Ok((pack, _)) if pack.game.id == folder => {
                packs.push(community::entry_for(&pack, &text));
            }
            Ok((pack, _)) => {
                println!(
                    "{}: error: game.id is {} but the folder is {folder}",
                    path.display(),
                    pack.game.id
                );
                ok = false;
            }
            Err(e) => {
                println!("{}: error: {e}", path.display());
                ok = false;
            }
        }
    }
    if !ok {
        return false;
    }
    packs.sort_by_key(|e| e.name.to_lowercase());
    let index = community::Index {
        format: community::FORMAT,
        updated: Some(share::today()),
        packs,
        tables: Vec::new(),
        web: Vec::new(),
    };
    let json = match serde_json::to_string_pretty(&index) {
        Ok(j) => j + "\n",
        Err(e) => {
            println!("error: {e}");
            return false;
        }
    };
    if let Err(e) = std::fs::write(out, json) {
        println!("{}: error: {e}", out.display());
        return false;
    }
    println!("wrote {} ({} pack(s))", out.display(), index.packs.len());
    true
}

fn validate(path: &Path) -> bool {
    let text = match store::read_file(path) {
        Ok(t) => t,
        Err(e) => {
            println!("{}: error: {e}", path.display());
            return false;
        }
    };
    match parse(&text) {
        Ok((pack, warnings)) => {
            for w in &warnings {
                println!("{}: {w}", path.display());
            }
            println!(
                "{}: ok — {} ({}), {} cheat(s), {} warning(s)",
                path.display(),
                pack.game.name,
                pack.game.exe,
                pack.cheats.len(),
                warnings.len()
            );
            true
        }
        Err(LoadError::Syntax(e)) => {
            println!("{}: error: {e}", path.display());
            false
        }
        Err(LoadError::Invalid(issues)) => {
            for i in &issues {
                println!("{}: {i}", path.display());
            }
            let errors = issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .count();
            println!("{}: {errors} error(s)", path.display());
            false
        }
    }
}
