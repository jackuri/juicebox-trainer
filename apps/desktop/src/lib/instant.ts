import type { EngineCheatsMade } from "./ipc";

/** "Made 5 cheats. Not found in this game: ammo, money (the AI helper can look for them)." */
export function madeNote(r: EngineCheatsMade): string {
  const parts = [`Made ${r.cheats.length} instant cheat${r.cheats.length === 1 ? "" : "s"}: ${r.cheats.join(", ")}.`];
  if (r.notFound.length) {
    parts.push(`Not found in this game: ${r.notFound.join(", ").toLowerCase()} (the AI helper can look for them).`);
  }
  parts.push("They work in many Unreal games, not every one: try each.");
  return parts.join(" ");
}

/** The exe's file name in lower case. */
export function exeName(path: string | null): string | null {
  return path?.split(/[\\/]/).pop()?.toLowerCase() ?? null;
}
