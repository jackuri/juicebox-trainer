import { beforeEach, describe, expect, it } from "vitest";
import { cheatName, loadPending, savePending, staticLocation, testValue } from "./finder";

describe("testValue", () => {
  it("picks a clearly different, usually smaller number", () => {
    expect(testValue("1250", "i32")).toBe("625");
    expect(testValue("7", "i32")).toBe("3");
    expect(testValue("1", "i32")).toBe("6");
    expect(testValue("100.5", "f32")).toBe("50.25");
    expect(testValue("0.5", "f64")).toBe("5.5");
    expect(testValue("??", "i32")).toBeNull();
  });
});

describe("staticLocation", () => {
  const modules = [{ name: "game.exe", path: "C:\\g\\game.exe", base: 0x140000000, size: 0x100000 }];
  it("gives module + offset for an address inside a module", () => {
    expect(staticLocation(0x140001234, modules)).toEqual({ module: "game.exe", offset: 0x1234 });
    expect(staticLocation(0x140100000, modules)).toBeNull();
    expect(staticLocation(0x2000, modules)).toBeNull();
  });
});

describe("pending cheats", () => {
  beforeEach(() => localStorage.clear());
  it("are kept per game exe", () => {
    savePending({ exe: "Game.exe", pid: 1, gameName: "Game", name: "Gold", ty: "i32", value: "999", keep: true });
    expect(loadPending("game.exe")?.name).toBe("Gold");
    expect(loadPending("other.exe")).toBeNull();
    savePending(null);
    expect(loadPending("game.exe")).toBeNull();
  });
});

it("names cheats", () => {
  expect(cheatName("  Gold   coins ")).toBe("Gold coins");
  expect(cheatName("")).toBe("My value");
});
