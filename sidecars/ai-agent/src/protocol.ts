// Messages between the JuiceBox Trainer app and this sidecar: one JSON object per
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

export type Task = "trainer" | "explainSave" | "webTrainer";

/** For "webTrainer" (v1.20.0): the web game open in JuiceBox Trainer's game window. */
export interface WebGameRef {
  id: string;
  url: string;
}

export interface StartMessage {
  type: "start";
  /** What to do; "trainer" by default. */
  task?: Task;
  /** For "explainSave": the backup made before the player's action. */
  backup?: string;
  game: GameRef;
  /** For "webTrainer": the web game (its name is game.name). */
  web?: WebGameRef;
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
  /** The helper may look at the game window (v1.21.0; on unless false). */
  pictures?: boolean;
}

export type Inbound =
  | StartMessage
  | { type: "answer"; id: string; text: string }
  /** A message the player typed while the helper works (not an answer). */
  | { type: "note"; text: string }
  | { type: "cancel" };

/** Longest player message passed on, in characters. */
export const MAX_NOTE = 2000;

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
      const task: Task = m.task === "explainSave" ? "explainSave" : m.task === "webTrainer" ? "webTrainer" : "trainer";
      const web =
        task === "webTrainer" && typeof g.url === "string" && typeof g.webId === "string"
          ? { id: g.webId, url: g.url }
          : undefined;
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
        task,
        web,
        backup: typeof m.backup === "string" ? m.backup : undefined,
        localUrl: typeof m.localUrl === "string" ? m.localUrl : undefined,
        analysis: parseAnalysis(m.analysis),
        pictures: m.pictures === false ? false : undefined,
      };
    }
    case "answer":
      return typeof m.id === "string" && typeof m.text === "string"
        ? { type: "answer", id: m.id, text: m.text }
        : null;
    case "note": {
      if (typeof m.text !== "string") return null;
      const text = m.text.trim().slice(0, MAX_NOTE);
      return text ? { type: "note", text } : null;
    }
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
