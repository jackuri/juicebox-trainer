/**
 * Cheat Engine tables the player downloads (v1.15.0): which game each one is for, the game the
 * player last searched tables for, and the tables they said "not now" to.
 */
import type { LibraryGame, RecentTable } from "./ipc";

export const SEARCH_KEY = "opentrainer.tableSearch";
export const DISMISSED_KEY = "opentrainer.tablesDismissed";
const MAX_DISMISSED = 200;
/** A search counts for tables downloaded within this long after it. */
export const SEARCH_WINDOW_MS = 6 * 60 * 60 * 1000;

/** The game the player last looked for tables for, and when. */
export interface TableSearch {
  gameId: string;
  at: number;
}

const base = (p: string) => p.split(/[\\/]/).pop()?.toLowerCase() ?? "";
/** "Baldur's Gate 3" → "baldursgate3". */
const squash = (s: string) => s.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, "");

/** Which of the player's games a downloaded table is for, or null. */
export function matchGame(t: RecentTable, games: LibraryGame[], search: TableSearch | null): LibraryGame | null {
  // 1. The table names the game's exe.
  if (t.exe) {
    const exe = t.exe.toLowerCase();
    const byExe = games.find((g) => (g.exe && base(g.exe) === exe) || g.pack?.exe.toLowerCase() === exe);
    if (byExe) return byExe;
  }
  // 2. Downloaded soon after the player searched tables for a game.
  if (search && t.modified >= search.at - 60_000 && t.modified <= search.at + SEARCH_WINDOW_MS) {
    const searched = games.find((g) => g.id === search.gameId);
    if (searched) return searched;
  }
  // 3. The file name is the game's name.
  const name = squash(t.game);
  if (name.length < 3) return null;
  const exact = games.find((g) => squash(g.name) === name);
  if (exact) return exact;
  const close = games.filter((g) => {
    const n = squash(g.name);
    return n.length >= 4 && name.length >= 4 && (n.includes(name) || name.includes(n));
  });
  // Longest name wins ("Dead Space 2" over "Dead Space" for DeadSpace2.CT).
  return close.sort((a, b) => squash(b.name).length - squash(a.name).length)[0] ?? null;
}

/** The exe to convert a table for: only needed when the table doesn't say. */
export function exeFor(t: RecentTable, game: LibraryGame | null): string | undefined {
  if (t.exe || !game) return undefined;
  const exe = game.exe ? game.exe.split(/[\\/]/).pop() : game.pack?.exe;
  return exe || undefined;
}

function read<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}

function write(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Only remembered for this session.
  }
}

export function lastSearch(): TableSearch | null {
  const s = read<TableSearch | null>(SEARCH_KEY, null);
  return s && typeof s.gameId === "string" && typeof s.at === "number" ? s : null;
}

export function rememberSearch(gameId: string, at = Date.now()) {
  write(SEARCH_KEY, { gameId, at } satisfies TableSearch);
}

const tableKey = (t: RecentTable) => `${t.path}|${t.modified}`;

export function dismissed(): Set<string> {
  const list = read<unknown>(DISMISSED_KEY, []);
  return new Set(Array.isArray(list) ? list.filter((x): x is string => typeof x === "string") : []);
}

/** Don't offer this table again (until the file changes). */
export function dismiss(t: RecentTable) {
  const list = [...dismissed()].filter((k) => k !== tableKey(t));
  list.push(tableKey(t));
  write(DISMISSED_KEY, list.slice(-MAX_DISMISSED));
}

export function isDismissed(t: RecentTable, set = dismissed()): boolean {
  return set.has(tableKey(t));
}
