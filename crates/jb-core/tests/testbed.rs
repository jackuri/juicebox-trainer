//! Integration tests against `testbed/native` (never a real game): attach,
//! typed read/write, module map and the anti-cheat guard. Helpers are in
//! `testbed/harness`.

use jb_core::modules::find;
use jb_core::{AttachedProcess, Error, FindingKind, Value, ValueType};
use testbed_harness::*;

fn read_write_roundtrip(flavor: Flavor) {
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(flavor));
    assert_eq!(tb.ready["arch"], flavor.arch().to_string());

    // Process list sees it with the right architecture.
    let list = jb_core::list_processes().unwrap();
    let entry = list
        .iter()
        .find(|p| p.pid == tb.pid())
        .expect("testbed listed");
    assert_eq!(entry.name, "testbed_native.exe");
    assert_eq!(entry.arch, flavor.arch());

    let session = AttachedProcess::attach(tb.pid()).expect("guard allows the testbed");
    assert_eq!(session.arch(), flavor.arch());

    // Module map.
    let modules = session.modules();
    let exe = find(&modules, "testbed_native.exe").expect("exe module");
    assert_eq!(exe.base, tb.addr("module_base"));
    assert!(find(&modules, "kernel32.dll").is_some());
    if flavor == Flavor::X86 {
        assert!(
            modules.iter().all(|m| m.end() <= 1 << 32),
            "64-bit modules leaked"
        );
    }

    // Typed reads.
    let gold = tb.addr("gold");
    let speed = tb.addr("speed");
    let health = tb.addr("health");
    assert_eq!(
        session.read_value(gold, ValueType::I32).unwrap(),
        Value::I32(250)
    );
    assert_eq!(
        session.read_value(speed, ValueType::F32).unwrap(),
        Value::F32(1.0)
    );

    // Typed writes, confirmed through the testbed's own view.
    session.write_value(gold, &Value::I32(9999)).unwrap();
    session.write_value(speed, &Value::F32(2.5)).unwrap();
    let state = tb.send("state");
    assert_eq!(state["gold"], 9999);
    assert_eq!(state["speed"].as_f64(), Some(2.5));

    // Changes made by the game are visible to us.
    tb.send("hit 30");
    assert_eq!(
        session.read_value(health, ValueType::I32).unwrap(),
        Value::I32(70)
    );

    // Pointer chain: exe+root -> World -> Player -> Stats.
    let chain = &tb.ready["chain"];
    let root = session.resolve(chain["root"].as_str().unwrap()).unwrap();
    let world = session.read_pointer(root).unwrap();
    let player = session
        .read_pointer(world + hex(&chain["offsets"][0]))
        .unwrap();
    let stats = session
        .read_pointer(player + hex(&chain["offsets"][1]))
        .unwrap();
    assert_eq!(stats + hex(&chain["fields"]["gold"]), gold);

    // The gold address sits in a committed, writable region.
    let regions = session.regions().unwrap();
    assert!(
        regions
            .iter()
            .any(|r| r.writable && r.base <= gold && gold < r.base + r.size)
    );

    session.detach();
}

#[test]
fn attach_read_write_x64() {
    read_write_roundtrip(Flavor::X64);
}

#[test]
fn attach_read_write_x86_wow64() {
    read_write_roundtrip(Flavor::X86);
}

#[test]
fn resolves_module_relative_addresses() {
    let _g = serial();
    let tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let session = AttachedProcess::attach(tb.pid()).unwrap();
    let base = tb.addr("module_base");
    assert_eq!(
        session.resolve("testbed_native.exe+0x10").unwrap(),
        base + 0x10
    );
    assert_eq!(session.resolve("TESTBED_NATIVE.EXE").unwrap(), base);
    assert!(matches!(
        session.resolve("nope.dll+1"),
        Err(Error::ModuleNotFound(_))
    ));
}

#[test]
fn x86_target_rejects_addresses_above_4gib() {
    let _g = serial();
    let tb = Testbed::spawn(&testbed_exe(Flavor::X86));
    let session = AttachedProcess::attach(tb.pid()).unwrap();
    assert!(matches!(
        session.read_bytes(0x1_0000_0000, 4),
        Err(Error::AddressOutOfRange { .. })
    ));
    assert!(matches!(
        session.read_bytes(0, 4),
        Err(Error::AddressOutOfRange { .. })
    ));
}

