import type { Wanted } from "./ipc";

/** Letters and digits only, lower case ("Hollow Knight: Voidheart" → "hollowknightvoidheart"). */
export function normName(s: string): string {
  return s.toLowerCase().replace(/[^a-z0-9]/g, "");
}

/** The exe's file name in lower case ("C:\\Games\\hk.exe" → "hk.exe"). */
function exeName(path: string | null | undefined): string | null {
  if (!path) return null;
  return path.split(/[\\/]/).pop()?.toLowerCase() ?? null;
}

/** The request for this game, if players asked for it (by exe, else by name). */
export function wantedFor(game: { name: string; exe?: string | null }, list: Wanted[]): Wanted | null {
  const exe = exeName(game.exe);
  if (exe) {
    const byExe = list.find((w) => w.exe && w.exe.toLowerCase() === exe);
    if (byExe) return byExe;
  }
  const name = normName(game.name);
  return name.length >= 3 ? (list.find((w) => normName(w.game) === name) ?? null) : null;
}

/** "1 player" / "3 players". */
export function players(n: number): string {
  return `${n} player${n === 1 ? "" : "s"}`;
}
