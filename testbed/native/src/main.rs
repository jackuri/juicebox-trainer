//! `testbed_native` — a tiny fake "game" that OpenTrainer attaches to in tests.
//!
//! It keeps player stats on the heap behind a pointer chain:
//!
//! ```text
//! testbed_native.exe+ROOT  ->  World  ->  Player  ->  Stats { health, max_health, ammo, gold, speed }
//! ```
//!
//! Commands are read line by line from stdin. With `--json` every reply is a
//! single JSON line, which is the side channel integration tests use to check
//! values after OpenTrainer writes them. Build with `--features shifted` to get
//! a layout with different offsets.
//!
//! Four small pieces of code are written in inline assembly so their bytes are
//! the same on every build, compiler version and architecture (x64 and x86).
//! Each starts with a marker `nop dword ptr [rax+0x0N544F54]` (bytes
//! `0F 1F 80 54 4F 54 0N`), which cheat packs use as an AOB signature:
//!
//! | N | Site | Bytes after the marker |
//! |---|------|------------------------|
//! | 1 | damage: `health -= amount` | `8B 01 29 D0 89 01` (`mov eax,[rcx]; sub eax,edx; mov [rcx],eax`) |
//! | 2 | fire: `ammo -= shots` | `8B 01 29 D0 89 01` |
//! | 3 | load the static root | x64 `48 8B 0D rel32` (`mov rcx,[rip+WORLD]`), x86 `8B 0D abs32` |
//! | 4 | earn: `gold += amount` | `8B 01 01 D0 89 01` (`mov eax,[rcx]; add eax,edx; mov [rcx],eax`) |
//!
//! `save` writes the stats to `AppData\LocalLow\TestCo\Testbed\save1.json` and
//! `load` reads them back, like a Unity game (it also writes
//! `<exe>_Datapp.info`), so the save manager has something to back up.

use std::arch::asm;
use std::io::{self, BufRead, Write};
use std::mem::offset_of;
use std::path::PathBuf;
use std::ptr::{self, addr_of, addr_of_mut};
use std::sync::atomic::{AtomicPtr, Ordering};

const MAGIC: [u8; 8] = *b"OTTESTBD";
const MAX_AMMO: i32 = 30;

#[repr(C)]
struct Stats {
    #[cfg(feature = "shifted")]
    _pad0: [u8; 24],
    health: i32,
    max_health: i32,
    ammo: i32,
    #[cfg(feature = "shifted")]
    _pad1: [u8; 8],
    gold: i32,
    speed: f32,
}

#[repr(C)]
struct Player {
    name: [u8; 16],
    #[cfg(feature = "shifted")]
    _pad: [u8; 16],
    stats: *mut Stats,
}

#[repr(C)]
struct World {
    magic: [u8; 8],
    tick: u64,
    player: *mut Player,
}

/// Static root of the pointer chain. Lives in the exe image, so its address is
/// `module base + fixed offset`.
static WORLD: AtomicPtr<World> = AtomicPtr::new(ptr::null_mut());

unsafe extern "C" {
    /// Provided by the MSVC linker: the base address of this image.
    static __ImageBase: u8;
}

fn image_base() -> usize {
    // Taking the address of an extern static is safe; we never read through it.
    addr_of!(__ImageBase) as usize
}

fn new_world() -> *mut World {
    let stats = Box::into_raw(Box::new(Stats {
        #[cfg(feature = "shifted")]
        _pad0: [0; 24],
        health: 100,
        max_health: 100,
        ammo: MAX_AMMO,
        #[cfg(feature = "shifted")]
        _pad1: [0; 8],
        gold: 250,
        speed: 1.0,
    }));
    let mut name = [0u8; 16];
    name[..6].copy_from_slice(b"Tester");
    let player = Box::into_raw(Box::new(Player {
        name,
        #[cfg(feature = "shifted")]
        _pad: [0; 16],
        stats,
    }));
    Box::into_raw(Box::new(World {
        magic: MAGIC,
        tick: 0,
        player,
    }))
}

// All game-state access goes through volatile reads/writes because another
// process (OpenTrainer) changes these values behind the compiler's back.

