/**
 * Packer levels (v1.32.0): XP from sharing packs (10 each, once accepted) and rating packs (2 each),
 * counted by the community list's robot. Level 2 takes 10 XP and each next level 10 more than the one
 * before: level L needs 5 × L × (L − 1) XP in all, up to 99 (48,510 XP).
 */
import type { Player } from "./ipc";

export const MAX_LEVEL = 99;
export const XP_PER_PACK = 10;
export const XP_PER_RATING = 2;

/** Total XP to reach a level. */
export const xpFor = (level: number) => 5 * level * (level - 1);

export function levelFor(xp: number): number {
  let level = 1;
  while (level < MAX_LEVEL && xp >= xpFor(level + 1)) level++;
  return level;
}

/** Titles and the level each starts at (the same as the community list's README). */
export const TITLES: readonly (readonly [number, string])[] = [
  [1, "Recruit Packer"],
  [3, "Pack Rat"],
  [5, "Intern Packer"],
  [10, "Pro Packer"],
  [20, "Pack Master"],
  [30, "Leader of the Pack"],
  [45, "Pack King"],
  [60, "Pack Legend"],
  [99, "Pack God"],
];

export function titleFor(level: number): string {
  return [...TITLES].reverse().find(([at]) => level >= at)?.[1] ?? TITLES[0]![1];
}

/** The next title and the level it starts at, or null at the top. */
export function nextTitle(level: number): { level: number; title: string } | null {
  const t = TITLES.find(([at]) => at > level);
  return t ? { level: t[0], title: t[1] } : null;
}

/** Where a player stands: level, title, and XP into this level out of what it takes. */
export function standing(xp: number) {
  const level = levelFor(Math.max(0, xp));
  const from = xpFor(level);
  const max = level >= MAX_LEVEL;
  const need = max ? 0 : xpFor(level + 1) - from;
  return { level, title: titleFor(level), into: xp - from, need, max };
}

/** A player's row in the XP list (GitHub names compare without case). */
export function findPlayer(list: Player[], login: string | null | undefined): Player | null {
  if (!login) return null;
  const l = login.toLowerCase();
  return list.find((p) => p.login.toLowerCase() === l) ?? null;
}

const KEY = "juicebox.packerLevel";

/**
 * The level last seen for this GitHub name on this PC. Returns the level to celebrate if it went up
 * since (none the first time), and remembers the new one.
 */
export function levelUp(login: string, level: number): number | null {
  try {
    const seen = JSON.parse(localStorage.getItem(KEY) ?? "{}") as Record<string, number>;
    const was = seen[login.toLowerCase()];
    seen[login.toLowerCase()] = level;
    localStorage.setItem(KEY, JSON.stringify(seen));
    return typeof was === "number" && level > was ? level : null;
  } catch {
    return null;
  }
}
