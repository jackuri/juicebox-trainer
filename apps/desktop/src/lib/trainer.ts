import type { Cheat } from "./ipc";

/** Rounds to the step's decimal places, e.g. step 0.1 → "2.5". */
export function formatStep(v: number, step: number): string {
  const decimals = (String(step).split(".")[1] ?? "").length;
  return v.toFixed(decimals);
}

export function groupByCategory(cheats: Cheat[]): [string, Cheat[]][] {
  const groups = new Map<string, Cheat[]>();
  for (const c of cheats) {
    const key = c.category?.trim() || "General";
    groups.set(key, [...(groups.get(key) ?? []), c]);
  }
  return [...groups.entries()];
}

/** A pointer path as the pointer scanner gives it. */
export interface PathParts {
  module: string;
  moduleOffset: number;
  offsets: number[];
}

/**
 * A pack cheat (PLAN §4, `value` op) for a pointer path, as pretty JSON ready to
 * paste into a pack's `cheats` list. `[[game.exe+0x10]+0x18]+0x40` becomes
 * `base: "game.exe+0x10", offsets: ["0x18", "0x40"]`.
 */
export function pointerCheatJson(path: PathParts, ty: string): string {
  const h = (n: number) => (n < 0 ? `-0x${(-n).toString(16).toUpperCase()}` : `0x${n.toString(16).toUpperCase()}`);
  const cheat = {
    id: "new_value",
    name: "New value",
    ui: "number",
    impl: {
      kind: "value",
      type: ty,
      address: {
        base: `${path.module}+${h(path.moduleOffset)}`,
        ...(path.offsets.length > 0 ? { offsets: path.offsets.map(h) } : {}),
      },
    },
  };
  return JSON.stringify(cheat, null, 2);
}

/**
 * Games (PIDs) that had a pack loaded at some point, by hand or automatically. Kept outside the
 * Trainer panel so that after "Unload pack" the pack isn't loaded again when the player comes back
 * from another tab.
 */
export const chosenPacks = new Set<number>();
