//! Drives the tool server like the AI agent would, against the testbed game
//! (never a real game): find gold, test a write, make it stable, build a
//! pack, try it and save it.

use ot_mcp::{Config, Tools};
use serde_json::{Value, json};
use testbed_harness::{Flavor, TempDir, Testbed, serial, testbed_exe};

fn tools(tmp: &TempDir) -> Tools {
    Tools::new(Config {
        journal_dir: None,
        log_path: Some(tmp.0.join("log.jsonl")),
        packs_dir: Some(tmp.0.join("packs")),
        saves_root: Some(tmp.0.join("saves")),
    })
}

fn call(t: &mut Tools, name: &str, args: Value) -> Value {
    t.call(name, &args)
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn agent_finds_gold_and_writes_a_pack() {
    let _s = serial();
    let tmp = TempDir::new("mcp-session");
    let mut game = Testbed::spawn(&testbed_exe(Flavor::X64));
    let mut t = tools(&tmp);

    let info = call(&mut t, "attach", json!({ "pid": game.pid() }));
    assert_eq!(info["name"], "testbed_native.exe");
    assert!(info["sha256"].as_str().is_some_and(|s| s.len() == 64));

    // "Note your gold" → exact scan; "earn some" → next scan.
    let gold = game.send("state")["gold"].as_i64().unwrap();
    let first = call(
        &mut t,
        "scan_first",
        json!({ "type": "i32", "mode": "exact", "value": gold }),
    );
    assert!(first["count"].as_u64().unwrap() >= 1);
    game.send("earn 37");
    let next = call(
        &mut t,
        "scan_next",
        json!({ "mode": "exact", "value": gold + 37 }),
    );
    let gold_at = format!("0x{:X}", game.addr("gold"));
    let hits: Vec<&str> = next["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["address"].as_str().unwrap())
        .collect();
    assert!(hits.contains(&gold_at.as_str()), "{hits:?} vs {gold_at}");

    // A test write shows up in game and is undone by revert_all.
    call(
        &mut t,
        "write_test",
        json!({ "address": gold_at, "type": "i32", "value": 12345 }),
    );
    assert_eq!(game.send("state")["gold"], 12345);
    call(&mut t, "revert_all", json!({}));
    assert_eq!(game.send("state")["gold"].as_i64().unwrap(), gold + 37);

    // Make it survive a restart: a pointer path from the exe.
    let ptr = call(
        &mut t,
        "pointer_scan",
        json!({ "address": gold_at, "max_depth": 3 }),
    );
    assert!(ptr["count"].as_u64().unwrap() > 0);
    // The testbed's own chain (static root -> world -> player -> stats) is
    // among the paths found.
    let chain = &game.ready["chain"];
    let path = format!(
        "[[[{}]+{}]+{}]+{}",
        chain["root"].as_str().unwrap(),
        chain["offsets"][0].as_str().unwrap(),
        chain["offsets"][1].as_str().unwrap(),
        chain["fields"]["gold"].as_str().unwrap()
    );
    let checked = call(
        &mut t,
        "pointer_check",
        json!({ "address": gold_at, "paths": [path] }),
    );
    assert_eq!(checked["count"], 1, "{checked}");
    let read = call(&mut t, "read", json!({ "address": path, "type": "i32" }));
    assert_eq!(read["value"], (gold + 37).to_string());

    // The damage code's signature is unique.
    let aob = call(
        &mut t,
        "verify_aob_unique",
        json!({ "aob": "0F 1F 80 54 4F 54 01 8B 01 ?? ?? 89 01" }),
    );
    assert_eq!(aob["unique"], true);

    // Build a pack from what was found, try it, save it.
    let inner = path.trim_start_matches('[');
    let (base, rest) = inner.split_once(']').unwrap();
    let offsets: Vec<String> = rest
        .split(']')
        .map(|s| s.trim().trim_start_matches('+').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let pack = json!({
        "schema": 1,
        "game": { "id": "testbed-native-ai", "name": "Testbed (AI)", "exe": "testbed_native.exe", "arch": "x64" },
        "meta": { "author": "someone", "packVersion": "0.1.0" },
        "cheats": [
            { "id": "set_gold", "name": "Set Gold", "ui": "number", "number": { "min": 0, "max": 999999 },
              "impl": { "kind": "value", "type": "i32", "address": { "base": base, "offsets": offsets } } },
            { "id": "inf_health", "name": "Infinite Health", "ui": "toggle",
              "impl": { "kind": "patch", "aob": "0F 1F 80 54 4F 54 01 8B 01 ?? ?? 89 01", "offset": 9,
                        "original": "29 D0", "replace": "90 90" } }
        ]
    });
    assert_eq!(
        call(&mut t, "pack_validate", json!({ "pack": pack }))["valid"],
        true
    );
    let tried = call(&mut t, "pack_try", json!({ "pack": pack }));
    assert_eq!(tried["cheats"].as_array().unwrap().len(), 2);
    call(
        &mut t,
        "cheat_set",
        json!({ "id": "set_gold", "value": 7777 }),
    );
    assert_eq!(game.send("state")["gold"], 7777);
    call(
        &mut t,
        "cheat_set",
        json!({ "id": "inf_health", "on": true }),
    );
    let hp = game.send("state")["health"].as_i64().unwrap();
    assert_eq!(game.send("hit 10")["health"].as_i64().unwrap(), hp);

    let written = call(&mut t, "pack_write", json!({ "pack": pack }));
    let path = written["written"].as_str().unwrap();
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(saved["meta"]["author"], "ai");
    assert_eq!(saved["game"]["builds"][0]["sha256"], info["sha256"]);

    // Detaching undoes the patch and puts the gold the cheat set back.
    call(&mut t, "detach", json!({}));
    assert_eq!(game.send("state")["gold"].as_i64().unwrap(), gold + 37);
    let before = game.send("state")["health"].as_i64().unwrap();
    assert_eq!(game.send("hit 10")["health"].as_i64().unwrap(), before - 10);

    let log = std::fs::read_to_string(tmp.0.join("log.jsonl")).unwrap();
    assert!(log.lines().count() >= 12);
    assert!(log.contains("\"tool\":\"pack_write\""));
}

#[test]
fn guard_refusal_is_final_and_explained() {
    let _s = serial();
    let tmp = TempDir::new("mcp-guard");
    let exe = tmp.copy_testbed(r"Game\testbed_native.exe");
    std::fs::create_dir_all(tmp.0.join("Game").join("BattlEye")).unwrap();
    let game = Testbed::spawn(&exe);
    let mut t = tools(&tmp);
    let e = t.call("attach", &json!({ "pid": game.pid() })).unwrap_err();
    assert!(e.contains("anti-cheat guard blocked"), "{e}");
    assert!(
        t.call("read", &json!({ "address": "0x10", "type": "i32" }))
            .is_err()
    );
}

#[test]
fn agent_makes_a_code_cheat_with_find_what_writes() {
    let _s = serial();
    let tmp = TempDir::new("mcp-code-cheat");
    let mut game = Testbed::spawn(&testbed_exe(Flavor::X64));
    let mut t = tools(&tmp);
    call(&mut t, "attach", json!({ "pid": game.pid() }));
    let recon = call(&mut t, "game_recon", json!({}));
    assert_eq!(recon["engine"], "native");

    // "Take some damage" while watching health.
    let health_at = format!("0x{:X}", game.addr("health"));
    call(
        &mut t,
        "watch_writes_start",
        json!({ "address": health_at }),
    );
    game.send("hit 3");
    game.send("hit 4");
    let found = call(&mut t, "watch_writes_stop", json!({}));
    let w = &found["writers"][0];
    assert_eq!(w["count"], 2, "{found}");
    let idea = &w["patchIdea"];
    let sig = call(&mut t, "make_aob", json!({ "address": idea["makeAobAt"] }));

    let pack = json!({
        "schema": 1,
        "game": { "id": "testbed-code-ai", "name": "Testbed", "exe": "testbed_native.exe", "arch": "x64" },
        "meta": { "author": "ai", "packVersion": "0.1.0" },
        "cheats": [{ "id": "inf_health", "name": "Infinite health", "ui": "toggle",
            "impl": { "kind": "patch", "aob": sig["pattern"], "offset": 0,
                      "original": idea["original"], "replace": idea["replace"] } }]
    });
    call(&mut t, "pack_try", json!({ "pack": pack }));
    call(
        &mut t,
        "cheat_set",
        json!({ "id": "inf_health", "on": true }),
    );
    let hp = game.send("state")["health"].as_i64().unwrap();
    assert_eq!(
        game.send("hit 10")["health"].as_i64().unwrap(),
        hp,
        "health still went down"
    );

    // Saving, then saving another AI pack for the same game, keeps both cheats.
    call(&mut t, "pack_write", json!({ "pack": pack }));
    let mut more = pack.clone();
    more["cheats"] = json!([{ "id": "gold", "name": "Set gold", "ui": "number",
        "impl": { "kind": "value", "type": "i32", "address": { "base": "testbed_native.exe+0x10" } } }]);
    let second = call(&mut t, "pack_write", json!({ "pack": more }));
    assert_eq!(second["keptFromEarlier"], json!(["Infinite health"]));
    assert_eq!(second["cheats"], 2);

    // A pack "from an older build": repair finds the site again.
    let mut old = pack.clone();
    old["cheats"][0]["impl"]["aob"] = json!("0F 1F 80 54 4F 99 01 8B 01 ?? ?? 89 01");
    old["cheats"][0]["impl"]["offset"] = json!(9);
    old["cheats"][0]["impl"]["original"] = json!("29 D0");
    old["cheats"][0]["impl"]["replace"] = json!("90 90");
    call(&mut t, "revert_all", json!({}));
    let healed = call(&mut t, "pack_heal", json!({ "pack": old }));
    assert_eq!(healed["fixed"][0]["cheat"], "inf_health", "{healed}");
    assert_eq!(healed["pack"]["meta"]["packVersion"], "0.1.1");

    call(&mut t, "detach", json!({}));
    let before = game.send("state")["health"].as_i64().unwrap();
    assert_eq!(game.send("hit 1")["health"].as_i64().unwrap(), before - 1);
}

#[test]
fn reads_class_names_of_dotnet_games() {
    let system =
        std::path::Path::new(r"C:\Windows\Microsoft.NET\Framework64\v4.0.30319\System.dll");
    if !system.is_file() {
        return;
    }
    let tmp = TempDir::new("mcp-classes");
    // A pretend XNA-style game: a .NET exe next to FNA.dll.
    let game = tmp.0.join("Game");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::copy(system, game.join("game.exe")).unwrap();
    std::fs::write(game.join("FNA.dll"), b"not real").unwrap();
    let mut t = tools(&tmp);
    let exe = game.join("game.exe").to_string_lossy().into_owned();
    let out = call(
        &mut t,
        "game_classes",
        json!({ "exe": exe, "query": "m_String" }),
    );
    let fields: Vec<&str> = out["fields"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        fields.contains(&"System.Uri.m_String: string"),
        "{fields:?}"
    );
    assert!(out["classes"].as_u64().unwrap() > 1000);

    // Not a .NET game: a clear refusal.
    let native = tmp.copy_testbed(r"Native\testbed_native.exe");
    let e = t
        .call("game_classes", &json!({ "exe": native.to_string_lossy() }))
        .unwrap_err();
    assert!(e.contains("Unity (Mono or IL2CPP) and .NET"), "{e}");
}
