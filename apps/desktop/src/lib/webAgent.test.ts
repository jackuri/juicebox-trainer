// Tests OpenTrainer's web game helper (src-tauri/src/web_agent.js) on a fake page.
import { beforeAll, describe, expect, it } from "vitest";
import agentSource from "../../src-tauri/src/web_agent.js?raw";

interface Agent {
  callJson(method: string, args?: unknown): string;
}
type Answer = { ok?: Record<string, unknown> & { results?: Hit[]; count?: number }; error?: string };
interface Hit {
  id: number;
  target: Record<string, unknown>;
  where: string;
  value: number;
}

const WASM = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x05, 0x03, 0x01, 0x00, 0x01, 0x07, 0x07, 0x01, 0x03, 0x6d, 0x65, 0x6d,
  0x02, 0x00,
]);

let agent: Agent;
let memory: WebAssembly.Memory;
const call = (method: string, args?: unknown): Answer => JSON.parse(agent.callJson(method, args)) as Answer;
const view = () => new DataView(memory.buffer);
let getterCalls = 0;

beforeAll(async () => {
  new Function(agentSource)();
  agent = (window as unknown as { __otAgent: Agent }).__otAgent;
  // The game makes its memory after the helper is in the page (like a real one).
  const { instance } = await WebAssembly.instantiate(WASM);
  memory = instance.exports.mem as WebAssembly.Memory;
  view().setInt32(0x1000, 7531, true);
  view().setFloat32(0x2000, 30, true);
  const w = window as unknown as Record<string, unknown>;
  w.game = { player: { gold: 4321, name: "x" } };
  w.risky = Object.defineProperty({}, "hp", {
    enumerable: true,
    get() {
      getterCalls++;
      return 4321;
    },
  });
});

