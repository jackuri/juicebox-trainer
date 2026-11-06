//! The anti-cheat guard (PLAN.md §3.1). It decides whether OpenTrainer may
//! attach to a process.
//!
//! It blocks when any of these is true:
//! - a known anti-cheat process, service or kernel driver is running;
//! - a known anti-cheat module is loaded in the target;
//! - the target's install folder contains an anti-cheat marker (so we catch
//!   anti-cheat that only starts with the game's launcher);
//! - the target is on the built-in online-only list;
//! - any of the checks above could not be run ("fail closed").
//!
//! There is no setting, flag, feature or environment variable that turns this
//! off, and the signature list is compiled into the binary. A test in this
//! module keeps it that way.

mod collect;

use std::fmt;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// What kind of evidence a finding is based on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FindingKind {
    Process,
    Service,
    Driver,
    Module,
    InstallMarker,
    OnlineOnly,
    /// A check could not run, so the guard can't vouch for the target.
    Unverifiable,
}

/// One reason the guard blocked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub kind: FindingKind,
    /// Anti-cheat product or game name.
    pub product: String,
    /// The matching name, or what failed.
    pub detail: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self.kind {
            FindingKind::Process => "process running",
            FindingKind::Service => "service running",
            FindingKind::Driver => "driver loaded",
            FindingKind::Module => "module loaded in game",
            FindingKind::InstallMarker => "found in game folder",
            FindingKind::OnlineOnly => "online-only game",
            FindingKind::Unverifiable => "check failed",
        };
        write!(f, "{} — {what}: {}", self.product, self.detail)
    }
}

/// The guard's decision for one process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    pub pid: u32,
    /// Target exe name.
    pub target: String,
    pub allowed: bool,
    pub findings: Vec<Finding>,
    /// Version of the built-in signature list used.
    pub signatures_version: u32,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.allowed {
            return write!(f, "{} (pid {}): no anti-cheat found", self.target, self.pid);
        }
        write!(f, "{} (pid {}): ", self.target, self.pid)?;
        for (i, finding) in self.findings.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{finding}")?;
        }
        Ok(())
    }
}

/// Runs every check against `pid` and returns the verdict. Never fails:
/// a check that can't run becomes an [`FindingKind::Unverifiable`] finding.
pub fn check(pid: u32) -> Verdict {
    let snapshot = collect::collect(pid);
    let findings = evaluate(&snapshot, signatures());
    Verdict {
        pid,
        target: snapshot.target_name.clone(),
        allowed: findings.is_empty(),
        findings,
        signatures_version: signatures().version,
    }
}

/// Everything the guard looked at, gathered by [`collect::collect`].
#[derive(Debug, Default, Clone)]
pub(crate) struct Snapshot {
    pub target_name: String,
    /// Names of all running processes.
    pub processes: Vec<String>,
    /// Names of running services, including kernel-driver services.
    pub services: Vec<String>,
    /// File names of loaded kernel drivers (best effort, see collect.rs).
    pub drivers: Vec<String>,
    /// File names of modules loaded in the target.
    pub modules: Vec<String>,
    /// Entry names in the target's folder and a few parent folders.
    pub install_entries: Vec<String>,
    /// Checks that failed to run.
    pub errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SignatureDb {
    #[serde(rename = "_comment")]
    _comment: String,
    pub version: u32,
    pub products: Vec<Product>,
    pub online_only: Vec<OnlineOnlyGame>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Product {
    pub name: String,
    pub processes: Vec<String>,
    pub services: Vec<String>,
    pub drivers: Vec<String>,
    pub modules: Vec<String>,
    pub install_markers: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OnlineOnlyGame {
    pub name: String,
    pub exes: Vec<String>,
}

const SIGNATURES_JSON: &str = include_str!("signatures.json");

pub(crate) fn signatures() -> &'static SignatureDb {
    static DB: OnceLock<SignatureDb> = OnceLock::new();
    DB.get_or_init(|| {
        serde_json::from_str(SIGNATURES_JSON)
            .expect("built-in guard signatures are valid (checked by tests)")
    })
}

fn any_match<'a>(haystack: &'a [String], needles: &[String]) -> Vec<&'a str> {
    haystack
        .iter()
        .filter(|h| needles.iter().any(|n| n.eq_ignore_ascii_case(h)))
        .map(String::as_str)
        .collect()
}

