import { describe, expect, it } from "vitest";
import { type PackSummary, packsFor, type ProcessInfo } from "./ipc";
import { formatStep, groupByCategory } from "./trainer";

describe("trainer helpers", () => {
  it("formats slider values to the step's precision", () => {
    expect(formatStep(2.5000001, 0.1)).toBe("2.5");
    expect(formatStep(3, 1)).toBe("3");
    expect(formatStep(0.125, 0.05)).toBe("0.13");
  });

  it("groups cheats by category in first-seen order", () => {
    const groups = groupByCategory([
      { id: "a", name: "A", category: "Player", ui: "toggle", impl: { kind: "patch" } },
      { id: "b", name: "B", ui: "toggle", impl: { kind: "patch" } },
      { id: "c", name: "C", category: "Player", ui: "toggle", impl: { kind: "patch" } },
    ]);
    expect(groups.map(([k, v]) => [k, v.map((c) => c.id)])).toEqual([
      ["Player", ["a", "c"]],
      ["General", ["b"]],
    ]);
  });

  it("matches packs by exe name (any case) and arch", () => {
    const p = { name: "Game.EXE", arch: "x86" } as ProcessInfo;
    const pack = (exe: string | null, arch: PackSummary["arch"]) => ({ key: `${exe}-${arch}`, exe, arch }) as PackSummary;
    const list = [pack("game.exe", "x86"), pack("game.exe", null), pack("game.exe", "x64"), pack("other.exe", "x86"), pack(null, null)];
    expect(packsFor(list, p).map((k) => k.key)).toEqual(["game.exe-x86", "game.exe-null"]);
  });
});
