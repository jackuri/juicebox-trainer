import { describe, expect, it } from "vitest";
import { friendlyError, problemText } from "./explain";
import type { Problem } from "./ipc";

const closed: Problem = {
  id: Date.UTC(2026, 10, 11, 2, 1),
  kind: "gameClosed",
  game: "Coin Quest",
  title: "Coin Quest crashed",
  why: "It crashed: it tried to use memory it couldn't.",
  advice: ["Start the game again and try it without that cheat."],
  details: ["Exit code: 0xC0000005 (3221225477)", "Cheats on: Infinite ammo"],
  crashed: true,
  seen: false,
};

describe("friendlyError", () => {
  it("puts engine errors in plain words with what to try", () => {
    const read = friendlyError({ kind: "read", message: "could not read 4 byte(s) at 0x1A2B", verdict: null });
    expect(read).toMatch(/couldn't read that value/);
    expect(read).not.toMatch(/0x1A2B/);
    expect(friendlyError({ kind: "needsRepair", message: "needs repair: expected 89 01", verdict: null })).toMatch(/Repair/);
    expect(friendlyError({ kind: "accessDenied", message: "x", verdict: null })).toMatch(/Run as administrator/);
  });

  it("says why the game closed", () => {
    expect(friendlyError({ kind: "processExited", message: "process 4 has exited", verdict: null, closed })).toBe(
      "Coin Quest crashed. It crashed: it tried to use memory it couldn't.",
    );
  });

  it("keeps JuiceBox Trainer's own messages and wraps technical ones", () => {
    expect(friendlyError({ kind: "webGame", message: "the game page took too long to answer", verdict: null })).toBe(
      "the game page took too long to answer",
    );
    expect(friendlyError({ kind: "io", message: "disk full", verdict: null })).toMatch(/Something went wrong \(disk full\)/);
    expect(friendlyError("boom")).toBe("boom");
  });
});

describe("problemText", () => {
  it("copies everything a helper needs", () => {
    const text = problemText(closed);
    expect(text).toMatch(/^Coin Quest crashed \(/);
    expect(text).toContain("What to try:\n- Start the game again");
    expect(text).toContain("- Cheats on: Infinite ammo");
  });
});
