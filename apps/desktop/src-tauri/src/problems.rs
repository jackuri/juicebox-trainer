//! Problems the player should hear about, in plain words (v1.22.0).
//!
//! When a game closes, a web game page crashes or freezes, OpenTrainer's own
//! screen stops, or OpenTrainer itself fails, a short report is kept in
//! `%LOCALAPPDATA%\OpenTrainer\problems.json` (the last [`KEEP`]) and the screen
//! is told (`problem` event). Each report says what happened, the likely
//! reason and what to try. Nothing is sent anywhere.
//!
//! For a game that crashed, Windows' own crash record (the "Application Error"
//! event in the Application log) says in which part of the game it crashed,
//! for example the game itself or the graphics driver.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

/// Reports kept.
pub const KEEP: usize = 30;

/// One problem, as the Help page shows it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    /// Unique (milliseconds since 1970 when it was recorded, made unique).
    pub id: u64,
    /// `gameClosed`, `webPage`, `appScreen` or `appCrash`.
    pub kind: String,
    /// The game or page it's about, if any.
    pub game: Option<String>,
    /// One line: what happened.
    pub title: String,
    /// Why it most likely happened, in plain words.
    pub why: String,
    /// What to try, most useful first.
    pub advice: Vec<String>,
    /// Technical facts for anyone helping (exit code, crash place, cheats on).
    pub details: Vec<String>,
    /// For a game that closed: whether it crashed (not a normal close).
    #[serde(default)]
    pub crashed: bool,
    /// The player has looked at it on the Help page.
    #[serde(default)]
    pub seen: bool,
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static FILE: Mutex<()> = Mutex::new(());

/// Lets reports reach the screen. Called once at startup.
pub fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
}

/// `%LOCALAPPDATA%\OpenTrainer`.
pub fn default_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("OpenTrainer"))
}

