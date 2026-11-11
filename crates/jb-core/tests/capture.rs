//! Reading a register when game code runs, against `testbed/native` (never a
//! real game): the damage code holds the health field's address in rcx/ecx
//! and the damage amount in edx.

use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use jb_core::capture::{CaptureSpec, Register, capture};
use jb_core::{AttachedProcess, Pattern};
use testbed_harness::*;

/// The damage code: marker 1, then `mov eax,[rcx]` at +7.
const DAMAGE: &str = "0F 1F 80 54 4F 54 01 8B 01";

fn captures_the_health_address(flavor: Flavor) {
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(flavor));
    let process = Arc::new(AttachedProcess::attach(tb.pid()).expect("attach to testbed"));
    let module = process.main_module().unwrap();
    let hits = process
        .aob_scan(&module, &Pattern::parse(DAMAGE).unwrap(), 2)
        .unwrap();
    assert_eq!(hits.len(), 1);
    let arch = process.arch();
    let (field, amount) = match flavor {
        Flavor::X64 => ("rcx", "edx"),
        Flavor::X86 => ("ecx", "edx"),
    };
    let spec = CaptureSpec {
        site: hits[0] + 7,
        register: Register::parse(field, arch).unwrap(),
        // Only a hit of exactly 7 counts.
        when: Some((Register::parse(amount, arch).unwrap(), 7)),
    };

    let p = Arc::clone(&process);
    let waiter = std::thread::spawn(move || capture(&p, spec, Duration::from_secs(10)));
    sleep(Duration::from_millis(600));
    // The game runs the code twice; only the second hit matches.
    assert_eq!(tb.send("hit 5")["health"], 95);
    assert_eq!(tb.send("hit 7")["health"], 88);
    let got = waiter.join().unwrap().unwrap();
    assert_eq!(got, Some(tb.addr("health")), "{flavor:?}");

    // Detached: the game keeps running normally, nothing was changed.
    assert_eq!(tb.send("hit 1")["health"], 87);
    assert_eq!(process.read_bytes(hits[0] + 7, 2).unwrap(), [0x8B, 0x01]);

    // Code that doesn't run in time gives nothing (and doesn't hang).
    let t = Instant::now();
    let none = capture(&process, spec, Duration::from_millis(800)).unwrap();
    assert_eq!(none, None);
    assert!(t.elapsed() < Duration::from_secs(5));
    assert_eq!(tb.send("hit 1")["health"], 86);
    tb.kill();
}

#[test]
fn capture_reads_a_register_in_the_x64_testbed() {
    captures_the_health_address(Flavor::X64);
}

#[test]
fn capture_reads_a_register_in_the_x86_testbed() {
    captures_the_health_address(Flavor::X86);
}
