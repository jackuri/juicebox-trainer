//! `otpack` — check cheat packs from the command line.
//!
//! ```text
//! otpack validate <pack.json>...   validate packs (exit 1 on any error)
//! otpack schema                    print the pack v1 JSON Schema
//! ```

use std::path::Path;
use std::process::ExitCode;

use ot_packs::{LoadError, Severity, parse, store};

const USAGE: &str = "usage: otpack validate <pack.json>... | otpack schema";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("schema") => {
            print!("{}", ot_packs::SCHEMA_JSON);
            ExitCode::SUCCESS
        }
        Some("validate") if args.len() > 1 => {
            let mut ok = true;
            for file in &args[1..] {
                ok &= validate(Path::new(file));
            }
            if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
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