fn file_in(dir: &Path) -> PathBuf {
    dir.join("problems.json")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The reports in `dir`, newest first (empty if none or unreadable).
pub fn list_in(dir: &Path) -> Vec<Problem> {
    std::fs::read(file_in(dir))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_in(dir: &Path, list: &[Problem]) {
    let _ = std::fs::create_dir_all(dir);
    if let Ok(json) = serde_json::to_vec_pretty(list) {
        let tmp = dir.join("problems.json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, file_in(dir));
        }
    }
}

/// Adds `p` to the reports in `dir` (newest first, at most [`KEEP`]) and
/// returns it with its id.
pub fn add_in(dir: &Path, mut p: Problem) -> Problem {
    let _guard = FILE.lock().unwrap_or_else(|e| e.into_inner());
    let mut list = list_in(dir);
    let newest = list.iter().map(|q| q.id).max().unwrap_or(0);
    p.id = now_ms().max(newest + 1);
    list.insert(0, p.clone());
    list.truncate(KEEP);
    save_in(dir, &list);
    p
}

/// Changes the report `id` in `dir`; returns the changed report.
pub fn change_in(dir: &Path, id: u64, f: impl FnOnce(&mut Problem)) -> Option<Problem> {
    let _guard = FILE.lock().unwrap_or_else(|e| e.into_inner());
    let mut list = list_in(dir);
    let p = list.iter_mut().find(|p| p.id == id)?;
    f(p);
    let changed = p.clone();
    save_in(dir, &list);
    Some(changed)
}

fn emit(p: &Problem) {
    if let Some(app) = APP.get() {
        let _ = app.emit("problem", p);
    }
}

/// Keeps `p` and tells the screen. Returns it with its id.
pub fn record(p: Problem) -> Problem {
    let p = match default_dir() {
        Some(dir) => add_in(&dir, p),
        None => p,
    };
    emit(&p);
    p
}

/// Changes a kept report and tells the screen.
pub fn change(id: u64, f: impl FnOnce(&mut Problem)) {
    if let Some(p) = default_dir().and_then(|d| change_in(&d, id, f)) {
        emit(&p);
    }
}

/// The kept reports, newest first.
#[tauri::command]
pub fn problems_list() -> Vec<Problem> {
    default_dir().map(|d| list_in(&d)).unwrap_or_default()
}

/// Marks every report as looked at.
#[tauri::command]
pub fn problems_seen() {
    if let Some(dir) = default_dir() {
        let _guard = FILE.lock().unwrap_or_else(|e| e.into_inner());
        let mut list = list_in(&dir);
        if list.iter().any(|p| !p.seen) {
            list.iter_mut().for_each(|p| p.seen = true);
            save_in(&dir, &list);
        }
    }
}

/// Forgets every report.
#[tauri::command]
pub fn problems_clear() {
    if let Some(dir) = default_dir() {
        let _guard = FILE.lock().unwrap_or_else(|e| e.into_inner());
        save_in(&dir, &[]);
    }
}

// ----- how a game ended ---------------------------------------------------------------------------

/// What a process's exit code says, in plain words: (crashed, why).
pub fn explain_exit(code: Option<u32>) -> (bool, String) {
    let Some(code) = code else {
        return (false, "Windows didn't say how it ended.".to_string());
    };
    let crash = |what: &str| {
        (
            true,
            format!("It crashed: {what} (error code 0x{code:08X})."),
        )
    };
    match code {
        0 => (
            false,
            "It closed normally: it was quit, or the game ended itself.".to_string(),
        ),
        0xC000_013A => (
            false,
            "It was closed from its window or with Ctrl+C.".to_string(),
        ),
        1 => (
            false,
            "It ended with error code 1. That usually means it was ended from Task Manager, or the game quit after its own error message.".to_string(),
        ),
        0xC000_0005 => crash("it tried to use memory it couldn't (an access violation)"),
        0xC000_0409 => crash("it found a serious error in itself and stopped on purpose"),
        0xC000_00FD => crash("it ran out of room for its work (a stack overflow)"),
        0xC000_0374 => crash("its memory got damaged (heap corruption)"),
        0xC000_001D => crash("it ran an instruction the processor doesn't know"),
        0xC000_0094 | 0xC000_008E => crash("it divided by zero"),
        0xC000_0142 => crash("one of its parts failed to start"),
        0xE06D_7363 => crash("a program error it didn't handle (C++ exception)"),
        0xE043_4352 => crash("a program error it didn't handle (.NET exception)"),
        0x8000_0003 => crash("it hit a debugging stop"),
        c if c & 0xC000_0000 == 0xC000_0000 => crash("Windows ended it after an error"),
        c => (
            false,
            format!("It ended by itself with error code {c}."),
        ),
    }
}

/// Windows' crash record for a process: in which file it crashed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashRecord {
    pub module: String,
    pub code: String,
    pub offset: String,
}

/// Reads `<Data Name='…'>value</Data>` from an event's XML.
fn data<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    for quote in ['\'', '"'] {
        let open = format!("<Data Name={quote}{name}{quote}>");
        if let Some(start) = xml.find(&open).map(|i| i + open.len()) {
            let end = xml[start..].find("</Data>")? + start;
            return Some(xml[start..end].trim());
        }
    }
    None
}

/// The crash record in an "Application Error" event's XML, if it is about `pid`.
pub fn crash_from_xml(xml: &str, pid: u32) -> Option<CrashRecord> {
    let raw = data(xml, "ProcessId")?;
    let event_pid = match raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => raw.parse().ok()?,
    };
    if event_pid != pid {
        return None;
    }
    Some(CrashRecord {
        module: data(xml, "ModuleName").unwrap_or("unknown").to_string(),
        code: data(xml, "ExceptionCode").unwrap_or("").to_string(),
        offset: data(xml, "FaultingOffset").unwrap_or("").to_string(),
    })
}

