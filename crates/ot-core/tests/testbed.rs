//! Integration tests against `testbed/native` (never a real game).
//!
//! The testbed is built on demand into `target/testbed` (a separate target dir
//! so the nested cargo doesn't wait on the outer build lock). Set
//! `OT_TESTBED_X64` / `OT_TESTBED_X86` to use prebuilt binaries instead.
//!
//! Tests are serialized: the guard looks at system-wide state, and some tests
//! briefly run a renamed copy of the testbed as a fake anti-cheat process.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock};

use ot_core::modules::find;
use ot_core::{Arch, AttachedProcess, Error, FindingKind, Value, ValueType};
use serde_json::Value as Json;

fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Flavor {
    X64,
    X86,
}

impl Flavor {
    fn arch(self) -> Arch {
        match self {
            Flavor::X64 => Arch::X64,
            Flavor::X86 => Arch::X86,
        }
    }
}

fn testbed_exe(flavor: Flavor) -> PathBuf {
    static X64: OnceLock<PathBuf> = OnceLock::new();
    static X86: OnceLock<PathBuf> = OnceLock::new();
    let (cell, var, target) = match flavor {
        Flavor::X64 => (&X64, "OT_TESTBED_X64", None),
        Flavor::X86 => (&X86, "OT_TESTBED_X86", Some("i686-pc-windows-msvc")),
    };
    cell.get_or_init(|| {
        if let Some(prebuilt) = std::env::var_os(var) {
            return PathBuf::from(prebuilt);
        }
        let root = workspace_root();
        let target_dir = root.join("target").join("testbed");
        let mut cmd = Command::new(env!("CARGO"));
        cmd.current_dir(&root)
            .args(["build", "-p", "testbed-native", "--bin", "testbed_native"])
            .env("CARGO_TARGET_DIR", &target_dir);
        if let Some(t) = target {
            cmd.args(["--target", t]);
        }
        let status = cmd.status().expect("run cargo to build the testbed");
        assert!(status.success(), "building the {flavor:?} testbed failed");
        let mut exe = target_dir;
        if let Some(t) = target {
            exe = exe.join(t);
        }
        exe.join("debug").join("testbed_native.exe")
    })
    .clone()
}

/// A running testbed with its JSON side channel.
struct Testbed {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    ready: Json,
}

impl Testbed {
    fn spawn(exe: &Path) -> Self {
        let mut child = Command::new(exe)
            .arg("--json")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("start {}: {e}", exe.display()));
        let stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let ready = read_json(&mut stdout);
        assert_eq!(ready["event"], "ready");
        Self {
            child,
            stdin,
            stdout,
            ready,
        }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    fn send(&mut self, cmd: &str) -> Json {
        writeln!(self.stdin, "{cmd}").unwrap();
        self.stdin.flush().unwrap();
        read_json(&mut self.stdout)
    }

    fn addr(&self, field: &str) -> u64 {
        hex(&self.ready["addr"][field])
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Testbed {
    fn drop(&mut self) {
        self.kill();
    }
}

fn read_json(r: &mut impl BufRead) -> Json {
    let mut line = String::new();
    r.read_line(&mut line).expect("read testbed output");
    serde_json::from_str(line.trim()).unwrap_or_else(|e| panic!("bad testbed JSON {line:?}: {e}"))
}

fn hex(v: &Json) -> u64 {
    let s = v.as_str().expect("hex string");
    u64::from_str_radix(s.trim_start_matches("0x"), 16).expect("hex value")
}

/// A scratch folder removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ot-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }

    /// Copies the x64 testbed into this folder under `rel`.
    fn copy_testbed(&self, rel: &str) -> PathBuf {
        let dest = self.0.join(rel);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::copy(testbed_exe(Flavor::X64), &dest).unwrap();
        dest
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn blocked(err: Error) -> ot_core::Verdict {
    match err {
        Error::Blocked(v) => v,
        other => panic!("expected Blocked, got {other:?}"),
    }
}

fn read_write_roundtrip(flavor: Flavor) {
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(flavor));
    assert_eq!(tb.ready["arch"], flavor.arch().to_string());

    // Process list sees it with the right architecture.
    let list = ot_core::list_processes().unwrap();
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
    tb.kill();
    assert!(!session.is_alive());
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
fn guard_blocks_game_with_anticheat_folder() {
    let _g = serial();
    let dir = TempDir::new("battleye-marker");
    std::fs::create_dir_all(dir.0.join("MyGame").join("BattlEye")).unwrap();
    let tb = Testbed::spawn(&dir.copy_testbed(r"MyGame\testbed_native.exe"));

    let v = blocked(AttachedProcess::attach(tb.pid()).unwrap_err());
    assert!(
        v.findings
            .iter()
            .any(|f| f.kind == FindingKind::InstallMarker && f.product == "BattlEye")
    );
}

#[test]
fn guard_blocks_marker_above_unreal_style_binaries_folder() {
    let _g = serial();
    let dir = TempDir::new("eac-unreal-layout");
    std::fs::create_dir_all(dir.0.join("Game").join("EasyAntiCheat")).unwrap();
    let tb = Testbed::spawn(&dir.copy_testbed(r"Game\Binaries\Win64\testbed_native.exe"));

    let v = blocked(AttachedProcess::attach(tb.pid()).unwrap_err());
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
