//! Integration tests against `testbed/native` (never a real game) for the
//! v0.5 tools: verified code writes, crash-safe restore, the pointer scanner
//! and the signature maker.

use ot_core::pointer::{self, PointerScanOptions};
use ot_core::{AttachedProcess, Error, Pattern, ScanControl, journal, signature};
use testbed_harness::*;

const DAMAGE: &str = "0F 1F 80 54 4F 54 01 8B 01 29 D0 89 01";

/// Address of the code after a marker.
fn site(p: &AttachedProcess, aob: &str) -> u64 {
    let exe = p.main_module().unwrap();
    let hits = p.aob_scan(&exe, &Pattern::parse(aob).unwrap(), 2).unwrap();
    assert_eq!(hits.len(), 1, "{aob}");
    hits[0] + 7
}

#[test]
fn verified_code_writes_restore_on_detach() {
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let p = AttachedProcess::attach(tb.pid()).unwrap();
    let sub = site(&p, DAMAGE) + 2;

    // Wrong original bytes: refused, nothing written.
    let err = p.apply_code(sub, &[0x01, 0xD0], &[0x90, 0x90]).unwrap_err();
    assert!(matches!(err, Error::BytesMismatch { .. }), "{err}");
    assert_eq!(err.kind(), "needsRepair");
    assert_eq!(p.read_bytes(sub, 2).unwrap(), [0x29, 0xD0]);

    assert!(p.apply_code(sub, &[0x29, 0xD0], &[0x90, 0x90]).unwrap());
    // Applying again is a no-op.
    assert!(!p.apply_code(sub, &[0x29, 0xD0], &[0x90, 0x90]).unwrap());
    assert_eq!(p.code_changes().len(), 1);
    assert_eq!(tb.send("hit 10")["health"], 100);

    // A change overlapping another one is refused.
    let err = p
        .apply_code(sub - 2, &[0x8B, 0x01, 0x29, 0xD0], &[0x90; 4])
        .unwrap_err();
    assert!(matches!(err, Error::CodeOverlap { .. }), "{err}");

    drop(p);
    assert_eq!(tb.send("hit 10")["health"], 90, "detach restored the code");
}

#[test]
fn journal_undoes_changes_after_a_crash() {
    let _g = serial();
    let dir = TempDir::new("journal");
    let mut tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let p = AttachedProcess::attach(tb.pid()).unwrap();
    assert_eq!(p.enable_journal(&dir.0).unwrap(), 0);
    let sub = site(&p, DAMAGE) + 2;
    p.apply_code(sub, &[0x29, 0xD0], &[0x90, 0x90]).unwrap();
    let files: Vec<_> = std::fs::read_dir(&dir.0).unwrap().flatten().collect();
    assert_eq!(files.len(), 1, "journal written before the change");

    // Simulate OpenTrainer crashing: the session never cleans up.
    std::mem::forget(p);
    assert_eq!(tb.send("hit 10")["health"], 100, "still patched");

    let report = journal::recover(&dir.0);
    assert_eq!(report.len(), 1);
    assert_eq!(report[0].restored, 1, "{report:?}");
    assert!(report[0].problems.is_empty());
    assert!(!report[0].game_closed);
    assert_eq!(tb.send("hit 10")["health"], 90, "original code is back");
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0);

    // A journal whose game has closed is just removed.
    let p = AttachedProcess::attach(tb.pid()).unwrap();
    p.enable_journal(&dir.0).unwrap();
    p.apply_code(sub, &[0x29, 0xD0], &[0x90, 0x90]).unwrap();
    std::mem::forget(p);
    tb.kill();
    let report = journal::recover(&dir.0);
    assert!(report[0].game_closed);
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0);
}

#[test]
fn a_new_session_takes_over_leftover_changes() {
    let _g = serial();
    let dir = TempDir::new("adopt");
    let mut tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let p = AttachedProcess::attach(tb.pid()).unwrap();
    p.enable_journal(&dir.0).unwrap();
    let sub = site(&p, DAMAGE) + 2;
    p.apply_code(sub, &[0x29, 0xD0], &[0x90, 0x90]).unwrap();
    std::mem::forget(p);

    let again = AttachedProcess::attach(tb.pid()).unwrap();
    assert_eq!(again.enable_journal(&dir.0).unwrap(), 1);
    drop(again);
    assert_eq!(tb.send("hit 10")["health"], 90);
}

fn pointer_scan_finds_a_stable_path(flavor: Flavor) {
    let _g = serial();
    let exe = testbed_exe(flavor);
    let tb = Testbed::spawn(&exe);
    let p = AttachedProcess::attach(tb.pid()).unwrap();
    let chain = &tb.ready["chain"];
    let root = chain["root"].as_str().unwrap().to_string();
    let expected = format!(
        "[[[{root}]+{:#X}]+{:#X}]+{:#X}",
        hex(&chain["offsets"][0]),
        hex(&chain["offsets"][1]),
        hex(&chain["fields"]["gold"])
    );
    let control = ScanControl::new();
    let options = PointerScanOptions {
        max_depth: 3,
        max_offset: 0x100,
        max_results: 2000,
    };
    let paths = pointer::scan(&p, tb.addr("gold"), options, &control).unwrap();
    let texts: Vec<String> = paths.iter().map(ToString::to_string).collect();
    assert!(
        texts.iter().any(|t| t.eq_ignore_ascii_case(&expected)),
        "{expected} not in {texts:?}"
    );
    // The path reads back the same address, also as a typed expression.
    assert_eq!(p.resolve(&expected).unwrap(), tb.addr("gold"));
    drop(p);
    drop(tb);

    // After a restart the heap moves; the static path still leads to gold.
    let tb2 = Testbed::spawn(&exe);
    let p2 = AttachedProcess::attach(tb2.pid()).unwrap();
    let kept = pointer::filter(&p2, &paths, tb2.addr("gold")).unwrap();
    assert!(
        kept.iter()
            .any(|k| k.to_string().eq_ignore_ascii_case(&expected))
    );
    assert!(kept.len() <= paths.len());
}

#[test]
fn pointer_scan_finds_a_stable_path_x64() {
    pointer_scan_finds_a_stable_path(Flavor::X64);
}

#[test]
fn pointer_scan_finds_a_stable_path_x86() {
    pointer_scan_finds_a_stable_path(Flavor::X86);
}

#[test]
fn signature_maker_finds_the_site_again() {
    let _g = serial();
    for flavor in [Flavor::X64, Flavor::X86] {
        let tb = Testbed::spawn(&testbed_exe(flavor));
        let p = AttachedProcess::attach(tb.pid()).unwrap();
        // The damage `mov eax,[rcx]` is also at the fire site, so the
        // signature has to grow past it.
        let at = site(&p, DAMAGE);
        let sig = signature::make(&p, at).unwrap();
        assert!(
            sig.pattern.starts_with("8B 01 29 D0 89 01"),
            "{}",
            sig.pattern
        );
        assert!(sig.len > 6);
        let exe = p.main_module().unwrap();
        let hits = p
            .aob_scan(&exe, &Pattern::parse(&sig.pattern).unwrap(), 2)
            .unwrap();
        assert_eq!(hits, [at], "{flavor:?}: {}", sig.pattern);
        assert_eq!(sig.module_offset, at - exe.base);
        assert_eq!(
            sig.instructions[0].text,
            if flavor == Flavor::X64 {
                "mov eax,[rcx]"
            } else {
                "mov eax,[ecx]"
            }
        );
        // Not a module address.
        assert!(signature::make(&p, tb.addr("gold")).is_err());
    }
}