/// Windows' crash record for `pid` from the last few minutes, if it wrote one.
pub fn crash_record(pid: u32) -> Option<CrashRecord> {
    use windows::Win32::System::EventLog::{
        EVT_HANDLE, EvtClose, EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryReverseDirection,
        EvtRender, EvtRenderEventXml,
    };
    use windows::core::{HSTRING, PCWSTR};

    let query = HSTRING::from(
        "*[System[Provider[@Name='Application Error'] and (EventID=1000) and TimeCreated[timediff(@SystemTime) <= 600000]]]",
    );
    let channel = HSTRING::from("Application");
    // SAFETY: standard Event Log query: every handle made here is closed
    // below, and buffers are sized from what EvtRender reports.
    unsafe {
        let results = EvtQuery(
            None,
            PCWSTR(channel.as_ptr()),
            PCWSTR(query.as_ptr()),
            EvtQueryChannelPath.0 | EvtQueryReverseDirection.0,
        )
        .ok()?;
        let mut found = None;
        'outer: loop {
            let mut events = [0isize; 16];
            let mut returned = 0u32;
            if EvtNext(results, &mut events, 0, 0, &raw mut returned).is_err() {
                break;
            }
            for &raw_event in events.iter().take(returned as usize) {
                let event = EVT_HANDLE(raw_event);
                let mut used = 0u32;
                let mut count = 0u32;
                let _ = EvtRender(
                    None,
                    event,
                    EvtRenderEventXml.0,
                    0,
                    None,
                    &raw mut used,
                    &raw mut count,
                );
                let mut buf = vec![0u16; (used as usize).div_ceil(2) + 1];
                let xml = if EvtRender(
                    None,
                    event,
                    EvtRenderEventXml.0,
                    (buf.len() * 2) as u32,
                    Some(buf.as_mut_ptr().cast()),
                    &raw mut used,
                    &raw mut count,
                )
                .is_ok()
                {
                    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                    String::from_utf16_lossy(&buf[..len])
                } else {
                    String::new()
                };
                let _ = EvtClose(event);
                if found.is_none() {
                    found = crash_from_xml(&xml, pid);
                }
            }
            if found.is_some() || returned == 0 {
                break 'outer;
            }
        }
        let _ = EvtClose(results);
        found
    }
}

/// "3 seconds before", or "just before" for under a second.
fn ago(secs: u64) -> String {
    match secs {
        0 => "just before".into(),
        1 => "1 second before".into(),
        n => format!("{n} seconds before"),
    }
}

/// Files that belong to graphics drivers or DirectX / Vulkan.
fn is_graphics(module: &str) -> bool {
    let m = module.to_ascii_lowercase();
    [
        "nvwgf2um",
        "nvd3dum",
        "nvoglv",
        "nvlddmkm",
        "nvgpucomp",
        "nvcuda",
        "atiumd",
        "atidxx",
        "amdxx",
        "amdvlk",
        "atio6axx",
        "aticfx",
        "igd10",
        "igd11",
        "igd12",
        "igdumd",
        "igxelp",
        "igc64",
        "intelocl",
        "d3d9.dll",
        "d3d10",
        "d3d11.dll",
        "d3d12",
        "dxgi.dll",
        "vulkan-1",
        "opengl32",
    ]
    .iter()
    .any(|g| m.starts_with(g))
}

/// A cheat or locked value that was on, and whether it changes the game's code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnCheat {
    pub name: String,
    pub changes_code: bool,
}

/// The last change the player made before the game ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastChange {
    /// "Infinite ammo was turned on", "Gold was set to 999".
    pub what: String,
    pub seconds_before: u64,
    pub changes_code: bool,
}

