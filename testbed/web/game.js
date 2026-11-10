// OpenTrainer's web testbed: a tiny fake game (never a real one).
// - gold: a number in a JavaScript object (window.game.player.gold), like most HTML5 games
// - health (32-bit integer at 0x1000) and ammo (32-bit float at 0x2000): in a WebAssembly memory
//   exported by a module, like a game built with Emscripten or a console emulator
"use strict";

// (module (memory (export "mem") 1)): one 64 KiB page, exported as "mem".
const WASM = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x05, 0x03, 0x01, 0x00, 0x01, 0x07, 0x07, 0x01, 0x03, 0x6d, 0x65, 0x6d,
  0x02, 0x00,
]);
const HEALTH = 0x1000;
const AMMO = 0x2000;

window.game = { player: { name: "Tester", gold: 100, level: 3 }, world: { day: 1 } };

WebAssembly.instantiate(WASM).then(({ instance }) => {
  const memory = instance.exports.mem;
  const view = () => new DataView(memory.buffer);
  view().setInt32(HEALTH, 75, true);
  view().setFloat32(AMMO, 30, true);

  const show = () => {
    document.getElementById("gold").textContent = String(window.game.player.gold);
    document.getElementById("health").textContent = String(view().getInt32(HEALTH, true));
    document.getElementById("ammo").textContent = String(view().getFloat32(AMMO, true));
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
