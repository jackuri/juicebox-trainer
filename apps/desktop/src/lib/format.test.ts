import { describe, expect, it } from "vitest";
import {
  backupReason,
  describeFinding,
  fileSize,
  filterProcesses,
  hex,
  isSystemProcess,
  timeAgo,
} from "./format";
import type { ProcessInfo } from "./ipc";

const proc = (name: string, pid: number, exePath: string | null): ProcessInfo => ({
  pid,
  parentPid: 0,
  name,
  exePath,
  arch: "x64",
  threads: 1,
});

const list = [
  proc("svchost.exe", 900, "C:\\Windows\\System32\\svchost.exe"),
  proc("System", 4, null),
  proc("testbed_native.exe", 1234, "C:\\dev\\opentrainer\\target\\debug\\testbed_native.exe"),
  proc("Game.exe", 5678, "D:\\Games\\Game\\Game.exe"),
];

describe("hex", () => {
  it("formats uppercase with prefix", () => {
    expect(hex(255)).toBe("0xFF");
    expect(hex(0x7ff6a0001000)).toBe("0x7FF6A0001000");
  });
});

describe("filterProcesses", () => {
  it("hides Windows and pathless processes", () => {
    expect(isSystemProcess(list[0]!)).toBe(true);
    expect(isSystemProcess(list[1]!)).toBe(true);
    expect(filterProcesses(list, "", true).map((p) => p.pid)).toEqual([1234, 5678]);
  });

  it("matches name substrings and exact pids", () => {
    expect(filterProcesses(list, "TESTBED", false).map((p) => p.pid)).toEqual([1234]);
    expect(filterProcesses(list, "5678", false).map((p) => p.pid)).toEqual([5678]);
    expect(filterProcesses(list, "56", false)).toEqual([]);
  });
});

describe("describeFinding", () => {
  it("reads naturally", () => {
    expect(
      describeFinding({ kind: "installMarker", product: "BattlEye", detail: "BattlEye" }),
    ).toBe("BattlEye: found in the game folder (BattlEye)");
  });
});

describe("save formatting", () => {
  it("formats sizes, ages and reasons", () => {
    expect(fileSize(512)).toBe("512 B");
    expect(fileSize(2048)).toBe("2.0 KB");
    expect(fileSize(5 * 1024 * 1024)).toBe("5.0 MB");
    const now = 1_000_000_000;
    expect(timeAgo(now - 10_000, now)).toBe("just now");
    expect(timeAgo(now - 5 * 60_000, now)).toBe("5 min ago");
    expect(timeAgo(now - 3 * 3_600_000, now)).toBe("3 h ago");
    expect(timeAgo(now - 3 * 86_400_000, now)).toBe("3 days ago");
    expect(backupReason("beforeRestore")).toBe("Before a restore");
  });
});
