import { describe, expect, it } from "vitest";
import type { WebCheat, WebTarget } from "./ipc";
import { combine, combinedName, keepWords, where } from "./webCheats";

const mem = (off: number, over: Partial<Extract<WebTarget, { k: "m" }>> = {}): WebCheat => ({
  name: `at ${off}`,
  target: { k: "m", mem: 0, label: "m", size: 16777216, off, t: "i16", ...over },
  value: 1,
});
const js = (...path: string[]): WebCheat => ({ name: path.join("."), target: { k: "j", path } });

describe("combining web cheats (v1.31.0)", () => {
  it("makes one group from places the same distance apart", () => {
    const r = combine([mem(0x6cb554), mem(0x6cd074), mem(0x6cc2e4)]);
    expect(r).toEqual({ target: { k: "m", mem: 0, label: "m", size: 16777216, off: 0x6cb554, t: "i16", every: 0xd90, count: 3 }, places: 3 });
    // A gap is one of the list too (the enemy between them).
    expect(combine([mem(100), mem(300)])).toMatchObject({ places: 2 });
    expect(combine([mem(0), mem(16), mem(48)])).toMatchObject({ target: { every: 16, count: 4 }, places: 4 });
    // An existing group joins in.
    expect(combine([mem(0, { every: 8, count: 3 }), mem(24)])).toMatchObject({ target: { every: 8, count: 4 } });
    expect(where((combine([mem(0x6cb554), mem(0x6cc2e4)]) as { target: WebTarget }).target)).toBe("memory 1 at 0x6CB554, 2 places 0xD90 apart (i16)");
  });

  it("says why when they can't be one", () => {
    expect(combine([mem(0)])).toHaveProperty("why");
    expect(combine([mem(0), mem(8, { t: "i32" })])).toHaveProperty("why", expect.stringMatching(/same kind of number/));
    expect(combine([mem(0), mem(8, { mem: 1 })])).toHaveProperty("why");
    expect(combine([mem(0x1000), mem(0x6cb554)])).toHaveProperty("why", expect.stringMatching(/evenly spaced/));
    expect(combine([mem(0), mem(4), mem(4000)])).toHaveProperty("why");
    expect(combine([mem(0), js("a", "b")])).toHaveProperty("why", expect.stringMatching(/JavaScript/));
  });

  it("uses * for the one part of a JavaScript path that differs", () => {
    expect(combine([js("game", "zombies", "0", "hp"), js("game", "zombies", "3", "hp")])).toEqual({ target: { k: "j", path: ["game", "zombies", "*", "hp"] }, places: 2 });
    expect(combine([js("game", "a", "hp"), js("game", "b", "mp")])).toHaveProperty("why");
    expect(combine([js("game", "hp"), js("other", "hp")])).toHaveProperty("why");
  });

  it("names it and says how it keeps the number", () => {
    expect(combinedName("Weak enemy 1 (1 HP)")).toBe("Weak enemy (1 HP)");
    expect(combinedName("Zombie #12")).toBe("Zombie");
    expect(combinedName("Gold")).toBe("Gold");
    expect(keepWords("atMost", 1)).toBe("never above 1");
    expect(keepWords("atLeast", 50)).toBe("never below 50");
    expect(keepWords("at", 1)).toBeNull();
    expect(keepWords(undefined, 1)).toBeNull();
  });
});
