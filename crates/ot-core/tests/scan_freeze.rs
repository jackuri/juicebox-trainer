//! Scanner and freezer integration tests against `testbed/native`.

use std::sync::Arc;
use std::time::Duration;

use ot_core::{
    AttachedProcess, Error, FirstScan, Freezer, NextScan, Scan, ScanControl, ScanOptions, Value,
    ValueType,
};
use testbed_harness::*;

fn contains(scan: &Scan, address: u64) -> bool {
    scan.hits(0, scan.count())
        .iter()
        .any(|h| h.address == address)
}

fn exact_scan_narrows_to_gold(flavor: Flavor, options: ScanOptions) {
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(flavor));
    let gold = tb.addr("gold");
    let session = AttachedProcess::attach(tb.pid()).unwrap();
    let ctl = ScanControl::new();

    let mut scan = Scan::first(
        &session,
        ValueType::I32,
        FirstScan::Exact(Value::I32(250)),
        options,
        &ctl,
    )
    .unwrap();
    assert!(contains(&scan, gold), "first scan misses gold");
    let (done, total) = ctl.progress();
    assert!(total > 0 && done == total, "progress {done}/{total}");

    tb.send("earn 17");
    scan.next(&session, NextScan::Exact(Value::I32(267)), &ctl)
        .unwrap();
    assert!(contains(&scan, gold), "next scan misses gold");
    assert!(scan.count() < 10, "{} candidates left", scan.count());
    assert_eq!(scan.rounds(), 2);
    let hit = scan
        .hits(0, 100)
        .into_iter()
        .find(|h| h.address == gold)
        .unwrap();
    assert_eq!(hit.value, Value::I32(267));
}

#[test]
fn exact_scan_x64() {
    exact_scan_narrows_to_gold(Flavor::X64, ScanOptions::default());
}

#[test]
fn exact_scan_x86_wow64() {
    exact_scan_narrows_to_gold(Flavor::X86, ScanOptions::default());
}

#[test]
fn exact_scan_unaligned() {
    exact_scan_narrows_to_gold(
        Flavor::X64,
        ScanOptions {
            aligned: false,
            ..ScanOptions::default()
        },
    );
}

#[test]
fn unknown_scan_then_changed_unchanged_increased_decreased() {
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let health = tb.addr("health");
    let session = AttachedProcess::attach(tb.pid()).unwrap();
    let ctl = ScanControl::new();

    let mut scan = Scan::first(
        &session,
        ValueType::I32,
        FirstScan::Unknown,
        ScanOptions::default(),
        &ctl,
    )
    .unwrap();
    let start = scan.count();
    assert!(contains(&scan, health));

    tb.send("hit 5");
    scan.next(&session, NextScan::Decreased, &ctl).unwrap();
    assert!(contains(&scan, health), "decreased");
    tb.send("heal");
    scan.next(&session, NextScan::Increased, &ctl).unwrap();
    assert!(contains(&scan, health), "increased");
    scan.next(&session, NextScan::Unchanged, &ctl).unwrap();
    assert!(contains(&scan, health), "unchanged");
    tb.send("hit 1");
    scan.next(&session, NextScan::Changed, &ctl).unwrap();
    assert!(contains(&scan, health), "changed");
    assert!(
        scan.count() < start / 100,
        "only narrowed from {start} to {}",
        scan.count()
    );
}

#[test]
fn float_range_scan_finds_speed() {
    let _g = serial();
    let tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let speed = tb.addr("speed");
    let session = AttachedProcess::attach(tb.pid()).unwrap();
    let ctl = ScanControl::new();

    let mut scan = Scan::first(
        &session,
        ValueType::F32,
        FirstScan::Between(Value::F32(1.5), Value::F32(0.5)),
        ScanOptions::default(),
        &ctl,
    )
    .unwrap();
    assert!(contains(&scan, speed));
    session.write_value(speed, &Value::F32(3.25)).unwrap();
    scan.next(&session, NextScan::Exact(Value::F32(3.25)), &ctl)
        .unwrap();
    assert!(contains(&scan, speed));
    assert!(scan.count() < 10);
}

#[test]
fn cancelled_scan_keeps_previous_results() {
    let _g = serial();
    let tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let gold = tb.addr("gold");
    let session = AttachedProcess::attach(tb.pid()).unwrap();
    let ctl = ScanControl::new();
    let first = |ctl: &ScanControl| {
        Scan::first(
            &session,
            ValueType::I32,
            FirstScan::Exact(Value::I32(250)),
            ScanOptions::default(),
            ctl,
        )
    };

    ctl.cancel();
    assert!(matches!(first(&ctl), Err(Error::Cancelled)));

    ctl.reset();
    let mut scan = first(&ctl).unwrap();
    let before = scan.count();
    ctl.cancel();
    assert!(matches!(
        scan.next(&session, NextScan::Exact(Value::I32(1)), &ctl),
        Err(Error::Cancelled)
    ));
    assert_eq!(scan.count(), before);
    assert!(contains(&scan, gold));
}

#[test]
fn scan_rejects_mismatched_value_type() {
    let _g = serial();
    let tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let session = AttachedProcess::attach(tb.pid()).unwrap();
    assert!(matches!(
        Scan::first(
            &session,
            ValueType::I32,
            FirstScan::Exact(Value::F32(1.0)),
            ScanOptions::default(),
            &ScanControl::new()
        ),
        Err(Error::InvalidValue { .. })
    ));
}