/// Loads the static root with code at a fixed byte pattern (marker 3).
#[inline(never)]
fn world() -> *mut World {
    let w: *mut World;
    // SAFETY: reads the WORLD static (a pointer-sized atomic) into a register;
    // the marker is a NOP. No stack use, no other memory touched.
    unsafe {
        #[cfg(target_arch = "x86_64")]
        asm!(
            "nop dword ptr [rax + 0x03544F54]",
            "mov rcx, qword ptr [rip + {w}]",
            w = sym WORLD,
            out("rcx") w,
            options(nostack, readonly),
        );
        #[cfg(target_arch = "x86")]
        asm!(
            "nop dword ptr [eax + 0x03544F54]",
            "mov ecx, dword ptr [{w}]",
            w = sym WORLD,
            out("ecx") w,
            options(nostack, readonly),
        );
    }
    w
}

/// `*field -= amount` (`sub`) or `*field += amount` (`add`) at a fixed byte
/// pattern. Marker 1 = damage, 2 = fire, 4 = earn.
macro_rules! asm_op {
    ($op:literal, $marker:literal, $field:expr, $amount:expr) => {{
        let field: *mut i32 = $field;
        let amount: i32 = $amount;
        // SAFETY: `field` points at a live i32 in the leaked Stats block. The
        // marker is a NOP; the rest is a plain load, subtract and store.
        unsafe {
            #[cfg(target_arch = "x86_64")]
            asm!(
                concat!("nop dword ptr [rax + ", $marker, "]"),
                "mov eax, dword ptr [rcx]",
                concat!($op, " eax, edx"),
                "mov dword ptr [rcx], eax",
                in("rcx") field,
                in("edx") amount,
                out("eax") _,
                options(nostack),
            );
            #[cfg(target_arch = "x86")]
            asm!(
                concat!("nop dword ptr [eax + ", $marker, "]"),
                "mov eax, dword ptr [ecx]",
                concat!($op, " eax, edx"),
                "mov dword ptr [ecx], eax",
                in("ecx") field,
                in("edx") amount,
                out("eax") _,
                options(nostack),
            );
        }
    }};
}

fn stats() -> *mut Stats {
    // SAFETY: WORLD is set once in main before any command runs and never freed.
    unsafe { ptr::read_volatile(addr_of!((*(*world()).player).stats)) }
}

macro_rules! get {
    ($field:ident) => {
        // SAFETY: stats() points at a leaked, live Stats allocation.
        unsafe { ptr::read_volatile(addr_of!((*stats()).$field)) }
    };
}

macro_rules! set {
    ($field:ident, $value:expr) => {{
        let v = $value;
        // SAFETY: stats() points at a leaked, live Stats allocation.
        unsafe { ptr::write_volatile(addr_of_mut!((*stats()).$field), v) }
    }};
}

#[inline(never)]
fn take_damage(amount: i32) {
    let amount = amount.clamp(0, get!(health).max(0));
    // SAFETY: only the field address is taken; stats() is live.
    let field = unsafe { addr_of_mut!((*stats()).health) };
    asm_op!("sub", "0x01544F54", field, amount);
}

#[inline(never)]
fn fire(shots: i32) -> i32 {
    let fired = shots.clamp(0, get!(ammo).max(0));
    // SAFETY: only the field address is taken; stats() is live.
    let field = unsafe { addr_of_mut!((*stats()).ammo) };
    asm_op!("sub", "0x02544F54", field, fired);
    fired
}

/// Gold pickups: `gold += amount` at marker 4 (wraps like the game would).
#[inline(never)]
fn earn(amount: i32) {
    // SAFETY: only the field address is taken; stats() is live.
    let field = unsafe { addr_of_mut!((*stats()).gold) };
    asm_op!("add", "0x04544F54", field, amount.max(0));
}

fn bump_tick() -> u64 {
    // SAFETY: world() is live for the whole program.
    unsafe {
        let p = addr_of_mut!((*world()).tick);
        let t = ptr::read_volatile(p) + 1;
        ptr::write_volatile(p, t);
        t
    }
}

fn module_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "testbed_native.exe".into())
}

