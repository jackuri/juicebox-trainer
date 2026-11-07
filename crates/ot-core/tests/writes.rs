//! Find what writes to an address, against the testbed (never a real game).

use std::sync::Arc;

use ot_core::AttachedProcess;
use ot_core::writes::WriteWatch;
use testbed_harness::{Flavor, Testbed, serial, testbed_exe};

fn finds_the_gold_writer(flavor: Flavor) {
    let _s = serial();
    let mut game = Testbed::spawn(&testbed_exe(flavor));
    let p = Arc::new(AttachedProcess::attach(game.pid()).unwrap());
    let gold = game.addr("gold");
    let watch = WriteWatch::start(Arc::clone(&p), gold, 4).unwrap();
    let before = game.send("state")["gold"].as_i64().unwrap();
    game.send("earn 10");
    game.send("earn 5");
    let writers = watch.stop().unwrap();
    assert!(!writers.is_empty(), "no writer seen");
    let w = &writers[0];
    let expected = if flavor == Flavor::X86 {
        "mov [ecx],eax"
    } else {
        "mov [rcx],eax"
    };
    assert_eq!(w.text, expected, "{writers:?}");
    assert_eq!(w.bytes, "89 01");
    assert_eq!(w.count, 2);
    assert!(
        w.location
            .as_deref()
            .unwrap()
            .starts_with("testbed_native.exe+")
    );
    // The code there is the earn site (marker 4), just before the store.
    let site = p.read_bytes(w.address - 4, 6).unwrap();
    assert_eq!(site, vec![0x8B, 0x01, 0x01, 0xD0, 0x89, 0x01]);

    // The game keeps running normally after the debugger left.
    let after = game.send("earn 1")["gold"].as_i64().unwrap();
    assert_eq!(after, before + 16);
    assert_eq!(game.send("hit 1")["event"], "state");
}

#[test]
fn finds_the_gold_writer_x64() {
    finds_the_gold_writer(Flavor::X64);
}

#[test]
fn finds_the_gold_writer_x86() {
    finds_the_gold_writer(Flavor::X86);
}

#[test]
fn refuses_bad_sizes_and_alignment() {
    let _s = serial();
    let game = Testbed::spawn(&testbed_exe(Flavor::X64));
    let p = Arc::new(AttachedProcess::attach(game.pid()).unwrap());
    let gold = game.addr("gold");
    assert!(WriteWatch::start(Arc::clone(&p), gold, 3).is_err());
    assert!(WriteWatch::start(Arc::clone(&p), gold + 1, 4).is_err());
}
