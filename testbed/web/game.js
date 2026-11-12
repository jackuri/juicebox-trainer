// JuiceBox Trainer's web testbed: a tiny fake game (never a real one).
// - gold: a number in a JavaScript object (window.game.player.gold), like most HTML5 games
// - rockets: a weapon object (window.game.player.weapon.rockets) that swapping weapons empties, like
//   a game that clears a weapon's ammo when it's put away (v1.27.1: a search result whose value is
//   gone must show "?", not break JuiceBox Trainer's screen)
// - health (32-bit integer at 0x1000) and ammo (32-bit float at 0x2000): in a WebAssembly memory
//   exported by a module, like a game built with Emscripten or a console emulator
// - zombies (v1.31.0): 8 slots in the same memory from 0x4000, 0x40 apart, each with its health as a
//   16-bit integer (0 = no zombie there); a new zombie takes a free slot, like a game's enemy list
"use strict";

// (module (memory (export "mem") 1)): one 64 KiB page, exported as "mem".
const WASM = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x05, 0x03, 0x01, 0x00, 0x01, 0x07, 0x07, 0x01, 0x03, 0x6d, 0x65, 0x6d,
  0x02, 0x00,
]);
const HEALTH = 0x1000;
const AMMO = 0x2000;
const ZOMBIES = 0x4000;
const ZOMBIE_SIZE = 0x40;
const ZOMBIE_SLOTS = 8;

window.game = { player: { name: "Tester", gold: 100, level: 3, weapon: { name: "Launcher", rockets: 7 } }, world: { day: 1 } };

WebAssembly.instantiate(WASM).then(({ instance }) => {
  const memory = instance.exports.mem;
  const view = () => new DataView(memory.buffer);
  view().setInt32(HEALTH, 75, true);
  view().setFloat32(AMMO, 30, true);
  [30, 30, 30].forEach((hp, i) => view().setInt16(ZOMBIES + i * ZOMBIE_SIZE, hp, true));
  const zombies = () => Array.from({ length: ZOMBIE_SLOTS }, (_, i) => view().getInt16(ZOMBIES + i * ZOMBIE_SIZE, true));

  const show = () => {
    document.getElementById("gold").textContent = String(window.game.player.gold);
    document.getElementById("health").textContent = String(view().getInt32(HEALTH, true));
    document.getElementById("ammo").textContent = String(view().getFloat32(AMMO, true));
    document.getElementById("rockets").textContent = String(window.game.player.weapon.rockets);
    document.getElementById("zombies").textContent = zombies().filter((hp) => hp > 0).join(", ") || "none";
  };
  document.getElementById("spawn").onclick = () => {
    const free = zombies().findIndex((hp) => hp <= 0);
    if (free >= 0) view().setInt16(ZOMBIES + free * ZOMBIE_SIZE, 40, true);
    show();
  };
  document.getElementById("kill").onclick = () => {
    const first = zombies().findIndex((hp) => hp > 0);
    if (first >= 0) view().setInt16(ZOMBIES + first * ZOMBIE_SIZE, 0, true);
    show();
  };
  document.getElementById("fire").onclick = () => {
    window.game.player.weapon.rockets -= 1;
    show();
  };
  document.getElementById("swap").onclick = () => {
    window.game.player.weapon.rockets = null;
    show();
  };
  document.getElementById("earn").onclick = () => {
    window.game.player.gold += 25;
    show();
  };
  document.getElementById("hit").onclick = () => {
    view().setInt32(HEALTH, view().getInt32(HEALTH, true) - 10, true);
    show();
  };
  document.getElementById("shoot").onclick = () => {
    view().setFloat32(AMMO, view().getFloat32(AMMO, true) - 1, true);
    show();
  };
  setInterval(show, 200);
  show();
  window.testbedReady = true;
});
