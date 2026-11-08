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
