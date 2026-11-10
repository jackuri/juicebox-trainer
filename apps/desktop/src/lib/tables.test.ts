import { afterEach, describe, expect, it } from "vitest";
import type { LibraryGame, RecentTable } from "./ipc";
import { dismiss, exeFor, isDismissed, lastSearch, matchGame, rememberSearch, SEARCH_WINDOW_MS } from "./tables";

function game(id: string, name: string, exe: string | null = null, packExe?: string): LibraryGame {
  return {
    id,
    name,
    store: "steam",
    folder: `C:\\Games\\${name}`,
    exe,
    launch: { kind: "manual" },
    pack: packExe ? { key: "k", name, ai: false, exe: packExe } : null,
    presets: [],
    autoPreset: null,
    blocked: null,
    antiCheat: null,
    hidden: false,
    cover: null,
  };
}

function table(over: Partial<RecentTable> = {}): RecentTable {
  return {
    path: "C:\\Users\\me\\Downloads\\x.CT",
    file: "x.CT",
    modified: 1_000_000,
    game: "x",
    exe: null,
    cheats: 3,
    error: null,
    ...over,
  };
}

const GAMES = [
  game("ds", "Dead Space", "C:\\Games\\Dead Space\\Dead Space.exe"),
  game("ds2", "Dead Space 2", "C:\\Games\\Dead Space 2\\deadspace2.exe"),
  game("bg3", "Baldur's Gate 3", null, "bg3.exe"),
  game("hk", "Hollow Knight", "D:\\HK\\hollow_knight.exe"),
];

describe("matching a downloaded table to a game", () => {
  afterEach(() => localStorage.clear());

  it("uses the exe the table names first", () => {
    expect(matchGame(table({ exe: "DEADSPACE2.EXE", game: "Something" }), GAMES, null)?.id).toBe("ds2");
    expect(matchGame(table({ exe: "bg3.exe" }), GAMES, null)?.id).toBe("bg3");
  });

  it("then the game the player just searched tables for", () => {
    const search = { gameId: "hk", at: 1_000_000 };
    expect(matchGame(table({ modified: 1_000_000 + 60_000, game: "HK v1.5" }), GAMES, search)?.id).toBe("hk");
    // Too long after the search: not counted.
    expect(matchGame(table({ modified: 1_000_000 + SEARCH_WINDOW_MS + 1, game: "HK v1.5" }), GAMES, search)).toBeNull();
  });

  it("then the file name", () => {
    expect(matchGame(table({ game: "Baldurs Gate 3" }), GAMES, null)?.id).toBe("bg3");
    expect(matchGame(table({ game: "DeadSpace2" }), GAMES, null)?.id).toBe("ds2");
    expect(matchGame(table({ game: "Dead Space" }), GAMES, null)?.id).toBe("ds");
    expect(matchGame(table({ game: "Hollow Knight Silksong v1.0.2" }), GAMES, null)?.id).toBe("hk");
    expect(matchGame(table({ game: "Unknown Game" }), GAMES, null)).toBeNull();
    expect(matchGame(table({ game: "HK" }), GAMES, null)).toBeNull();
  });

  it("names the exe only when the table doesn't", () => {
    expect(exeFor(table(), GAMES[3] ?? null)).toBe("hollow_knight.exe");
    expect(exeFor(table(), GAMES[2] ?? null)).toBe("bg3.exe");
    expect(exeFor(table({ exe: "x.exe" }), GAMES[3] ?? null)).toBeUndefined();
    expect(exeFor(table(), null)).toBeUndefined();
  });
});

describe("remembered choices", () => {
  afterEach(() => localStorage.clear());

  it("remembers the last search", () => {
    expect(lastSearch()).toBeNull();
    rememberSearch("hk", 42);
    expect(lastSearch()).toEqual({ gameId: "hk", at: 42 });
    localStorage.setItem("opentrainer.tableSearch", "{broken");
    expect(lastSearch()).toBeNull();
  });

  it("doesn't offer a dismissed table again until the file changes", () => {
    const t = table();
    expect(isDismissed(t)).toBe(false);
    dismiss(t);
    dismiss(t);
    expect(isDismissed(t)).toBe(true);
    expect(isDismissed({ ...t, modified: t.modified + 1 })).toBe(false);
    expect(JSON.parse(localStorage.getItem("opentrainer.tablesDismissed") ?? "[]")).toHaveLength(1);
  });
});
