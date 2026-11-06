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