fn state_json(event: &str, message: &str) -> String {
    let base = image_base();
    let s = stats();
    let w = world();
    // SAFETY: only addresses are taken; no memory is read here except `tick`.
    let (health_at, ammo_at, gold_at, speed_at, tick) = unsafe {
        (
            addr_of!((*s).health) as usize,
            addr_of!((*s).ammo) as usize,
            addr_of!((*s).gold) as usize,
            addr_of!((*s).speed) as usize,
            ptr::read_volatile(addr_of!((*w).tick)),
        )
    };
    let root_offset = addr_of!(WORLD) as usize - base;
    format!(
        concat!(
            "{{\"event\":\"{event}\",\"message\":\"{message}\",",
            "\"pid\":{pid},\"arch\":\"{arch}\",\"shifted\":{shifted},\"module\":\"{module}\",",
            "\"tick\":{tick},\"health\":{health},\"max_health\":{max_health},\"ammo\":{ammo},",
            "\"gold\":{gold},\"speed\":{speed},",
            "\"addr\":{{\"module_base\":\"0x{base:X}\",\"health\":\"0x{health_at:X}\",",
            "\"ammo\":\"0x{ammo_at:X}\",\"gold\":\"0x{gold_at:X}\",\"speed\":\"0x{speed_at:X}\"}},",
            "\"chain\":{{\"root\":\"{module}+0x{root_offset:X}\",\"offsets\":[",
            "\"0x{o_player:X}\",\"0x{o_stats:X}\"],\"fields\":{{\"health\":\"0x{o_health:X}\",",
            "\"ammo\":\"0x{o_ammo:X}\",\"gold\":\"0x{o_gold:X}\",\"speed\":\"0x{o_speed:X}\"}}}}}}"
        ),
        event = event,
        message = message.replace('\\', "\\\\").replace('"', "\\\""),
        pid = std::process::id(),
        arch = if cfg!(target_pointer_width = "64") {
            "x64"
        } else {
            "x86"
        },
        shifted = cfg!(feature = "shifted"),
        module = module_name(),
        tick = tick,
        health = get!(health),
        max_health = get!(max_health),
        ammo = get!(ammo),
        gold = get!(gold),
        speed = get!(speed),
        base = base,
        health_at = health_at,
        ammo_at = ammo_at,
        gold_at = gold_at,
        speed_at = speed_at,
        root_offset = root_offset,
        o_player = offset_of!(World, player),
        o_stats = offset_of!(Player, stats),
        o_health = offset_of!(Stats, health),
        o_ammo = offset_of!(Stats, ammo),
        o_gold = offset_of!(Stats, gold),
        o_speed = offset_of!(Stats, speed),
    )
}

fn human_state() -> String {
    format!(
        "  HP {}/{}   Ammo {}/{}   Gold {}   Speed {:.2}",
        get!(health),
        get!(max_health),
        get!(ammo),
        MAX_AMMO,
        get!(gold),
        get!(speed)
    )
}

const HELP: &str = "commands: state | hit [n] | heal | fire [n] | reload | earn [n] | spend [n] | save | load | tick | help | quit";

/// Where `save` writes, like a Unity game: `AppData\LocalLow\TestCo\Testbed`.
/// Tests point it elsewhere with `OT_TESTBED_SAVE_DIR`.
fn save_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("OT_TESTBED_SAVE_DIR") {
        return Some(PathBuf::from(dir));
    }
    let profile = std::env::var_os("USERPROFILE")?;
    Some(
        PathBuf::from(profile)
            .join("AppData")
            .join("LocalLow")
            .join("TestCo")
            .join("Testbed"),
    )
}

/// Writes `<exe>_Datapp.info` (company and product) like a Unity game, so
/// OpenTrainer's save locator finds the save folder.
fn write_app_info() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let (Some(dir), Some(stem)) = (exe.parent(), exe.file_stem()) else {
        return;
    };
    let data = dir.join(format!("{}_Data", stem.to_string_lossy()));
    let info = data.join("app.info");
    if !info.exists() && std::fs::create_dir_all(&data).is_ok() {
        let _ = std::fs::write(info, "TestCo\nTestbed\n");
    }
}

fn save_game() -> String {
    let Some(dir) = save_dir() else {
        return "can't find a save folder".into();
    };
    let text = format!(
        concat!(
            "{{\n",
            "  \"version\": 1,\n",
            "  \"player\": {{\n",
            "    \"name\": \"Tester\",\n",
            "    \"health\": {},\n",
            "    \"maxHealth\": {},\n",
            "    \"ammo\": {},\n",
            "    \"gold\": {},\n",
            "    \"speed\": {}\n",
            "  }}\n",
            "}}\n"
        ),
        get!(health),
        get!(max_health),
        get!(ammo),
        get!(gold),
        get!(speed)
    );
    let path = dir.join("save1.json");
    match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, text)) {
        Ok(()) => format!("saved to {}", path.display()),
        Err(e) => format!("save failed: {e}"),
    }
}

