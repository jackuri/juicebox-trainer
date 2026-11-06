//! Trainer integration tests: load packs against the live testbed (never a
//! real game), toggle cheats and check the results through its side channel.

use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use ot_core::AttachedProcess;
use ot_packs::trainer::Error;
use ot_packs::{BuildMatch, Pack, Trainer, parse, store};
use serde_json::{Value as Json, json};
use testbed_harness::*;

fn example(flavor: Flavor) -> Pack {
    let id = match flavor {
        Flavor::X64 => "bundled:testbed-native",
        Flavor::X86 => "bundled:testbed-native-x86",
    };
    let found = store::bundled().into_iter().find(|f| f.key == id).unwrap();
    parse(found.text.as_ref().unwrap()).unwrap().0
}

fn attach(tb: &Testbed) -> Arc<AttachedProcess> {
    Arc::new(AttachedProcess::attach(tb.pid()).expect("attach to testbed"))
}

fn state(tb: &mut Testbed) -> Json {
    tb.send("state")
}

fn status(t: &mut Trainer, id: &str) -> (String, Option<String>) {
    let s = t
        .states()
        .unwrap()
        .into_iter()
        .find(|s| s.id == id)
        .unwrap();
    (s.status.to_string(), s.message)
}

fn example_pack_works(flavor: Flavor) {
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(flavor));
    let process = attach(&tb);
    let mut t = Trainer::load(Arc::clone(&process), example(flavor)).unwrap();
    assert_eq!(t.build(), &BuildMatch::Untagged);
    for s in t.states().unwrap() {
        assert_eq!(s.status, "ready", "{} on {flavor:?}: {:?}", s.id, s.message);
    }

    // Patch: health no longer goes down; off restores the original bytes.
    t.set_active("inf_health", true).unwrap();
    assert_eq!(state(&mut tb)["health"], 100);
    assert_eq!(tb.send("hit 10")["health"], 100);
    let states = t.states().unwrap();
    let ih = states.iter().find(|s| s.id == "inf_health").unwrap();
    assert_eq!(ih.status, "active");
    assert!(ih.touches[0].contains("29 D0 → 90 90"), "{:?}", ih.touches);
    assert!(
        ih.touches[1].contains("sub eax,edx → nop; nop"),
        "{:?}",
        ih.touches
    );
    let site = ih.address.unwrap();
    assert_eq!(process.read_bytes(site, 2).unwrap(), [0x90, 0x90]);
    t.set_active("inf_health", false).unwrap();
    assert_eq!(process.read_bytes(site, 2).unwrap(), [0x29, 0xD0]);
    assert_eq!(tb.send("hit 10")["health"], 90);

    // Hotkey flips a toggle.
    t.hotkey("inf_ammo").unwrap();
    assert_eq!(tb.send("fire 3")["ammo"], 30);
    t.hotkey("inf_ammo").unwrap();
    assert_eq!(tb.send("fire 3")["ammo"], 27);

    // Value toggle: frozen at 999 against damage.
    t.set_active("super_health", true).unwrap();
    tb.send("hit 50");
    sleep(Duration::from_millis(200));
    assert_eq!(state(&mut tb)["health"], 999);
    t.set_active("super_health", false).unwrap();
    assert_eq!(tb.send("hit 9")["health"], 990);

    // Button, number with bounds, slider with hold.
    t.press("add_gold").unwrap();
    assert_eq!(state(&mut tb)["gold"], 1250);
    t.hotkey("add_gold").unwrap();
    assert_eq!(state(&mut tb)["gold"], 2250);
    t.set_value("set_gold", "777").unwrap();
    assert_eq!(state(&mut tb)["gold"], 777);
    assert!(matches!(
        t.set_value("set_gold", "-1"),
        Err(Error::OutOfRange { .. })
    ));
    assert!(t.set_value("set_gold", "lots").is_err());
    t.set_value("speed", "2.5").unwrap();
    assert_eq!(state(&mut tb)["speed"], 2.5);
    let speed = t
        .states()
        .unwrap()
        .into_iter()
        .find(|s| s.id == "speed")
        .unwrap();
    assert!(speed.active, "slider with hold is held");
    assert_eq!(speed.value.as_deref(), Some("2.5"));
    assert!(matches!(
        t.set_value("speed", "9"),
        Err(Error::OutOfRange { .. })
    ));
    t.release("speed").unwrap();
    assert!(matches!(t.press("speed"), Err(Error::WrongUi { .. })));
    assert!(matches!(
        t.set_active("nope", true),
        Err(Error::UnknownCheat(_))
    ));

    // Dropping the trainer restores patches that are still on.
    t.set_active("inf_ammo", true).unwrap();
    drop(t);
    assert_eq!(tb.send("fire 1")["ammo"], 26);
}

