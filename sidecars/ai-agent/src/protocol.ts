// Messages between the OpenTrainer app and this sidecar: one JSON object per
// line on stdin (app -> sidecar) and stdout (sidecar -> app). Logs go to
// stderr.

export type Backend = "subscription" | "apiKey";

export interface GameRef {
  pid: number;
  name: string;
  exe: string | null;
}

export interface StartMessage {
  type: "start";
  game: GameRef;
  /** Cheats the player asked for, in plain words. */
  wishes: string[];
  notes?: string;
  backend?: Backend;
  model?: string;
  /** Stop when the estimated cost passes this (USD). */
  maxBudgetUsd?: number;
}

export type Inbound =
  | StartMessage
  | { type: "answer"; id: string; text: string }
  | { type: "cancel" };

export type AskKind = "done" | "yesno" | "text";

export type Outbound =
  | { type: "ready"; version: string }
  | { type: "say"; text: string }
  | { type: "ask"; id: string; text: string; kind: AskKind }
  | { type: "activity"; tool: string; summary: string }
  | { type: "cost"; usd: number }
  | { type: "finished"; ok: boolean; summary: string }
  | { type: "error"; message: string };

/** Parses one inbound line; `null` for anything that isn't a valid message. */
export function parseInbound(line: string): Inbound | null {
  let v: unknown;
  try {
    v = JSON.parse(line);
  } catch {
    return null;
  }
  if (typeof v !== "object" || v === null || !("type" in v)) return null;
  const m = v as Record<string, unknown>;
  switch (m.type) {
    case "start": {
      const g = m.game as Record<string, unknown> | undefined;
      if (!g || typeof g.pid !== "number" || typeof g.name !== "string") return null;
      const wishes = Array.isArray(m.wishes)
        ? m.wishes.filter((w): w is string => typeof w === "string" && w.trim() !== "")
        : [];
      return {
        type: "start",
        game: { pid: g.pid, name: g.name, exe: typeof g.exe === "string" ? g.exe : null },
        wishes,
        notes: typeof m.notes === "string" ? m.notes : undefined,
        backend: m.backend === "apiKey" ? "apiKey" : "subscription",
        model: typeof m.model === "string" ? m.model : undefined,
        maxBudgetUsd: typeof m.maxBudgetUsd === "number" ? m.maxBudgetUsd : undefined,
      };
    }
    case "answer":
      return typeof m.id === "string" && typeof m.text === "string"
        ? { type: "answer", id: m.id, text: m.text }
        : null;
    case "cancel":
      return { type: "cancel" };
    default:
      return null;
  }
}

export function encode(m: Outbound): string {
  return JSON.stringify(m) + "\n";
}
