/** Helpers for a web game's cheats (v1.19.0). */
import type { Keep, WebCheat, WebKey, WebTarget } from "./ipc";

const hex = (n: number) => `0x${n.toString(16).toUpperCase()}`;
/** Where a cheat is, in words. */
export const where = (t: WebTarget) => {
  if (t.k !== "m") return t.path.join(".");
  const at = `memory ${t.mem + 1} at ${hex(t.off)}`;
  return t.count && t.count > 1 && t.every ? `${at}, ${t.count} places ${hex(t.every)} apart (${t.t})` : `${at} (${t.t})`;
};

/** How a lock keeps its number, in words ("never above 1"). */
export function keepWords(keep: Keep | null | undefined, value: number | null | undefined): string | null {
  if (!keep || keep === "at" || value == null) return null;
  return `${keep === "atMost" ? "never above" : "never below"} ${value}`;
}

/** The most places one group cheat may cover (as the app checks). */
export const MAX_GROUP = 1024;
/** The biggest item of a list put together by hand (64 KiB). */
const MAX_ITEM = 0x10000;

const gcd = (a: number, b: number): number => (b === 0 ? a : gcd(b, a % b));

/**
 * Several cheats for the same kind of thing (every zombie's health) as one cheat that covers all
 * their places (v1.31.0), or why they can't be. In a memory they must be the same kind of number,
 * evenly spaced (with at most a few unknown slots between them); in JavaScript their paths may
 * differ in one part only, which becomes "*" (every item there).
 */
export function combine(list: WebCheat[]): { target: WebTarget; places: number } | { why: string } {
  if (list.length < 2) return { why: "Tick two or more cheats to put together." };
  const ts = list.map((c) => c.target);
  if (ts.every((t) => t.k === "m")) {
    const ms = ts as Extract<WebTarget, { k: "m" }>[];
    const first = ms[0]!;
    if (ms.some((t) => t.mem !== first.mem || t.t !== first.t)) return { why: "They have to be the same kind of number, in the same game memory." };
    const offs = [...new Set(ms.flatMap((t) => Array.from({ length: t.count && t.count > 1 ? t.count : 1 }, (_, i) => t.off + i * (t.every ?? 0))))].sort((a, b) => a - b);
    if (offs.length < 2) return { why: "They're all the same place." };
    const start = offs[0]!;
    const every = offs.slice(1).reduce((g, o) => gcd(g, o - start), 0);
    const count = (offs[offs.length - 1]! - start) / every + 1;
    // A list keeps its items close together; far apart, two places are just two places.
    if (every > MAX_ITEM || count > MAX_GROUP || count > offs.length * 4) return { why: "They aren't evenly spaced close together in the game's memory, so they don't look like one list." };
    return { target: { k: "m", mem: first.mem, label: first.label, size: first.size, off: start, t: first.t, every, count }, places: count };
  }
  if (ts.every((t) => t.k === "j")) {
    const ps = (ts as Extract<WebTarget, { k: "j" }>[]).map((t) => t.path);
    const len = ps[0]!.length;
    if (ps.some((p) => p.length !== len)) return { why: "Their places in the game are too different to put together." };
    const differ = Array.from({ length: len }, (_, i) => i).filter((i) => ps.some((p) => p[i] !== ps[0]![i]));
    if (differ.length !== 1 || differ[0] === 0 || differ[0] === len - 1) return { why: "Their places in the game are too different to put together." };
    const path = ps[0]!.map((p, i) => (i === differ[0] ? "*" : p));
    return { target: { k: "j", path }, places: ps.length };
  }
  return { why: "Some are in the game's memory and some in its JavaScript: they can't be put together." };
}

/** A name for the combined cheat: the first one's, without its number ("Weak enemy 1 (1 HP)" → "Weak enemy (1 HP)"). */
export const combinedName = (name: string) => name.replace(/\s+#?\d+(?=\s*(\(|$))/, "").trim() || name;
/** How a cheat's lock is named in the game page (also by its key). */
export const lockId = (c: WebCheat) => `${c.name}|${where(c.target)}`;

/** The cheats' keys for the helper in the game page, and a tag that changes when they do. */
export function keysFor(cheats: WebCheat[]): { list: WebKey[]; tag: string } {
  const list: WebKey[] = cheats
    .filter((c) => c.key)
    .map((c) => ({ id: lockId(c), name: c.name, key: c.key!, does: c.keyDoes === "set" ? "set" : "lock", target: c.target, value: c.value ?? null, how: c.keep ?? "at" }));
  const text = JSON.stringify(list);
  let h = 0;
  for (let i = 0; i < text.length; i++) h = (Math.imul(h, 31) + text.charCodeAt(i)) | 0;
  return { list, tag: list.length ? `${list.length}-${(h >>> 0).toString(36)}` : "" };
}