/// Builds the report for a game that ended. `record` is Windows' crash record
/// when there is one (it may come a few seconds later).
pub fn game_closed(
    game: &str,
    exe: &str,
    code: Option<u32>,
    on: &[OnCheat],
    last: Option<&LastChange>,
    record: Option<&CrashRecord>,
) -> Problem {
    let (crashed, how) = explain_exit(code);
    let crashed = crashed || record.is_some();
    let mut why = how;
    let mut advice = Vec::new();
    let names: Vec<&str> = on.iter().map(|c| c.name.as_str()).collect();
    let recent = last.filter(|l| l.seconds_before <= 30);
    if crashed {
        let module = record.map(|r| r.module.as_str()).unwrap_or("");
        let in_game = !module.is_empty() && module.eq_ignore_ascii_case(exe);
        if is_graphics(module) {
            why.push_str(&format!(
                " It crashed in the graphics driver ({module}), not in a cheat."
            ));
            advice.push("Update your graphics driver (NVIDIA app, AMD Software or Intel's driver tool), then restart your PC.".into());
            advice.push("Lower the game's graphics settings or turn off overlays (Discord, Steam, recording apps).".into());
        } else if let Some(l) = recent {
            why.push_str(&format!(
                " {} {}, so that may have caused it.",
                l.what,
                ago(l.seconds_before)
            ));
            advice.push("Start the game again and try it without that cheat. If the game is fine without it, the cheat needs fixing.".into());
            if l.changes_code {
                advice.push("That cheat changes the game's code. If the game was updated, click Repair on the Cheats screen, or ask the AI helper to make it again.".into());
            } else {
                advice.push("If it's a number you set, try a smaller one (some games crash on very big numbers).".into());
            }
        } else if !names.is_empty() {
            let place = if in_game {
                format!(" It crashed inside the game's own code ({module}),")
            } else if !module.is_empty() {
                format!(" It crashed in {module},")
            } else {
                " It crashed".to_string()
            };
            why.push_str(&format!(
                "{place} while these cheats were on: {}. A cheat may be involved.",
                names.join(", ")
            ));
            advice.push("Start the game again and play a while without cheats. If it doesn't crash then, turn them on one at a time to find the one causing it.".into());
        } else {
            if !module.is_empty() {
                why.push_str(&format!(" It crashed in {module}."));
            }
            why.push_str(" No cheats were on, so this wasn't caused by OpenTrainer.");
            advice.push("Check the game's files (Steam: right-click the game, Properties, Installed Files, Verify integrity).".into());
            advice.push("Make sure the game and your graphics driver are up to date.".into());
        }
    } else if code == Some(0) || code == Some(0xC000_013A) {
        advice.push(
            "Nothing went wrong. Start the game again and click Play in OpenTrainer to use cheats."
                .into(),
        );
    } else {
        if let Some(l) = recent {
            why.push_str(&format!(" {} {}.", l.what, ago(l.seconds_before)));
        }
        advice.push(
            "If you didn't close it, start it again. If it keeps happening, try without cheats."
                .into(),
        );
    }
    let mut details = Vec::new();
    match code {
        Some(c) => details.push(format!("Exit code: 0x{c:08X} ({c})")),
        None => details.push("Exit code: unknown".into()),
    }
    if let Some(r) = record {
        details.push(format!(
            "Windows crash record: {} crashed in {} (exception {}, offset {})",
            exe, r.module, r.code, r.offset
        ));
    }
    if names.is_empty() {
        details.push("Cheats on: none".into());
    } else {
        details.push(format!("Cheats on: {}", names.join(", ")));
    }
    if let Some(l) = last {
        details.push(format!(
            "Last change: {} ({} s before)",
            l.what, l.seconds_before
        ));
    }
    Problem {
        kind: "gameClosed".into(),
        game: Some(game.to_string()),
        title: if crashed {
            format!("{game} crashed")
        } else {
            format!("{game} closed")
        },
        why,
        advice,
        details,
        crashed,
        // A normal close isn't news: no red dot or banner, only the note on
        // the Cheats screen and the line on the Help page.
        seen: !crashed && code.is_some(),
        ..Problem::default()
    }
}

