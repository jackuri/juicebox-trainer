//! Shared helpers for integration tests against `testbed/native` (never a real game).
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

use ot_core::{Arch, Error};
use serde_json::Value as Json;

pub fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Flavor {
    X64,
    X86,
}

impl Flavor {
    pub fn arch(self) -> Arch {
        match self {
            Flavor::X64 => Arch::X64,
            Flavor::X86 => Arch::X86,
        }
    }
}

pub fn testbed_exe(flavor: Flavor) -> PathBuf {
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
#[derive(Debug)]
pub struct Testbed {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    pub ready: Json,
}

impl Testbed {
    pub fn spawn(exe: &Path) -> Self {
        Self::spawn_with_env(exe, &[])
    }

    /// Like [`Testbed::spawn`] with extra environment variables (e.g.
    /// `OT_TESTBED_SAVE_DIR`).
    pub fn spawn_with_env(exe: &Path, env: &[(&str, &Path)]) -> Self {
        let mut command = Command::new(exe);
        for (k, v) in env {
            command.env(k, v);
        }
        let mut child = command
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

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn send(&mut self, cmd: &str) -> Json {
        writeln!(self.stdin, "{cmd}").unwrap();
        self.stdin.flush().unwrap();
        read_json(&mut self.stdout)
    }

    pub fn addr(&self, field: &str) -> u64 {
        hex(&self.ready["addr"][field])
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Testbed {
    fn drop(&mut self) {
        self.kill();
    }
}

pub fn read_json(r: &mut impl BufRead) -> Json {
    let mut line = String::new();
    r.read_line(&mut line).expect("read testbed output");
    serde_json::from_str(line.trim()).unwrap_or_else(|e| panic!("bad testbed JSON {line:?}: {e}"))
}

pub fn hex(v: &Json) -> u64 {
    let s = v.as_str().expect("hex string");
    u64::from_str_radix(s.trim_start_matches("0x"), 16).expect("hex value")
}

/// A scratch folder under `target/tmp`, removed on drop.
#[derive(Debug)]
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        // Under target/, not %TEMP%: the guard looks at the exe folder and
        // three parents, and %LOCALAPPDATA% may hold a real anti-cheat
        // folder (e.g. BattlEye) on a developer's PC.
        let p = workspace_root()
            .join("target")
            .join("tmp")
            .join(format!("ot-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }

    /// Copies the x64 testbed into this folder under `rel`.
    pub fn copy_testbed(&self, rel: &str) -> PathBuf {
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

pub fn blocked(err: Error) -> ot_core::Verdict {
    match err {
        Error::Blocked(v) => v,
        other => panic!("expected Blocked, got {other:?}"),
    }
}
