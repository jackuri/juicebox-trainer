import { describe, expect, it } from "vitest";
import {
  backupReason,
  describeFinding,
  fileSize,
  hex,
  inspect,
  isSystemProcess,
  splitGames,
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

describe("splitGames", () => {
  const games = [{ pid: 5678, name: "Big Game", libraryId: "steam:1" }];
  const pids = (s: ReturnType<typeof splitGames>) => [s.games.map((g) => g.process.pid), s.others.map((p) => p.pid)];

  it("puts games first and hides Windows and pathless processes", () => {
    expect(isSystemProcess(list[0]!)).toBe(true);
    expect(isSystemProcess(list[1]!)).toBe(true);
    expect(pids(splitGames(list, games))).toEqual([[5678], [1234]]);
  });

  it("matches game names, exe name substrings and exact pids", () => {
    expect(pids(splitGames(list, games, "big"))).toEqual([[5678], []]);
    expect(pids(splitGames(list, games, "TESTBED"))).toEqual([[], [1234]]);
    expect(pids(splitGames(list, games, "5678"))).toEqual([[5678], []]);
    expect(pids(splitGames(list, games, "56"))).toEqual([[], []]);
  });

  it("skips a game whose process is gone", () => {
    expect(splitGames(list, [{ pid: 1, name: "Gone", libraryId: null }]).games).toEqual([]);
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

describe("hex inspector", () => {
  it("reads little-endian numbers", () => {
    const n = inspect([0xe8, 0x03, 0, 0, 0, 0, 0, 0]);
    expect(n.find((x) => x.label === "32-bit")?.value).toBe("1000 / 1000");
    expect(n.find((x) => x.label === "8-bit")?.value).toBe("232 / -24");
    expect(inspect([1, 0])).toHaveLength(2);
  });
});