/// Records a game that ended and, if it crashed, looks for Windows' crash
/// record a few seconds later to say where (the report is updated).
pub fn report_game_closed(
    pid: u32,
    game: String,
    exe: String,
    code: Option<u32>,
    on: Vec<OnCheat>,
    last: Option<LastChange>,
) -> Problem {
    let p = record(game_closed(&game, &exe, code, &on, last.as_ref(), None));
    let (crashed, _) = explain_exit(code);
    if crashed || code.is_none() {
        let id = p.id;
        std::thread::spawn(move || {
            // Windows writes the record a moment after the crash.
            for wait in [2, 4, 8] {
                std::thread::sleep(Duration::from_secs(wait));
                if let Some(r) = crash_record(pid) {
                    let better = game_closed(&game, &exe, code, &on, last.as_ref(), Some(&r));
                    change(id, |p| {
                        p.title = better.title;
                        p.why = better.why;
                        p.advice = better.advice;
                        p.details = better.details;
                        p.crashed = better.crashed;
                        p.seen = p.seen && better.seen;
                    });
                    return;
                }
            }
        });
    }
    p
}

// ----- pages and OpenTrainer itself ---------------------------------------------------------------

/// The report for a web game page that crashed or froze. `last` is the last
/// change made in the page (by the player or the AI helper) and how long ago.
pub fn web_page(
    game: &str,
    failure: crate::page_watch::PageFailure,
    last: Option<(&str, u64)>,
) -> Problem {
    use crate::page_watch::PageFailure as F;
    let recent = last.filter(|(_, s)| *s <= 30);
    let (title, mut why, mut advice): (String, String, Vec<String>) = match failure {
        F::Crashed { out_of_memory: true } => (
            format!("The {game} page crashed"),
            "It ran out of memory.".into(),
            vec![
                "Click Reload game to start it again.".into(),
                "Close other programs and browser tabs you don't need, so the game has more memory.".into(),
            ],
        ),
        F::Crashed { .. } | F::PartCrashed => (
            format!("The {game} page crashed"),
            "The part of the browser that runs the game stopped.".into(),
            vec!["Click Reload game to start it again.".into()],
        ),
        F::Frozen => (
            format!("The {game} page stopped responding"),
            "The game page is frozen: it isn't answering clicks or keys.".into(),
            vec![
                "Wait a few seconds: big games sometimes come back by themselves.".into(),
                "If it doesn't, click Reload game.".into(),
            ],
        ),
        F::EngineGone => (
            format!("The {game} window stopped working"),
            "The browser part of Windows that shows the game (WebView2) closed. That can happen when Windows updates it while it's running.".into(),
            vec!["Close the game window and open the game again with Play. If that doesn't work, restart OpenTrainer.".into()],
        ),
    };
    match recent {
        Some((what, secs)) if what.starts_with("A value search") => {
            why.push_str(&format!(
                " {what} {}. Searching a big game's memory can make the page pause.",
                ago(secs)
            ));
            advice.push("When searching, wait for the search to finish before playing.".into());
        }
        Some((what, secs)) => {
            why.push_str(&format!(
                " {what} {}, so that may have caused it.",
                ago(secs)
            ));
            advice.push("After reloading, try the game without that cheat. If it's fine without it, the cheat needs changing (a smaller number, or a different value).".into());
        }
        None if failure != F::EngineGone => {
            why.push_str(" Nothing had been changed in the game just before, so a cheat probably didn't cause it.");
        }
        None => {}
    }
    advice.push("If it keeps happening, check your internet connection: some web games freeze when it drops.".into());
    let mut details = vec![format!("Page problem: {failure:?}")];
    if let Some((what, secs)) = last {
        details.push(format!("Last change: {what} ({secs} s before)"));
    }
    Problem {
        kind: "webPage".into(),
        game: Some(game.to_string()),
        title,
        why,
        advice,
        details,
        crashed: !matches!(failure, F::Frozen),
        ..Problem::default()
    }
}

