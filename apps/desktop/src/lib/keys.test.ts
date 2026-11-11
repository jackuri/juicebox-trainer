import { describe, expect, it } from "vitest";
import { chordFromButtons, keyFromEvent } from "./keys";

const key = (code: string, mods: Partial<{ ctrlKey: boolean; altKey: boolean; shiftKey: boolean }> = {}) =>
  keyFromEvent({ code, ctrlKey: false, altKey: false, shiftKey: false, ...mods });

describe("keyFromEvent", () => {
  it("names keys the way packs do", () => {
    expect(key("F5")).toBe("F5");
    expect(key("Numpad1", { ctrlKey: true })).toBe("Ctrl+Num1");
    expect(key("NumpadAdd", { altKey: true })).toBe("Alt+Num+");
    expect(key("KeyH", { ctrlKey: true, shiftKey: true })).toBe("Ctrl+Shift+H");
    expect(key("ArrowUp")).toBe("Up");
    expect(key("Digit7", { altKey: true })).toBe("Alt+7");
  });

  it("ignores lone modifiers and odd keys", () => {
    expect(key("ControlLeft", { ctrlKey: true })).toBeNull();
    expect(key("Backquote")).toBeNull();
  });
});

describe("chordFromButtons", () => {
  it("orders buttons like jb-packs", () => {
    expect(chordFromButtons([12, 5, 4])).toBe("LB+RB+DPadUp");
    expect(chordFromButtons([0, 6, 7])).toBe("LT+RT+A");
    expect(chordFromButtons([99])).toBe("");
  });
});
