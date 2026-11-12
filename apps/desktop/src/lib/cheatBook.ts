/** The cheat book's helpers (v1.28.0): what a cheat does, and the player's edits for saving. */
import type { BookEdit, Cheat } from "./ipc";

/** A cheat the player wants the AI helper to fix. */
export interface AiFix {
  game: string;
  exe: string;
  cheat: string;
  problem: string;
  /** What the cheat is now ("switch: keeps it at 100"). */
  describe: string;
  /** The pack and the cheat (v1.30.0: fixed on the spot when the game runs with this pack loaded). */
  key?: string;
  id?: string;
}

/** "keep" for a switch that keeps a value, "patch" for one that changes the game's code. */
export function kindOf(c: Cheat): "keep" | "patch" | "other" {
  if (c.ui !== "toggle") return "other";
  return c.impl?.kind === "patch" ? "patch" : "keep";
}

/** What the cheat is, in a few plain words. */
export function describeCheat(c: Cheat): string {
  switch (c.ui) {
    case "toggle":
      if (kindOf(c) === "patch") return "switch (changes how the game works)";
      return c.toggle?.value !== undefined ? `switch: keeps it at ${c.toggle.value}` : "switch: keeps it where it is";
    case "slider":
      return c.slider ? `slider ${c.slider.min} to ${c.slider.max}` : "slider";
    case "number":
      return "type a number";
    case "button":
      return c.button?.set !== undefined ? `button: sets it to ${c.button.set}` : `button: adds ${c.button?.add ?? "?"}`;
    default:
      return "cheat";
  }
}

/** The AI helper's request for fixing a cheat. */
export function fixRequest(f: AiFix): { wishes: string[]; notes: string } {
  return {
    wishes: [`Fix my cheat “${f.cheat}”`],
    notes: `The player's pack for ${f.game} (${f.exe}) has a cheat “${f.cheat}” (${f.describe}) that doesn't work. In their words: ${f.problem}. Check it with known_cheats, find why it fails, and save the fixed cheat in their pack (keep its name).`,
  };
}

/** A number typed in a box: "" is "not set", anything else must be a number. */
function parse(name: string, label: string, text: string | undefined): number | undefined | string {
  const t = (text ?? "").trim().replace(/,/g, "");
  if (!t) return undefined;
  const n = Number(t);
  return Number.isFinite(n) ? n : `${name}: “${t}” isn't a number (${label})`;
}

/**
 * The player's edits, ready to save, or what's wrong (a message). `numbers` holds each cheat's
 * number boxes as typed (keepAt, min, max, step, start, set, add).
 */
export function toEdit(
  cheats: Cheat[],
  numbers: Record<string, Record<string, string>>,
  notes: Record<string, string>,
  favourites: string[],
  keys: Record<string, string>,
): BookEdit | string {
  const out: Cheat[] = [];
  for (const c of cheats) {
    const n = numbers[c.id] ?? {};
    const num = (k: string, label: string) => parse(c.name.trim() || "A cheat", label, n[k]);
    const next: Cheat = { ...c };
    if (c.ui === "toggle" && kindOf(c) === "keep") {
      const v = num("keepAt", "keep it at");
      if (typeof v === "string") return v;
      next.toggle = v === undefined ? {} : { value: v };
    }
    if (c.ui === "slider" && c.slider) {
      const [min, max, step, start] = [num("min", "lowest"), num("max", "highest"), num("step", "step"), num("start", "starts at")];
      for (const v of [min, max, step, start]) if (typeof v === "string") return v;
      if (min === undefined || max === undefined || step === undefined) return `${c.name}: a slider needs its lowest, highest and step`;
      next.slider = { ...c.slider, min: min as number, max: max as number, step: step as number, default: start as number | undefined };
      if (next.slider.default === undefined) delete next.slider.default;
    }
    if (c.ui === "number") {
      const [min, max] = [num("min", "lowest"), num("max", "highest")];
      for (const v of [min, max]) if (typeof v === "string") return v;
      next.number = { ...c.number };
      if (min === undefined) delete next.number.min;
      else next.number.min = min as number;
      if (max === undefined) delete next.number.max;
      else next.number.max = max as number;
    }
    if (c.ui === "button" && c.button) {
      const which = c.button.set !== undefined ? "set" : "add";
      const v = num(which, which === "set" ? "sets it to" : "adds");
      if (typeof v === "string") return v;
      if (v === undefined) return `${c.name}: the button needs a number`;
      next.button = { [which]: v };
    }
    out.push(next);
  }
  const ids = new Set(out.map((c) => c.id));
  const keyable = new Set(out.filter((c) => c.ui === "toggle" || c.ui === "button").map((c) => c.id));
  return {
    cheats: out,
    notes: Object.fromEntries(Object.entries(notes).filter(([id, t]) => ids.has(id) && t.trim())),
    favourites: favourites.filter((id) => ids.has(id)),
    keys: Object.fromEntries(Object.entries(keys).filter(([id]) => keyable.has(id)).map(([id, k]) => [id, k.trim()])),
  };
}
