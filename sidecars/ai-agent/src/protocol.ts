// Messages between the OpenTrainer app and this sidecar: one JSON object per
// line on stdin (app -> sidecar) and stdout (sidecar -> app). Logs go to
// stderr.

export type Backend = "subscription" | "apiKey" | "local";

/** An extra MCP server for static analysis (e.g. REA), chosen by the player. */
export interface AnalysisServer {
  command: string;
  args: string[];
}

export interface GameRef {
  pid: number;
  name: string;
  exe: string | null;
}

export type Task = "trainer" | "explainSave";

export interface StartMessage {
  type: "start";
  /** What to do; "trainer" by default. */
  task?: Task;
  /** For "explainSave": the backup made before the player's action. */
  backup?: string;
  game: GameRef;
  /** Cheats the player asked for, in plain words. */
  wishes: string[];
  notes?: string;
  backend?: Backend;
  model?: string;
  /** Stop when the estimated cost passes this (USD). */
  maxBudgetUsd?: number;
  /** For the "local" backend: Ollama / LM Studio address. */
  localUrl?: string;
  /** Optional static-analysis tool server. */
  analysis?: AnalysisServer;
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
        backend: m.backend === "apiKey" ? "apiKey" : m.backend === "local" ? "local" : "subscription",
        model: typeof m.model === "string" ? m.model : undefined,
        maxBudgetUsd: typeof m.maxBudgetUsd === "number" ? m.maxBudgetUsd : undefined,
        task: m.task === "explainSave" ? "explainSave" : "trainer",
        backup: typeof m.backup === "string" ? m.backup : undefined,
        localUrl: typeof m.localUrl === "string" ? m.localUrl : undefined,
        analysis: parseAnalysis(m.analysis),
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

function parseAnalysis(v: unknown): AnalysisServer | undefined {
  if (typeof v !== "object" || v === null) return undefined;
  const a = v as Record<string, unknown>;
  if (typeof a.command !== "string" || !a.command.trim()) return undefined;
  const args = Array.isArray(a.args) ? a.args.filter((x): x is string => typeof x === "string") : [];
  return { command: a.command.trim(), args };
}

export function encode(m: Outbound): string {
  return JSON.stringify(m) + "\n";
}
