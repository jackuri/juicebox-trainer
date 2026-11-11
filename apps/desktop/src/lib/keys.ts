/** Turning key presses and controller buttons into the text jb-packs reads. */

const NUMPAD: Record<string, string> = {
  NumpadAdd: "Num+",
  NumpadSubtract: "Num-",
  NumpadMultiply: "Num*",
  NumpadDivide: "Num/",
  NumpadDecimal: "Num.",
};

const NAMED: Record<string, string> = {
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  PageUp: "PageUp",
  PageDown: "PageDown",
  Home: "Home",
  End: "End",
  Insert: "Insert",
  Delete: "Delete",
  Pause: "Pause",
  Space: "Space",
};

interface KeyLike {
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

/** `Ctrl+F5`, `Num1`… from a key press, or null for a lone modifier or an unsupported key. */
export function keyFromEvent(e: KeyLike): string | null {
  const c = e.code;
  let key: string | null = null;
  if (/^F([1-9]|1\d|2[0-4])$/.test(c)) key = c;
  else if (/^Numpad\d$/.test(c)) key = `Num${c.slice(6)}`;
  else if (NUMPAD[c]) key = NUMPAD[c];
  else if (/^Key[A-Z]$/.test(c)) key = c.slice(3);
  else if (/^Digit\d$/.test(c)) key = c.slice(5);
  else if (NAMED[c]) key = NAMED[c];
  if (!key) return null;
  return [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", key].filter(Boolean).join("+");
}

/** Standard-mapping gamepad button index → XInput name. */
const PAD_BUTTONS = [
  "A", "B", "X", "Y", "LB", "RB", "LT", "RT", "Back", "Start", "LS", "RS",
  "DPadUp", "DPadDown", "DPadLeft", "DPadRight",
];

/** Display order (matches jb-packs' canonical order). */
const ORDER = ["LB", "RB", "LT", "RT", "LS", "RS", "Back", "Start", "A", "B", "X", "Y", "DPadUp", "DPadDown", "DPadLeft", "DPadRight"];

/** A chord from pressed button indexes, e.g. [4, 5, 12] → "LB+RB+DPadUp". */
export function chordFromButtons(pressed: Iterable<number>): string {
  const names = new Set<string>();
  for (const i of pressed) {
    const n = PAD_BUTTONS[i];
    if (n) names.add(n);
  }
  return ORDER.filter((n) => names.has(n)).join("+");
}

/** Buttons held on any connected controller right now. */
export function heldButtons(): number[] {
  const pads = typeof navigator !== "undefined" && navigator.getGamepads ? navigator.getGamepads() : [];
  const held = new Set<number>();
  for (const p of pads) {
    if (!p) continue;
    p.buttons.forEach((b, i) => {
      if (b.pressed || b.value > 0.5) held.add(i);
    });
  }
  return [...held];
}
