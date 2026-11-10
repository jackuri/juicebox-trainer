/** Helpers for a web game's cheats (v1.19.0). */
import type { WebCheat, WebKey, WebTarget } from "./ipc";

/** Where a cheat is, in words. */
export const where = (t: WebTarget) => (t.k === "m" ? `memory ${t.mem + 1} at 0x${t.off.toString(16).toUpperCase()} (${t.t})` : t.path.join("."));
/** How a cheat's lock is named in the game page (also by its key). */
export const lockId = (c: WebCheat) => `${c.name}|${where(c.target)}`;

/** The cheats' keys for the helper in the game page, and a tag that changes when they do. */
export function keysFor(cheats: WebCheat[]): { list: WebKey[]; tag: string } {
  const list: WebKey[] = cheats
    .filter((c) => c.key)
    .map((c) => ({ id: lockId(c), name: c.name, key: c.key!, does: c.keyDoes === "set" ? "set" : "lock", target: c.target, value: c.value ?? null }));
  const text = JSON.stringify(list);
  let h = 0;
  for (let i = 0; i < text.length; i++) h = (Math.imul(h, 31) + text.charCodeAt(i)) | 0;
  return { list, tag: list.length ? `${list.length}-${(h >>> 0).toString(36)}` : "" };
}
