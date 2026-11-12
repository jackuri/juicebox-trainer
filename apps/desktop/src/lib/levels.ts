/**
 * Packer levels (v1.32.0): XP from sharing packs (10 each, once accepted) and rating packs (2 each),
 * counted by the community list's robot. Level 2 takes 10 XP and each next level 10 more than the one
 * before: level L needs 5 × L × (L − 1) XP in all, up to 99 (48,510 XP).
 */
import type { Player } from "./ipc";

export const MAX_LEVEL = 99;
export const XP_PER_PACK = 10;
export const XP_PER_RATING = 2;
/** Daily check-in (v1.33.0): XP a day, and the reward for every so many days in a row. */
export const XP_PER_DAY = 2;
export const STREAK_DAYS = 5;
export const STREAK_XP = 10;

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

// ----- daily check-in (v1.33.0) --------------------------------------------------------------

/** Today (UTC, as GitHub dates check-ins), `YYYY-MM-DD`. */
export const utcToday = (now: Date = new Date()) => now.toISOString().slice(0, 10);
const dayBefore = (d: string) => new Date(Date.parse(`${d}T00:00:00Z`) - 86400000).toISOString().slice(0, 10);

/** Days in a row with today, once checked in today (the XP list may not have today yet). */
export function streakWithToday(p: Player | null, today: string): number {
  if (!p?.lastDay) return 1;
  if (p.lastDay === today) return Math.max(1, p.streak ?? 1);
  if (p.lastDay === dayBefore(today)) return (p.streak ?? 0) + 1;
  return 1;
}

/** The streak reward comes on this day of a run (every 5th). */
export const streakReward = (streak: number) => streak > 0 && streak % STREAK_DAYS === 0;

const ON = "juicebox.checkIn";
const DONE = "juicebox.checkedIn";

/** The player turned the daily check-in on (on this PC). */
export function checkInOn(): boolean {
  try {
    return localStorage.getItem(ON) === "on";
  } catch {
    return false;
  }
}
export function setCheckInOn(on: boolean): void {
  try {
    localStorage.setItem(ON, on ? "on" : "off");
  } catch {
    // Only a setting on this PC.
  }
}

/** Checked in today already, for this GitHub name, from this PC. */
export function checkedIn(login: string, day: string): boolean {
  try {
    return (JSON.parse(localStorage.getItem(DONE) ?? "{}") as Record<string, string>)[login.toLowerCase()] === day;
  } catch {
    return false;
  }
}
export function rememberCheckIn(login: string, day: string): void {
  try {
    const all = JSON.parse(localStorage.getItem(DONE) ?? "{}") as Record<string, string>;
    all[login.toLowerCase()] = day;
    localStorage.setItem(DONE, JSON.stringify(all));
  } catch {
    // Asked again next time: GitHub says it's done already.
  }
}
