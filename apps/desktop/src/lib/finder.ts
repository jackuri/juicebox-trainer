import type { Module, ValueType } from "./ipc";

/** Few enough places to try one by one. */
export const TEST_LIMIT = 10;
/** Most places the player can choose to try anyway. */
export const TEST_ANYWAY_LIMIT = 40;

/**
 * A number to write while testing a place: clearly different from `current`
 * and smaller where possible (a health bar may not go above full, but it
 * shows a drop).
 */
export function testValue(current: string, ty: ValueType): string | null {
  const n = Number(current);
  if (!Number.isFinite(n)) return null;
  if (ty === "f32" || ty === "f64") {
    const t = Math.abs(n) >= 2 ? Math.round((n / 2) * 100) / 100 : n + 5;
    return String(t);
  }
  return String(n >= 4 ? Math.floor(n / 2) : n + 5);
}

/** The module an address lies in (a value there is at the same place every start). */
export function staticLocation(address: number, modules: Module[]): { module: string; offset: number } | null {
  const m = modules.find((m) => address >= m.base && address < m.base + m.size);
  return m ? { module: m.name, offset: address - m.base } : null;
}

/** A cheat waiting for the game to be restarted before it's saved. */
export interface PendingCheat {
  exe: string;
  /** The game's process when it was found (a restart gives a new one). */
  pid: number;
  gameName: string;
  name: string;
  ty: ValueType;
  value: string;
  keep: boolean;
}

const PENDING_KEY = "ot.findValue.pending";

export function loadPending(exe: string): PendingCheat | null {
  try {
    const p = JSON.parse(localStorage.getItem(PENDING_KEY) ?? "null") as PendingCheat | null;
    return p && p.exe.toLowerCase() === exe.toLowerCase() ? p : null;
  } catch {
    return null;
  }
}

export function savePending(p: PendingCheat | null) {
  try {
    if (p) localStorage.setItem(PENDING_KEY, JSON.stringify(p));
    else localStorage.removeItem(PENDING_KEY);
  } catch {
    // Without storage the player just saves it again next time.
  }
}

/** "Gold" from a typed name, or a sensible default. */
export function cheatName(name: string): string {
  const t = name.trim().replace(/\s+/g, " ");
  return t ? t.slice(0, 60) : "My value";
}
