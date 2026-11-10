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

  it("refuses writes outside the memory and non-numbers", () => {
    const outside = { k: "m", mem: 0, label: "m", size: 65536, off: 70000, t: "i32" };
    expect(call("set", { target: outside, value: 1 }).error).toMatch(/outside/);
    expect(call("set", { target: { k: "j", path: ["game", "player", "name"] }, value: 1 }).error).toMatch(/isn't a number/);
    expect(call("next", { how: "equals", value: 1 }).ok).toBeDefined();
  });
});