#[test]
fn example_pack_works_x64() {
    example_pack_works(Flavor::X64);
}

#[test]
fn example_pack_works_x86() {
    example_pack_works(Flavor::X86);
}

/// A pack built for this testbed run (its pointer root offset is known only
/// at runtime).
fn custom_pack(tb: &Testbed, cheats: Json, builds: Json) -> Pack {
    let arch = tb.ready["arch"].as_str().unwrap();
    let text = json!({
        "schema": 1,
        "game": { "id": "testbed-custom", "name": "Testbed", "exe": "testbed_native.exe",
                  "arch": arch, "builds": builds },
        "meta": { "author": "ai", "packVersion": "0.1.0" },
        "cheats": cheats
    })
    .to_string();
    parse(&text).unwrap_or_else(|e| panic!("{e}")).0
}

fn patch_cheat(id: &str, aob: &str, original: &str) -> Json {
    json!({ "id": id, "name": id, "ui": "toggle",
            "impl": { "kind": "patch", "aob": aob, "offset": 9, "original": original, "replace": "90 90" } })
}

#[test]
fn static_base_pointer_chain() {
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let chain = &tb.ready["chain"];
    let mut offsets: Vec<Json> = chain["offsets"].as_array().unwrap().clone();
    offsets.push(chain["fields"]["gold"].clone());
    let pack = custom_pack(
        &tb,
        json!([{ "id": "gold", "name": "Gold", "ui": "number",
                 "impl": { "kind": "value", "type": "i32",
                           "address": { "base": chain["root"], "offsets": offsets } } }]),
        json!([]),
    );
    let mut t = Trainer::load(attach(&tb), pack).unwrap();
    let s = &t.states().unwrap()[0];
    assert_eq!(s.value.as_deref(), Some("250"));
    assert_eq!(s.address, Some(tb.addr("gold")));
    t.set_value("gold", "4242").unwrap();
    assert_eq!(state(&mut tb)["gold"], 4242);
}

#[test]
fn broken_signatures_fail_cleanly() {
    let _g = serial();
    let tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let pack = custom_pack(
        &tb,
        json!([
            patch_cheat("drifted", "0F 1F 80 54 4F 54 01 8B 01 ?? ?? 89 01", "29 D1"),
            patch_cheat("missing", "0F 1F 80 54 4F 54 09 8B 01 ?? ?? 89 01", "29 D0"),
            patch_cheat("ambiguous", "8B 01 ?? ?? 89 01 ?? ?? ?? ?? ?? ?? ??", "29 D0"),
            { "id": "null_chain", "name": "n", "ui": "number",
              "impl": { "kind": "value", "type": "i32",
                        "address": { "base": "testbed_native.exe+0x0", "offsets": ["0x0", "0x0", "0x0"] } } }
        ]),
        json!([]),
    );
    let mut t = Trainer::load(attach(&tb), pack).unwrap();
    let (st, msg) = status(&mut t, "drifted");
    assert_eq!(st, "failed");
    assert!(msg.unwrap().contains("needs repair"));
    assert!(status(&mut t, "missing").1.unwrap().contains("not found"));
    assert!(
        status(&mut t, "ambiguous")
            .1
            .unwrap()
            .contains("more than once")
    );
    assert!(matches!(
        t.set_active("drifted", true),
        Err(Error::Unavailable(..))
    ));
    assert_eq!(status(&mut t, "null_chain").0, "failed");
    assert!(t.set_value("null_chain", "1").is_err());
}