/// The number after `"key":` in the save text.
fn field(text: &str, key: &str) -> Option<f64> {
    let at = text.find(&format!("\"{key}\":"))? + key.len() + 3;
    let rest = text[at..].trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '-' || c == '.'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn load_game() -> String {
    let Some(path) = save_dir().map(|d| d.join("save1.json")) else {
        return "can't find a save folder".into();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return "no save yet (type save first)".into();
    };
    let int = |k| field(&text, k).map(|v| v as i32);
    match (
        int("health"),
        int("maxHealth"),
        int("ammo"),
        int("gold"),
        field(&text, "speed"),
    ) {
        (Some(h), Some(m), Some(a), Some(g), Some(s)) => {
            set!(health, h);
            set!(max_health, m);
            set!(ammo, a);
            set!(gold, g);
            set!(speed, s as f32);
            "loaded".into()
        }
        _ => "the save file is damaged".into(),
    }
}

/// Runs one command. Returns `None` to quit.
fn run(cmd: &str, arg: Option<i32>) -> Option<String> {
    let msg = match cmd {
        "" | "state" => "state".to_string(),
        "hit" => {
            take_damage(arg.unwrap_or(10));
            format!("took {} damage", arg.unwrap_or(10))
        }
        "heal" => {
            set!(health, get!(max_health));
            "healed".into()
        }
        "fire" => format!("fired {}", fire(arg.unwrap_or(1))),
        "reload" => {
            set!(ammo, MAX_AMMO);
            "reloaded".into()
        }
        "earn" => {
            earn(arg.unwrap_or(100));
            format!("earned {}", arg.unwrap_or(100))
        }
        "spend" => {
            let want = arg.unwrap_or(50);
            if get!(gold) >= want {
                set!(gold, get!(gold) - want);
                format!("spent {want}")
            } else {
                "not enough gold".into()
            }
        }
        "save" => save_game(),
        "load" => load_game(),
        "tick" => format!("tick {}", bump_tick()),
        "help" => HELP.into(),
        "quit" | "exit" => return None,
        other => format!("unknown command '{other}'. {HELP}"),
    };
    Some(msg)
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetErrorMode(mode: u32) -> u32;
}

fn main() {
    // Report crashes to Windows like a game started from Explorer or Steam,
    // even when the starting program turned crash reports off (Node does),
    // so tests see Windows' crash record (OpenTrainer 1.22's "why it closed").
    // Programs Node starts directly are also in its job object, which skips
    // the report: tests start the testbed through cmd for that.
    // SAFETY: plain Win32 call with no pointers.
    unsafe { SetErrorMode(0) };
    let json = std::env::args().any(|a| a == "--json");
    WORLD.store(new_world(), Ordering::Release);
    write_app_info();

    let stdout = io::stdout();
    let mut out = stdout.lock();
    if json {
        let _ = writeln!(out, "{}", state_json("ready", "ready"));
    } else {
        let _ = writeln!(
            out,
            "OpenTrainer testbed ({}, pid {}) — a fake game for testing. {HELP}",
            if cfg!(target_pointer_width = "64") {
                "x64"
            } else {
                "x86"
            },
            std::process::id()
        );
        let _ = writeln!(out, "{}", state_json("ready", "ready"));
        let _ = writeln!(out, "{}", human_state());
        let _ = write!(out, "> ");
    }
    let _ = out.flush();

    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        // PowerShell pipes may prepend a UTF-8 BOM.
        let mut parts = line.trim_start_matches('\u{feff}').split_whitespace();
        let cmd = parts.next().unwrap_or("").to_ascii_lowercase();
        let arg = parts.next().and_then(|a| a.parse::<i32>().ok());
        let Some(msg) = run(&cmd, arg) else { break };
        if json {
            let _ = writeln!(out, "{}", state_json("state", &msg));
        } else {
            let _ = writeln!(out, "{msg}\n{}\n> ", human_state());
        }
        let _ = out.flush();
    }
}