#[test]
fn reads_fail_after_target_exits() {
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let gold = tb.addr("gold");
    let session = AttachedProcess::attach(tb.pid()).unwrap();
    assert!(session.is_alive());
    assert_eq!(session.exit_code(), None);
    assert_eq!(session.exit_time(), None);
    tb.kill();
    assert!(!session.is_alive());
    // Ended from outside (like Task Manager): exit code 1, a moment ago (v1.22.0).
    assert_eq!(session.exit_code(), Some(1));
    let ago = session.exit_time().unwrap().elapsed().unwrap_or_default();
    assert!(ago < std::time::Duration::from_secs(30), "{ago:?}");
    assert!(matches!(
        session.read_value(gold, ValueType::I32),
        Err(Error::ProcessExited { .. })
    ));
}

#[test]
fn guard_blocks_while_anticheat_process_runs() {
    let _g = serial();
    let dir = TempDir::new("fake-eac-process");
    let tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let fake = Testbed::spawn(&dir.copy_testbed("EasyAntiCheat.exe"));

    let v = blocked(AttachedProcess::attach(tb.pid()).unwrap_err());
    assert!(!v.allowed);
    assert!(v.findings.iter().any(|f| f.kind == FindingKind::Process
        && f.product == "Easy Anti-Cheat"
        && f.detail.eq_ignore_ascii_case("EasyAntiCheat.exe")));

    drop(fake);
    AttachedProcess::attach(tb.pid()).expect("allowed once the anti-cheat is gone");
}

#[test]
fn game_with_anticheat_folder_attaches_only_while_it_is_not_running() {
    let _g = serial();
    let dir = TempDir::new("battleye-marker");
    std::fs::create_dir_all(dir.0.join("MyGame").join("BattlEye")).unwrap();
    let tb = Testbed::spawn(&dir.copy_testbed(r"MyGame\testbed_native.exe"));

    // Shipped with the game but not running (an official no-anti-cheat mode).
    let v = jb_core::guard::check(tb.pid());
    assert!(v.allowed, "{v}");
    assert!(
        v.notes
            .iter()
            .any(|f| f.kind == FindingKind::InstallMarker && f.product == "BattlEye")
    );
    AttachedProcess::attach(tb.pid()).expect("nothing of the anti-cheat runs");

    // Running, it blocks.
    let fake = Testbed::spawn(&dir.copy_testbed("BEService.exe"));
    let v = blocked(AttachedProcess::attach(tb.pid()).unwrap_err());
    assert!(v.findings.iter().any(|f| f.kind == FindingKind::Process));
    assert!(
        v.findings
            .iter()
            .any(|f| f.kind == FindingKind::InstallMarker)
    );
    drop(fake);
}

#[test]
fn anticheat_folder_above_unreal_style_binaries_folder_is_seen() {
    let _g = serial();
    let dir = TempDir::new("eac-unreal-layout");
    std::fs::create_dir_all(dir.0.join("Game").join("EasyAntiCheat")).unwrap();
    let exe = dir.copy_testbed(r"Game\Binaries\Win64\testbed_native.exe");
    let tb = Testbed::spawn(&exe);

    let v = jb_core::guard::check(tb.pid());
    assert!(v.notes.iter().any(|f| f.kind == FindingKind::InstallMarker));
    // Changing the game's files while it isn't running stays blocked.
    let v = jb_core::guard::check_install(&exe);
    assert!(!v.allowed);
    assert!(
        v.findings
            .iter()
            .any(|f| f.kind == FindingKind::InstallMarker)
    );
}

#[test]
fn guard_blocks_online_only_game() {
    let _g = serial();
    let dir = TempDir::new("online-only");
    let tb = Testbed::spawn(&dir.copy_testbed("cs2.exe"));

    let v = blocked(AttachedProcess::attach(tb.pid()).unwrap_err());
    assert!(v.findings.iter().any(|f| f.kind == FindingKind::OnlineOnly));
}

#[test]
fn session_stays_blocked_once_anticheat_appears() {
    let _g = serial();
    let dir = TempDir::new("late-eac");
    let tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let gold = tb.addr("gold");
    let session = AttachedProcess::attach(tb.pid()).unwrap();
    assert!(session.read_value(gold, ValueType::I32).is_ok());

    let fake = Testbed::spawn(&dir.copy_testbed("EasyAntiCheat.exe"));
    blocked(session.recheck_guard().unwrap_err());
    drop(fake);

    // No way back for this session: detach and attach again.
    blocked(session.read_value(gold, ValueType::I32).unwrap_err());
    blocked(session.write_value(gold, &Value::I32(1)).unwrap_err());
}

#[test]
fn system_process_cannot_be_attached() {
    let _g = serial();
    assert!(AttachedProcess::attach(4).is_err());
}