#[test]
fn scan_is_blocked_once_guard_trips() {
    let _g = serial();
    let dir = TempDir::new("scan-eac");
    let tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let session = AttachedProcess::attach(tb.pid()).unwrap();
    let _fake = Testbed::spawn(&dir.copy_testbed("EasyAntiCheat.exe"));
    blocked(session.recheck_guard().unwrap_err());
    let err = Scan::first(
        &session,
        ValueType::I32,
        FirstScan::Unknown,
        ScanOptions::default(),
        &ScanControl::new(),
    )
    .unwrap_err();
    blocked(err);
}

fn wait_for(mut f: impl FnMut() -> bool) -> bool {
    for _ in 0..50 {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn freezer_holds_a_value_until_unfrozen() {
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let gold = tb.addr("gold");
    let session = Arc::new(AttachedProcess::attach(tb.pid()).unwrap());
    let freezer = Freezer::start(Arc::clone(&session), Duration::from_millis(20));

    freezer.set(1, gold, &Value::I32(5000));
    assert!(
        wait_for(|| tb.send("state")["gold"] == 5000),
        "never frozen"
    );
    tb.send("spend 100");
    assert!(
        wait_for(|| tb.send("state")["gold"] == 5000),
        "freeze did not restore gold"
    );
    assert!(freezer.is_frozen(1));

    freezer.remove(1);
    std::thread::sleep(Duration::from_millis(60));
    tb.send("spend 100");
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(tb.send("state")["gold"], 4900);
    assert!(freezer.fatal().is_none());
}

#[test]
fn freezer_stops_when_guard_blocks() {
    let _g = serial();
    let dir = TempDir::new("freeze-eac");
    let tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let gold = tb.addr("gold");
    let session = Arc::new(AttachedProcess::attach(tb.pid()).unwrap());
    let freezer = Freezer::start(Arc::clone(&session), Duration::from_millis(20));
    freezer.set(1, gold, &Value::I32(1234));

    let _fake = Testbed::spawn(&dir.copy_testbed("EasyAntiCheat.exe"));
    blocked(session.recheck_guard().unwrap_err());
    assert!(
        wait_for(|| freezer.fatal().is_some()),
        "freezer kept running"
    );
    assert!(matches!(freezer.fatal(), Some(Error::Blocked(_))));
    assert!(!freezer.is_running());
    assert!(!freezer.is_frozen(1));
}

#[test]
fn freezer_stops_when_game_exits() {
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let gold = tb.addr("gold");
    let session = Arc::new(AttachedProcess::attach(tb.pid()).unwrap());
    let freezer = Freezer::start(Arc::clone(&session), Duration::from_millis(20));
    freezer.set(1, gold, &Value::I32(1));
    tb.kill();
    assert!(wait_for(|| matches!(
        freezer.fatal(),
        Some(Error::ProcessExited { .. })
    )));
}

#[test]
fn finder_finds_whole_and_decimal_numbers_without_a_type() {
    use ot_core::{FindNext, FindStart, ValueFinder};
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let (gold, speed) = (tb.addr("gold"), tb.addr("speed"));
    let session = AttachedProcess::attach(tb.pid()).unwrap();
    let ctl = ScanControl::new();
    let has = |f: &ValueFinder, a: u64, ty: ValueType| {
        f.hits(f.count())
            .iter()
            .any(|h| h.address == a && h.ty == ty)
    };

    // Gold as the game shows it, with a thousands separator later on.
    let mut f = ValueFinder::start(&session, FindStart::Number("250".into()), &ctl).unwrap();
    assert!(has(&f, gold, ValueType::I32), "first search misses gold");
    tb.send("earn 1000");
    f.next(&session, FindNext::Number("1,250".into()), &ctl)
        .unwrap();
    assert!(has(&f, gold, ValueType::I32), "second search misses gold");
    assert!(f.count() < 10, "{} places left", f.count());
    assert_eq!(f.rounds(), 2);

    // Speed 1.0 shown as "1"; then 3.25 shown rounded as "3.3".
    let mut f = ValueFinder::start(&session, FindStart::Number("1".into()), &ctl).unwrap();
    assert!(has(&f, speed, ValueType::F32), "first search misses speed");
    session.write_value(speed, &Value::F32(3.25)).unwrap();
    f.next(&session, FindNext::Number("3.3".into()), &ctl)
        .unwrap();
    assert!(has(&f, speed, ValueType::F32), "second search misses speed");
    session.write_value(speed, &Value::F32(7.5)).unwrap();
    f.next(&session, FindNext::Increased, &ctl).unwrap();
    assert!(has(&f, speed, ValueType::F32), "increased misses speed");
}

#[test]
fn finder_without_a_number_reads_memory_both_ways() {
    use ot_core::{FindNext, FindStart, ValueFinder};
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let (health, speed) = (tb.addr("health"), tb.addr("speed"));
    let session = AttachedProcess::attach(tb.pid()).unwrap();
    let ctl = ScanControl::new();
    let has = |f: &ValueFinder, a: u64, ty: ValueType| {
        f.hits(f.count())
            .iter()
            .any(|h| h.address == a && h.ty == ty)
    };

    let mut f = ValueFinder::start(&session, FindStart::Unknown, &ctl).unwrap();
    let start = f.count();
    assert!(f.hits(10).is_empty(), "nothing to show before a comparison");
    tb.send("hit 5");
    session.write_value(speed, &Value::F32(0.5)).unwrap();
    f.next(&session, FindNext::Decreased, &ctl).unwrap();
    assert!(has(&f, health, ValueType::I32), "health (whole) missed");
    assert!(has(&f, speed, ValueType::F32), "speed (decimal) missed");
    f.next(&session, FindNext::Unchanged, &ctl).unwrap();
    assert!(has(&f, health, ValueType::I32));
    assert!(
        f.count() < start / 100,
        "only narrowed from {start} to {}",
        f.count()
    );
}
