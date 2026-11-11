//! Integration tests against `testbed/native` (never a real game) for the
//! memory viewer (v1.12): partial reads across unreadable pages, regions, and
//! byte edits (data written, code journaled and restored, read-only refused).

use jb_core::{AttachedProcess, ByteEdit, Error, Pattern, Value, ValueType};
use testbed_harness::*;

const DAMAGE: &str = "0F 1F 80 54 4F 54 01 8B 01 29 D0 89 01";

fn views_and_edits(flavor: Flavor) {
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(flavor));
    let p = AttachedProcess::attach(tb.pid()).unwrap();

    // Reading the game's gold through the viewer.
    let gold = tb.addr("gold");
    let (bytes, ok) = p.read_partial(gold, 4).unwrap();
    assert!(ok.iter().all(|&b| b));
    assert_eq!(i32::from_le_bytes(bytes.try_into().unwrap()), 250);
    let region = p
        .region_at(gold)
        .unwrap()
        .expect("gold is in committed memory");
    assert!(region.readable && region.writable && !region.executable);
    assert!(region.base <= gold && gold < region.base + region.size);

    // A read that runs off the end of a region into memory that isn't there.
    let regions = p.regions().unwrap();
    let edge = regions
        .windows(2)
        .find(|w| w[0].readable && w[0].base + w[0].size < w[1].base)
        .map(|w| w[0].base + w[0].size)
        .expect("a gap between regions");
    let (_, ok) = p.read_partial(edge - 8, 16).unwrap();
    assert_eq!(&ok[..8], &[true; 8]);
    assert_eq!(&ok[8..], &[false; 8]);

    // Data: written directly, seen by the game.
    assert_eq!(
        p.edit_bytes(gold, &5000i32.to_le_bytes()).unwrap(),
        ByteEdit::Data
    );
    assert_eq!(tb.send("state")["gold"], 5000);
    assert_eq!(
        p.edit_bytes(gold, &5000i32.to_le_bytes()).unwrap(),
        ByteEdit::Unchanged
    );
    assert_eq!(
        p.read_value(gold, ValueType::I32).unwrap(),
        Value::I32(5000)
    );

    // The exe's header is read-only: refused.
    let exe = p.main_module().unwrap();
    let err = p.edit_bytes(exe.base, &[0x00]).unwrap_err();
    assert!(matches!(err, Error::PatchRejected(_)), "{err}");
    assert_eq!(p.read_bytes(exe.base, 2).unwrap(), b"MZ");

    // Code: changed through the journal and put back on detach.
    if flavor == Flavor::X64 {
        let hits = p
            .aob_scan(&exe, &Pattern::parse(DAMAGE).unwrap(), 2)
            .unwrap();
        assert_eq!(hits.len(), 1);
        let sub = hits[0] + 9;
        assert_eq!(p.read_bytes(sub, 2).unwrap(), [0x29, 0xD0]);
        assert_eq!(p.edit_bytes(sub, &[0x90, 0x90]).unwrap(), ByteEdit::Code);
        assert_eq!(p.code_changes().len(), 1);
        assert_eq!(tb.send("hit 10")["health"], 100);
        drop(p);
        assert_eq!(tb.send("hit 10")["health"], 90, "detach restored the code");
    }

    // Too much at once.
    let p = AttachedProcess::attach(tb.pid()).unwrap();
    assert!(p.edit_bytes(gold, &[0; 65]).is_err());
    assert!(p.edit_bytes(gold, &[]).is_err());
}

#[test]
fn memory_viewer_x64() {
    views_and_edits(Flavor::X64);
}

#[test]
fn memory_viewer_x86() {
    views_and_edits(Flavor::X86);
}
