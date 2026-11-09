//! A Cheat Engine table written for the testbed (never a real game), opened
//! as a pack and used on the live testbed: the values it points at must be
//! the game's real ones.

use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use ot_core::AttachedProcess;
use ot_packs::{Trainer, share};
use serde_json::Value as Json;
use testbed_harness::*;

/// `0x1A` → `1A` (Cheat Engine writes hex without the prefix).
fn bare(v: &Json) -> String {
    v.as_str().unwrap().trim_start_matches("0x").to_string()
}

/// A table like the ones shared on forums: a group, a pointer with
/// `<Offsets>` (stored last-applied first), the bracket form, a child entry
/// relative to its parent, and a script that must not come over.
fn table(ready: &Json) -> String {
    let chain = &ready["chain"];
    let (module, root) = chain["root"].as_str().unwrap().split_once('+').unwrap();
    let root = root.trim_start_matches("0x");
    let player = bare(&chain["offsets"][0]);
    let stats = bare(&chain["offsets"][1]);
    let f = &chain["fields"];
    let (health, gold, speed) = (bare(&f["health"]), bare(&f["gold"]), bare(&f["speed"]));
    let speed_from_health =
        u64::from_str_radix(&speed, 16).unwrap() - u64::from_str_radix(&health, 16).unwrap();
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<CheatTable CheatEngineTableVersion="45">
  <CheatEntries>
    <CheatEntry>
      <ID>0</ID>
      <Description>"Player"</Description>
      <Options moHideChildren="1"/>
      <GroupHeader>1</GroupHeader>
      <CheatEntries>
        <CheatEntry>
          <ID>1</ID>
          <Description>"Health"</Description>
          <ShowAsSigned>1</ShowAsSigned>
          <VariableType>4 Bytes</VariableType>
          <Address>"{module}"+{root}</Address>
          <Offsets>
            <Offset>{health}</Offset>
            <Offset>{stats}</Offset>
            <Offset>{player}</Offset>
          </Offsets>
          <CheatEntries>
            <CheatEntry>
              <ID>2</ID>
              <Description>"Speed"</Description>
              <VariableType>Float</VariableType>
              <Address>+{speed_from_health:X}</Address>
            </CheatEntry>
          </CheatEntries>
        </CheatEntry>
        <CheatEntry>
          <ID>3</ID>
          <Description>"Gold"</Description>
          <VariableType>4 Bytes</VariableType>
          <Address>[[[{module}+{root}]+{player}]+{stats}]+{gold}</Address>
        </CheatEntry>
      </CheatEntries>
    </CheatEntry>
    <CheatEntry>
      <ID>4</ID>
      <Description>"Infinite ammo"</Description>
      <VariableType>Auto Assembler Script</VariableType>
      <AssemblerScript>[ENABLE]
aobscanmodule(INJECT,{module},29 D0 89 01)
alloc(newmem,$1000,INJECT)
[DISABLE]
dealloc(newmem)
</AssemblerScript>
    </CheatEntry>
  </CheatEntries>
</CheatTable>
"#
    )
}

fn table_works(flavor: Flavor) {
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(flavor));
    let opened = share::open_table(table(&tb.ready).as_bytes(), "Testbed_Native", None).unwrap();
    assert!(!opened.needs_exe);
    assert_eq!(opened.pack.game.exe, tb.ready["module"].as_str().unwrap());
    let names: Vec<&str> = opened.pack.cheats.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Health", "Speed", "Gold"]);
    assert_eq!(opened.skipped.len(), 1);
    assert_eq!(opened.skipped[0].name, "Infinite ammo");

    let process = Arc::new(AttachedProcess::attach(tb.pid()).expect("attach to testbed"));
    let mut t = Trainer::load(process, opened.pack).unwrap();
    for s in t.states().unwrap() {
        assert_eq!(s.status, "ready", "{} on {flavor:?}: {:?}", s.id, s.message);
    }
    let shown = |t: &mut Trainer, id: &str| {
        t.states()
            .unwrap()
            .into_iter()
            .find(|s| s.id == id)
            .unwrap()
            .value
    };
    assert_eq!(shown(&mut t, "health").as_deref(), Some("100"));
    assert_eq!(
        shown(&mut t, "gold").as_deref(),
        Some(tb.send("state")["gold"].to_string().as_str())
    );

    // Set and held (like ticking the box in Cheat Engine), until released.
    t.set_value("health", "555").unwrap();
    tb.send("hit 10");
    sleep(Duration::from_millis(200));
    assert_eq!(tb.send("state")["health"], 555);
    t.release("health").unwrap();
    assert_eq!(tb.send("hit 5")["health"], 550);

    t.set_value("gold", "4242").unwrap();
    assert_eq!(tb.send("state")["gold"], 4242);
    t.set_value("speed", "2.5").unwrap();
    assert_eq!(tb.send("state")["speed"], 2.5);
    t.restore_all().unwrap();
    tb.kill();
}

#[test]
fn cheat_table_values_reach_the_x64_testbed() {
    table_works(Flavor::X64);
}

#[test]
fn cheat_table_values_reach_the_x86_testbed() {
    table_works(Flavor::X86);
}