/// The report for OpenTrainer's own screen crashing or freezing (it is
/// reloaded right after).
pub fn app_screen(failure: crate::page_watch::PageFailure) -> Problem {
    use crate::page_watch::PageFailure as F;
    let why = match failure {
        F::Frozen => {
            "OpenTrainer's screen stopped responding, so it was restarted. Your games and cheats weren't touched."
        }
        F::EngineGone => {
            "The browser part of Windows that draws OpenTrainer's screen (WebView2) closed. That can happen when Windows updates it while OpenTrainer is open."
        }
        _ => {
            "OpenTrainer's screen crashed, so it was restarted. Your games and cheats weren't touched."
        }
    };
    let advice = if failure == F::EngineGone {
        vec!["Close OpenTrainer and open it again.".to_string()]
    } else {
        vec![
            "If cheats were on, check they still are on the Cheats screen.".to_string(),
            "If the screen goes blank again, close OpenTrainer and open it again.".to_string(),
        ]
    };
    Problem {
        kind: "appScreen".into(),
        title: "OpenTrainer's screen had a problem".into(),
        why: why.into(),
        advice,
        details: vec![format!("Screen problem: {failure:?}")],
        crashed: failure != F::Frozen,
        ..Problem::default()
    }
}

/// The report for a bug in OpenTrainer itself (a Rust panic). Written
/// straight to disk: OpenTrainer may be closing.
pub fn app_crash(message: &str) {
    if let Some(dir) = default_dir() {
        add_in(
            &dir,
            Problem {
                kind: "appCrash".into(),
                title: "OpenTrainer ran into a bug".into(),
                why: "Something inside OpenTrainer went wrong. If it closed, it first put your games' original code back.".into(),
                advice: vec![
                    "Start OpenTrainer again. Your packs, saves and settings are safe.".into(),
                    "If it keeps happening, copy the details below and report it on OpenTrainer's GitHub page (Issues).".into(),
                ],
                details: vec![message.chars().take(600).collect()],
                crashed: true,
                ..Problem::default()
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_problems_in_plain_words() {
        use crate::page_watch::PageFailure as F;
        let p = web_page(
            "Outbreak",
            F::Frozen,
            Some(("A value was locked at 999", 2)),
        );
        assert_eq!(p.title, "The Outbreak page stopped responding");
        assert!(p.why.contains("locked at 999 2 seconds before"));
        assert!(!p.crashed);
        let p = web_page(
            "Outbreak",
            F::Crashed {
                out_of_memory: true,
            },
            None,
        );
        assert!(p.why.contains("out of memory") && p.why.contains("probably didn't"));
        let p = web_page(
            "Outbreak",
            F::Frozen,
            Some(("A value search was running", 1)),
        );
        assert!(p.why.contains("pause"));
        assert!(app_screen(F::EngineGone).why.contains("WebView2"));
    }

    const XML: &str = "<Event><EventData><Data Name='AppName'>game.exe</Data><Data Name='ModuleName'>nvwgf2umx.dll</Data><Data Name='ExceptionCode'>c0000005</Data><Data Name='FaultingOffset'>00000000001a2b3c</Data><Data Name='ProcessId'>0x72c8</Data></EventData></Event>";

    #[test]
    fn exit_codes_in_plain_words() {
        assert!(!explain_exit(Some(0)).0);
        assert!(explain_exit(Some(0)).1.contains("normally"));
        let (crashed, why) = explain_exit(Some(0xC000_0005));
        assert!(crashed && why.contains("access violation") && why.contains("0xC0000005"));
        assert!(explain_exit(Some(0xC0FF_FFFF)).0);
        assert!(!explain_exit(Some(1)).0);
        assert!(!explain_exit(None).0);
    }

    /// Reads a real record: crash a program (with Windows crash reporting
    /// on), then run with `OT_CRASH_PID=<its pid>` and `--ignored`.
    #[test]
    #[ignore = "needs a crash from the last 10 minutes"]
    fn finds_a_real_crash_record() {
        let pid: u32 = std::env::var("OT_CRASH_PID").unwrap().parse().unwrap();
        let r = crash_record(pid).expect("no crash record for that pid");
        assert_eq!(r.code, "c0000005");
        assert!(!r.module.is_empty());
        assert!(crash_record(1).is_none());
    }

    #[test]
    fn reads_windows_crash_records() {
        let r = crash_from_xml(XML, 0x72c8).unwrap();
        assert_eq!(r.module, "nvwgf2umx.dll");
        assert_eq!(r.code, "c0000005");
        assert!(crash_from_xml(XML, 1234).is_none());
        let quoted = XML.replace('\'', "\"").replace("0x72c8", "29384");
        assert_eq!(
            crash_from_xml(&quoted, 29384).unwrap().offset,
            "00000000001a2b3c"
        );
    }

    #[test]
    fn explains_why_a_game_closed() {
        let on = [OnCheat {
            name: "Infinite ammo".into(),
            changes_code: true,
        }];
        let last = LastChange {
            what: "Infinite ammo was turned on".into(),
            seconds_before: 3,
            changes_code: true,
        };
        // A crash just after turning a cheat on points at it.
        let p = game_closed(
            "Coin Quest",
            "coin.exe",
            Some(0xC000_0005),
            &on,
            Some(&last),
            None,
        );
        assert!(p.crashed && p.title == "Coin Quest crashed");
        assert!(
            p.why
                .contains("Infinite ammo was turned on 3 seconds before")
        );
        assert!(p.advice.iter().any(|a| a.contains("Repair")));
        // The graphics driver is not a cheat.
        let gpu = CrashRecord {
            module: "nvwgf2umx.dll".into(),
            code: "c0000005".into(),
            offset: "1".into(),
        };
        let p = game_closed(
            "Coin Quest",
            "coin.exe",
            Some(0xC000_0005),
            &on,
            Some(&last),
            Some(&gpu),
        );
        assert!(p.why.contains("graphics driver"));
        // No cheats: not OpenTrainer.
        let p = game_closed("Coin Quest", "coin.exe", Some(0xC000_0409), &[], None, None);
        assert!(p.why.contains("wasn't caused by OpenTrainer"));
        // A normal close.
        let p = game_closed("Coin Quest", "coin.exe", Some(0), &on, None, None);
        assert!(!p.crashed && p.title == "Coin Quest closed");
        assert!(p.seen, "a normal close needs no attention");
        // Under a second: "just before".
        let now = LastChange {
            seconds_before: 0,
            ..last.clone()
        };
        let p = game_closed(
            "Coin Quest",
            "coin.exe",
            Some(0xC000_0005),
            &on,
            Some(&now),
            None,
        );
        assert!(
            p.why.contains("Infinite ammo was turned on just before"),
            "{}",
            p.why
        );
        assert!(p.details.iter().any(|d| d.contains("Infinite ammo")));
    }

    #[test]
    fn keeps_the_last_reports_newest_first() {
        let dir = std::env::temp_dir().join(format!("ot-problems-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for i in 0..(KEEP + 3) {
            add_in(
                &dir,
                Problem {
                    title: format!("p{i}"),
                    ..Problem::default()
                },
            );
        }
        let list = list_in(&dir);
        assert_eq!(list.len(), KEEP);
        assert_eq!(list[0].title, format!("p{}", KEEP + 2));
        assert!(list.windows(2).all(|w| w[0].id > w[1].id));
        let id = list[1].id;
        let changed = change_in(&dir, id, |p| p.why = "because".into()).unwrap();
        assert_eq!(changed.why, "because");
        assert_eq!(list_in(&dir)[1].why, "because");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