#[test]
fn build_hash_matching() {
    let _g = serial();
    let tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let process = attach(&tb);
    let real = process.exe_sha256().unwrap();
    let cheat = json!([patch_cheat(
        "inf",
        "0F 1F 80 54 4F 54 01 8B 01 ?? ?? 89 01",
        "29 D0"
    )]);
    let verified = custom_pack(
        &tb,
        cheat.clone(),
        json!([{ "sha256": real, "label": "dev", "verified": true }]),
    );
    let t = Trainer::load(Arc::clone(&process), verified).unwrap();
    assert_eq!(
        t.build(),
        &BuildMatch::Verified {
            label: Some("dev".into())
        }
    );
    assert_eq!(t.exe_sha256(), Some(real.as_str()));
    drop(t);
    let other = custom_pack(&tb, cheat, json!([{ "sha256": "0".repeat(64) }]));
    let t = Trainer::load(process, other).unwrap();
    assert_eq!(t.build(), &BuildMatch::Unknown);
}

#[test]
fn wrong_game_or_arch_is_refused() {
    let _g = serial();
    let x86 = Testbed::spawn(&testbed_exe(Flavor::X86));
    let err = Trainer::load(attach(&x86), example(Flavor::X64)).unwrap_err();
    assert!(matches!(err, Error::WrongArch { .. }), "{err}");
    drop(x86);

    let dir = TempDir::new("wronggame");
    let other = Testbed::spawn(&dir.copy_testbed("other_game.exe"));
    let err = Trainer::load(attach(&other), example(Flavor::X64)).unwrap_err();
    assert!(matches!(err, Error::WrongGame { .. }), "{err}");
}

#[test]
fn a_site_left_patched_is_recognized_and_restored() {
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let process = attach(&tb);
    let mut first = Trainer::load(Arc::clone(&process), example(Flavor::X64)).unwrap();
    let site = first.states().unwrap()[0].address.unwrap();
    drop(first);
    // Simulate a crash that left the patch in place: another session patches
    // and never cleans up.
    let crashed = attach(&tb);
    crashed
        .apply_code(site, &[0x29, 0xD0], &[0x90, 0x90])
        .unwrap();
    std::mem::forget(crashed);
    assert_eq!(tb.send("hit 5")["health"], 100);

    let mut t = Trainer::load(Arc::clone(&process), example(Flavor::X64)).unwrap();
    assert_eq!(status(&mut t, "inf_health").0, "active");
    t.set_active("inf_health", false).unwrap();
    assert_eq!(tb.send("hit 5")["health"], 95);

    drop(t);

    // A site found patched is recorded with the session, so detaching (or
    // the crash journal) restores it even if the cheat is never turned off.
    let again = attach(&tb);
    again
        .apply_code(site, &[0x29, 0xD0], &[0x90, 0x90])
        .unwrap();
    std::mem::forget(again);
    assert!(process.code_changes().is_empty());
    let t2 = Trainer::load(Arc::clone(&process), example(Flavor::X64)).unwrap();
    assert_eq!(process.code_changes().len(), 1);
    assert!(process.restore_code().is_empty());
    assert_eq!(tb.send("hit 5")["health"], 90);
    drop(t2);
}

#[test]
fn shown_state_follows_the_bytes_in_the_game() {
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let process = attach(&tb);
    let mut t = Trainer::load(Arc::clone(&process), example(Flavor::X64)).unwrap();
    t.set_active("inf_health", true).unwrap();
    // Something else (e.g. the crash handler) restores the code.
    process.restore_code();
    t.refresh().unwrap();
    assert_eq!(status(&mut t, "inf_health").0, "ready");
    assert_eq!(tb.send("hit 5")["health"], 95);
    t.set_active("inf_health", true).unwrap();
    assert_eq!(tb.send("hit 5")["health"], 95);
}