/// A table in the style of FearLessRevolution tables: a Lua script finds the
/// game's data pointer from its code (RIP-relative), a label holds a save-slot
/// number used in offset math, a byte-swap script makes health stop going
/// down, and an injection script stays out. Nothing in it is run: OpenTrainer
/// redoes the first three its own way.
fn script_table(ready: &Json) -> String {
    let chain = &ready["chain"];
    let player = bare(&chain["offsets"][0]);
    let stats = bare(&chain["offsets"][1]);
    let f = &chain["fields"];
    let (gold, speed) = (bare(&f["gold"]), bare(&f["speed"]));
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<CheatTable CheatEngineTableVersion="45">
  <CheatEntries>
    <CheatEntry>
      <ID>0</ID>
      <Description>"Game Data Pointers"</Description>
      <VariableType>Auto Assembler Script</VariableType>
      <AssemblerScript>[ENABLE]
{{$lua}}
if syntaxcheck then return end
autoAssemble([[
aobscanmodule(world,testbed_native.exe,0F 1F 80 54 4F 54 03 48 8B 0D * * * *)
registersymbol(world)
label(slot)
1:
slot:
registersymbol(slot)
]])
local address = getAddress("world") + 10
address = address + readInteger(address) + 4
unregisterSymbol("world")
registerSymbol("world", address, true)
{{$asm}}
[DISABLE]
unregistersymbol(world)
unregistersymbol(slot)
</AssemblerScript>
      <CheatEntries>
        <CheatEntry>
          <ID>1</ID>
          <Description>"Stats Base Address"</Description>
          <VariableType>String</VariableType>
          <Length>8</Length>
          <Address>world</Address>
          <Offsets>
            <Offset>0</Offset>
            <Offset>slot*{stats}-{stats}+{stats}</Offset>
            <Offset>{player}</Offset>
          </Offsets>
          <CheatEntries>
            <CheatEntry>
              <ID>2</ID>
              <Description>"Gold"</Description>
              <VariableType>4 Bytes</VariableType>
              <Address>+{gold}</Address>
            </CheatEntry>
          </CheatEntries>
        </CheatEntry>
        <CheatEntry>
          <ID>3</ID>
          <Description>"Move Speed"</Description>
          <VariableType>Float</VariableType>
          <Address>[[[world]+{player}]+{stats}]+{speed}</Address>
        </CheatEntry>
      </CheatEntries>
    </CheatEntry>
    <CheatEntry>
      <ID>4</ID>
      <Description>"No Damage"</Description>
      <VariableType>Auto Assembler Script</VariableType>
      <AssemblerScript>[ENABLE]
aobscanmodule(hurt,testbed_native.exe,0F 1F 80 54 4F 54 01 8B 01 29 D0 89 01) // should be unique
hurt+9:
  db 90 90
registersymbol(hurt)
[DISABLE]
hurt+9:
  db 29 D0
unregistersymbol(hurt)
{{
// ORIGINAL CODE - INJECTION POINT: "testbed_native.exe"+1234
// ---------- INJECTING HERE ----------
}}
</AssemblerScript>
    </CheatEntry>
    <CheatEntry>
      <ID>5</ID>
      <Description>"Infinite Ammo"</Description>
      <VariableType>Auto Assembler Script</VariableType>
      <AssemblerScript>[ENABLE]
aobscanmodule(ammo,testbed_native.exe,0F 1F 80 54 4F 54 02 8B 01)
alloc(newmem,$1000,ammo)
newmem:
  mov rax,[rcx]
  jmp return
[DISABLE]
dealloc(newmem)
</AssemblerScript>
    </CheatEntry>
  </CheatEntries>
</CheatTable>
"#
    )
}

#[test]
fn script_style_table_works_on_the_x64_testbed() {
    let _g = serial();
    let mut tb = Testbed::spawn(&testbed_exe(Flavor::X64));
    let opened =
        share::open_table(script_table(&tb.ready).as_bytes(), "Testbed_Scripts", None).unwrap();
    assert!(!opened.needs_exe);
    let names: Vec<&str> = opened.pack.cheats.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        ["Gold", "Move Speed", "No Damage"],
        "skipped: {:?}",
        opened.skipped
    );
    // Only the injection script and the text entry are left out; the script
    // that finds the pointer isn't needed (OpenTrainer finds it itself).
    let skipped: Vec<&str> = opened.skipped.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(skipped, ["Stats Base Address", "Infinite Ammo"]);

    let process = Arc::new(AttachedProcess::attach(tb.pid()).expect("attach to testbed"));
    let mut t = Trainer::load(process, opened.pack).unwrap();
    for s in t.states().unwrap() {
        assert_eq!(s.status, "ready", "{}: {:?}", s.id, s.message);
    }
    t.set_value("gold", "31337").unwrap();
    assert_eq!(tb.send("state")["gold"], 31337);
    t.set_value("move-speed", "3.5").unwrap();
    assert_eq!(tb.send("state")["speed"], 3.5);

    // The byte swap: hits do no damage while on, and do again when off.
    let before = tb.send("state")["health"].as_i64().unwrap();
    t.set_active("no-damage", true).unwrap();
    assert_eq!(tb.send("hit 30")["health"].as_i64().unwrap(), before);
    t.set_active("no-damage", false).unwrap();
    assert_eq!(tb.send("hit 30")["health"].as_i64().unwrap(), before - 30);
    t.restore_all().unwrap();
    tb.kill();
}