describe("web game helper", () => {
  it("can't be replaced by the page and only answers known calls", () => {
    const w = window as unknown as Record<string, unknown>;
    expect(() => {
      w.__otAgent = { callJson: () => "fake" };
    }).toThrow();
    expect(w.__otAgent).toBe(agent);
    expect(call("eval", { code: "1" }).error).toMatch(/unknown call/);
    expect(call("find", { value: "x" }).error).toMatch(/type a number/);
  });

  it("finds the game's WebAssembly memory", () => {
    const info = call("info").ok as { memories: { label: string; bytes: number }[] };
    expect(info.memories.some((m) => m.bytes === 65536)).toBe(true);
  });

  it("finds a number in memory, narrows it, changes it and locks it", async () => {
    const first = call("find", { value: 7531, kind: "whole" }).ok!;
    const hit = first.results!.find((h) => h.target.off === 0x1000);
    expect(hit?.target).toMatchObject({ k: "m", t: "i32" });
    expect(hit?.where).toMatch(/at 0x1000 \(i32\)/);

    view().setInt32(0x1000, 7521, true); // took a hit
    const next = call("next", { how: "decreased" }).ok!;
    expect(next.count).toBe(1);
    const target = next.results![0]!.target;

    expect(call("set", { target, value: 999 }).ok).toEqual({ value: 999 });
    expect(view().getInt32(0x1000, true)).toBe(999);

    call("lock", { id: "hp", target, value: 500 });
    view().setInt32(0x1000, 1, true);
    await new Promise((r) => setTimeout(r, 250));
    expect(view().getInt32(0x1000, true)).toBe(500);
    call("unlock", { id: "hp" });
    view().setInt32(0x1000, 2, true);
    await new Promise((r) => setTimeout(r, 250));
    expect(view().getInt32(0x1000, true)).toBe(2);
  });

  it("finds decimal numbers (rounded)", () => {
    const found = call("find", { value: 30, kind: "decimal" }).ok!;
    expect(found.results!.some((h) => h.target.off === 0x2000 && h.target.t === "f32")).toBe(true);
  });

  it("finds a JavaScript value by its path, without calling getters", () => {
    getterCalls = 0;
    const found = call("find", { value: 4321, kind: "any" }).ok!;
    const gold = found.results!.find((h) => h.target.k === "j");
    expect(gold?.where).toBe("game.player.gold");
    expect(getterCalls).toBe(0);
    expect(call("set", { target: gold!.target, value: 99999 }).ok).toEqual({ value: 99999 });
    expect((window as unknown as { game: { player: { gold: number } } }).game.player.gold).toBe(99999);
    // Reading a path that's gone says so.
    const read = call("read", { targets: [{ k: "j", path: ["nope", "x"] }, gold!.target] }).ok as unknown as {
      value?: number;
      error?: string;
    }[];
    expect(read[0]?.error).toMatch(/isn't there/);
    expect(read[1]?.value).toBe(99999);
  });

  it("finds numbers by name for the AI helper, without calling getters", () => {
    getterCalls = 0;
    const found = call("names", { query: "GOLD | hp" }).ok!;
    expect(found.results!.map((h) => h.where)).toEqual(["game.player.gold"]);
    expect(found.results![0]!.target).toEqual({ k: "j", path: ["game", "player", "gold"] });
    expect(getterCalls).toBe(0);
    // The object's name counts too ("player.gold" matches "player").
    expect(call("names", { query: "player" }).ok!.count).toBe(1);
    expect(call("names", { query: "nothing-like-this" }).ok!.count).toBe(0);
    expect(call("names", { query: " " }).error).toMatch(/which name/);
  });

  it("refuses writes outside the memory and non-numbers", () => {
    const outside = { k: "m", mem: 0, label: "m", size: 65536, off: 70000, t: "i32" };
    expect(call("set", { target: outside, value: 1 }).error).toMatch(/outside/);
    expect(call("set", { target: { k: "j", path: ["game", "player", "name"] }, value: 1 }).error).toMatch(/isn't a number/);
    expect(call("next", { how: "equals", value: 1 }).ok).toBeDefined();
  });

  it("works the cheats' keys while the game is in front, before the game sees them", () => {
    const w = window as unknown as { game: { player: { gold: number; level?: number } } };
    w.game.player.gold = 10;
    w.game.player.level = 3;
    const gold = { k: "j", path: ["game", "player", "gold"] };
    const level = { k: "j", path: ["game", "player", "level"] };
    expect(call("state").ok).toEqual({ locks: [], keysTag: "" });
    call("keys", {
      tag: "t1",
      list: [
        { id: "gold", name: "Gold", key: "F1", does: "lock", target: gold, value: 777 },
        { id: "level", name: "Level", key: "Ctrl+L", does: "set", target: level, value: 50 },
        { id: "bad", name: "Bad", key: "F2", does: "set", target: level, value: null },
      ],
    });
    expect(call("state").ok).toEqual({ locks: [], keysTag: "t1" });

    let gameSaw = 0;
    const game = () => gameSaw++;
    window.addEventListener("keydown", game);
    const press = (init: KeyboardEventInit, on: EventTarget = window) => {
      const e = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
      on.dispatchEvent(e);
      return e;
    };

    const e = press({ code: "F1" });
    expect(e.defaultPrevented).toBe(true);
    expect(w.game.player.gold).toBe(777);
    expect(call("state").ok).toEqual({ locks: ["gold"], keysTag: "t1" });
    expect(document.body.lastElementChild?.tagName).toBe("DIV"); // the note in the corner
    press({ code: "F1" });
    expect(call("state").ok).toEqual({ locks: [], keysTag: "t1" });

    press({ code: "KeyL", ctrlKey: true });
    expect(w.game.player.level).toBe(50);
    // A key that would set nothing was left out; other keys and held keys reach the game.
    w.game.player.level = 3;
    press({ code: "F2" });
    press({ code: "KeyL" });
    press({ code: "F1", repeat: true });
    expect(w.game.player.level).toBe(3);
    expect(gameSaw).toBe(3);

    // Typing a letter into a chat box isn't a cheat; F-keys still are.
    call("keys", { tag: "t2", list: [{ id: "g", name: "Gold", key: "G", does: "lock", target: gold, value: 1 }] });
    const chat = document.createElement("input");
    document.body.appendChild(chat);
    expect(press({ code: "KeyG" }, chat).defaultPrevented).toBe(false);
    expect(press({ code: "KeyG" }).defaultPrevented).toBe(true);
    call("keys", { tag: "", list: [] });
    call("unlock", {});
    window.removeEventListener("keydown", game);
  });
});