/// Pure decision function: which findings does this snapshot produce?
pub(crate) fn evaluate(s: &Snapshot, db: &SignatureDb) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut push = |kind, product: &str, detail: &str| {
        let f = Finding {
            kind,
            product: product.to_string(),
            detail: detail.to_string(),
        };
        if !out.contains(&f) {
            out.push(f);
        }
    };

    for error in &s.errors {
        push(FindingKind::Unverifiable, "OpenTrainer guard", error);
    }
    for p in &db.products {
        for hit in any_match(&s.processes, &p.processes) {
            push(FindingKind::Process, &p.name, hit);
        }
        for hit in any_match(&s.services, &p.services) {
            push(FindingKind::Service, &p.name, hit);
        }
        for hit in any_match(&s.drivers, &p.drivers) {
            push(FindingKind::Driver, &p.name, hit);
        }
        for hit in any_match(&s.modules, &p.modules) {
            push(FindingKind::Module, &p.name, hit);
        }
        for hit in any_match(&s.install_entries, &p.install_markers) {
            push(FindingKind::InstallMarker, &p.name, hit);
        }
    }
    for game in &db.online_only {
        if game
            .exes
            .iter()
            .any(|e| e.eq_ignore_ascii_case(&s.target_name))
        {
            push(FindingKind::OnlineOnly, &game.name, &s.target_name);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean() -> Snapshot {
        Snapshot {
            target_name: "testbed_native.exe".into(),
            processes: vec!["explorer.exe".into(), "testbed_native.exe".into()],
            services: vec!["Dnscache".into(), "WinDefend".into()],
            drivers: vec!["ntoskrnl.exe".into(), "tcpip.sys".into()],
            modules: vec!["testbed_native.exe".into(), "ntdll.dll".into()],
            install_entries: vec!["testbed_native.exe".into(), "data".into()],
            errors: vec![],
        }
    }

    fn kinds(findings: &[Finding]) -> Vec<FindingKind> {
        findings.iter().map(|f| f.kind).collect()
    }

    #[test]
    fn clean_system_is_allowed() {
        assert!(evaluate(&clean(), signatures()).is_empty());
    }

    #[test]
    fn running_anticheat_process_blocks_case_insensitively() {
        let mut s = clean();
        s.processes.push("easyanticheat_eos.EXE".into());
        let f = evaluate(&s, signatures());
        assert_eq!(kinds(&f), vec![FindingKind::Process]);
        assert_eq!(f[0].product, "Easy Anti-Cheat");
    }

    #[test]
    fn running_service_blocks() {
        let mut s = clean();
        s.services.push("BEService".into());
        assert_eq!(
            kinds(&evaluate(&s, signatures())),
            vec![FindingKind::Service]
        );
    }

    #[test]
    fn loaded_driver_blocks() {
        let mut s = clean();
        s.drivers.push("vgk.sys".into());
        let f = evaluate(&s, signatures());
        assert_eq!(kinds(&f), vec![FindingKind::Driver]);
        assert_eq!(f[0].product, "Riot Vanguard");
    }

    #[test]
    fn module_in_target_blocks() {
        let mut s = clean();
        s.modules.push("BEClient_x64.dll".into());
        assert_eq!(
            kinds(&evaluate(&s, signatures())),
            vec![FindingKind::Module]
        );
    }

    #[test]
    fn install_marker_blocks() {
        let mut s = clean();
        s.install_entries.push("EasyAntiCheat".into());
        assert_eq!(
            kinds(&evaluate(&s, signatures())),
            vec![FindingKind::InstallMarker]
        );
    }

    #[test]
    fn online_only_game_blocks() {
        let mut s = clean();
        s.target_name = "CS2.exe".into();
        let f = evaluate(&s, signatures());
        assert_eq!(kinds(&f), vec![FindingKind::OnlineOnly]);
        assert_eq!(f[0].product, "Counter-Strike 2");
    }

    #[test]
    fn failed_check_blocks() {
        let mut s = clean();
        s.errors.push("could not list services".into());
        assert_eq!(
            kinds(&evaluate(&s, signatures())),
            vec![FindingKind::Unverifiable]
        );
    }

    #[test]
    fn findings_are_deduplicated() {
        let mut s = clean();
        s.processes.push("BEService.exe".into());
        s.processes.push("BEService.exe".into());
        assert_eq!(evaluate(&s, signatures()).len(), 1);
    }

    #[test]
    fn partial_names_do_not_match() {
        let mut s = clean();
        s.processes.push("notEasyAntiCheat.exe".into());
        s.install_entries.push("BattlEye-wallpapers.png".into());
        assert!(evaluate(&s, signatures()).is_empty());
    }

    #[test]
    fn verdict_display_lists_findings() {
        let v = Verdict {
            pid: 7,
            target: "game.exe".into(),
            allowed: false,
            findings: vec![Finding {
                kind: FindingKind::Driver,
                product: "Riot Vanguard".into(),
                detail: "vgk.sys".into(),
            }],
            signatures_version: 1,
        };
        assert_eq!(
            v.to_string(),
            "game.exe (pid 7): Riot Vanguard — driver loaded: vgk.sys"
        );
    }

    #[test]
    fn signature_db_is_well_formed() {
        let db: SignatureDb = serde_json::from_str(SIGNATURES_JSON).expect("valid JSON");
        assert_eq!(db.version, 1);
        assert!(db.products.len() >= 10);
        let mut names = std::collections::HashSet::new();
        for p in &db.products {
            assert!(
                names.insert(p.name.to_lowercase()),
                "duplicate product {}",
                p.name
            );
            let all = [
                &p.processes,
                &p.services,
                &p.drivers,
                &p.modules,
                &p.install_markers,
            ];
            assert!(
                all.iter().any(|l| !l.is_empty()),
                "{} has no indicators",
                p.name
            );
            for entry in all.iter().flat_map(|l| l.iter()) {
                assert!(!entry.trim().is_empty(), "{} has an empty entry", p.name);
                assert_eq!(entry.trim(), entry, "{}: stray whitespace", p.name);
            }
            for d in &p.drivers {
                assert!(d.to_lowercase().ends_with(".sys"), "{}: driver {d}", p.name);
            }
        }
        for g in &db.online_only {
            assert!(!g.exes.is_empty(), "{} has no exes", g.name);
            for e in &g.exes {
                assert!(e.to_lowercase().ends_with(".exe"), "{}: {e}", g.name);
            }
        }
    }

    /// The guard must have no off switch (CLAUDE.md hard rule). Fail the build
    /// if anyone adds an env var, feature flag or config lookup to it.
    #[test]
    fn guard_has_no_off_switch() {
        let sources = [
            ("guard/mod.rs", include_str!("mod.rs")),
            ("guard/collect.rs", include_str!("collect.rs")),
            ("session.rs", include_str!("../session.rs")),
        ];
        let forbidden = [
            ["env", "::var"].concat(),
            ["env", "::var_os"].concat(),
            ["cfg(", "feature"].concat(),
            ["std::", "fs::read_to_string"].concat(),
            ["disable", "_guard"].concat(),
            ["skip", "_guard"].concat(),
            ["bypass", "_guard"].concat(),
        ];
        for (file, src) in sources {
            for needle in &forbidden {
                assert!(!src.contains(needle.as_str()), "{file} contains `{needle}`");
            }
        }
    }
}
