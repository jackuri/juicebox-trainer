import {
  createSdkMcpServer,
  type Options,
  type SDKMessage,
  tool,
} from "@anthropic-ai/claude-agent-sdk";
import { z } from "zod";
import { kickoff, SYSTEM_PROMPT } from "./prompt.js";
import type { AskKind, Outbound, StartMessage } from "./protocol.js";

/** The guarded tools ot-mcp offers (crates/ot-mcp/src/tools.rs). */
export const OT_TOOLS = [
  "list_processes",
  "attach",
  "detach",
  "game_info",
  "scan_first",
  "scan_next",
  "read",
  "write_test",
  "pointer_scan",
  "pointer_check",
  "make_aob",
  "verify_aob_unique",
  "pack_validate",
  "pack_try",
  "cheat_set",
  "revert_all",
  "pack_write",
  "save_snapshot",
  "save_diff",
  "save_read",
] as const;

export const DEFAULT_MODEL = "claude-opus-5-5";
export const DEFAULT_BUDGET_USD = 5;

export interface Io {
  send(m: Outbound): void;
  /** Shows a step to the player and waits for the answer. */
  ask(text: string, kind: AskKind): Promise<string>;
}

/** How to start the guarded tool server. */
export interface McpCommand {
  command: string;
  args: string[];
}

export type QueryFn = (params: { prompt: string; options?: Options }) => AsyncIterable<SDKMessage>;

/**
 * Query options. The agent gets no built-in tools (no shell, files or web):
 * only the guarded ot-mcp tools and ask_player, all pre-approved, and
 * anything else is denied. No user settings, hooks or plugins are loaded.
 */
export function buildOptions(
  start: StartMessage,
  mcp: McpCommand,
  io: Io,
  abort: AbortController,
  appVersion: string,
): Options {
  const askPlayer = tool(
    "ask_player",
    "Show the player one short step and wait. kind 'done': the player presses Done after doing it. " +
      "'yesno': the answer is yes or no. 'text': the player types an answer (e.g. a number they see).",
    { question: z.string(), kind: z.enum(["done", "yesno", "text"]) },
    async ({ question, kind }) => ({
      content: [{ type: "text" as const, text: await io.ask(question, kind) }],
    }),
  );
  const ui = createSdkMcpServer({ name: "ui", version: "1.0.0", tools: [askPlayer] });

  const env: Record<string, string | undefined> = { ...process.env };
  // Settings inherited from another Claude program (if OpenTrainer was
  // started from one) would make the engine use that program's login.
  for (const key of Object.keys(env)) {
    if (/^(CLAUDECODE$|CLAUDE_CODE_|CLAUDE_AGENT_SDK_)/i.test(key)) delete env[key];
  }
  env.CLAUDE_AGENT_SDK_CLIENT_APP = `opentrainer/${appVersion}`;
  if (start.backend !== "apiKey") {
    // Use the player's Claude login, not a key or gateway that happens to be set.
    delete env.ANTHROPIC_API_KEY;
    delete env.ANTHROPIC_BASE_URL;
  }

  return {
    model: start.model ?? DEFAULT_MODEL,
    effort: "high",
    systemPrompt: SYSTEM_PROMPT,
    tools: [],
    mcpServers: {
      ot: { type: "stdio", command: mcp.command, args: mcp.args },
      ui,
    },
    allowedTools: [...OT_TOOLS.map((t) => `mcp__ot__${t}`), "mcp__ui__ask_player"],
    permissionMode: "dontAsk",
    settingSources: [],
    persistSession: false,
    maxTurns: 300,
    maxBudgetUsd: start.maxBudgetUsd ?? DEFAULT_BUDGET_USD,
    abortController: abort,
    env,
  };
}

const ACTIVITY: Record<string, string> = {
  list_processes: "Looking for the game",
  attach: "Connecting to the game",
  detach: "Disconnecting",
  game_info: "Reading game details",
  scan_first: "Searching memory",
  scan_next: "Narrowing the search",
  read: "Reading a value",
  write_test: "Testing a value",
  pointer_scan: "Finding a stable path",
  pointer_check: "Checking paths",
  make_aob: "Checking game code",
  verify_aob_unique: "Checking game code",
  pack_validate: "Checking the cheat pack",
  pack_try: "Loading the cheats",
  cheat_set: "Trying a cheat",
  revert_all: "Undoing tests",
  pack_write: "Saving the cheat pack",
  save_snapshot: "Backing up saves",
  save_diff: "Comparing saves",
  save_read: "Reading saves",
};

/** `mcp__ot__scan_first` → `scan_first`. */
export function shortName(name: string): string {
  return name.replace(/^mcp__[^_]+__/, "");
}

const RESULT_TEXT: Record<string, string> = {
  error_max_turns: "The helper took too many steps and stopped.",
  error_max_budget_usd: "The helper reached its spending limit and stopped.",
  error_during_execution: "The helper hit an error and stopped.",
};

const ERROR_TEXT: Record<string, string> = {
  authentication_failed:
    "Claude isn't signed in. Open Claude Code once and log in (or choose API key in the helper).",
  billing_error: "Your Claude plan or API account can't run this right now (billing).",
  rate_limit: "Claude is busy for your account right now. Try again in a few minutes.",
  oauth_org_not_allowed: "Your Claude organization doesn't allow this use.",
};

/** Runs one helper session to the end. */
export async function runSession(
  start: StartMessage,
  io: Io,
  run: QueryFn,
  mcp: McpCommand,
  abort: AbortController,
  appVersion: string,
): Promise<void> {
  if (start.backend === "apiKey" && !process.env.ANTHROPIC_API_KEY) {
    io.send({
      type: "finished",
      ok: false,
      summary: "No API key: set ANTHROPIC_API_KEY in Windows, or use your Claude login instead.",
    });
    return;
  }
  const options = buildOptions(start, mcp, io, abort, appVersion);
  let finished = false;
  try {
    for await (const m of run({ prompt: kickoff(start), options })) {
      if (m.type === "assistant") {
        if (m.error) {
          io.send({ type: "error", message: ERROR_TEXT[m.error] ?? `Claude reported a problem (${m.error}).` });
        }
        for (const block of m.message.content) {
          if (block.type === "text" && block.text.trim()) {
            io.send({ type: "say", text: block.text.trim() });
          } else if (block.type === "tool_use") {
            const name = shortName(block.name);
            if (name !== "ask_player") {
              io.send({ type: "activity", tool: name, summary: ACTIVITY[name] ?? name });
            }
          }
        }
      } else if (m.type === "result") {
        io.send({ type: "cost", usd: m.total_cost_usd });
        finished = true;
        io.send(
          m.subtype === "success"
            ? { type: "finished", ok: !m.is_error, summary: m.result }
            : { type: "finished", ok: false, summary: RESULT_TEXT[m.subtype] ?? "The helper stopped." },
        );
      }
    }
  } catch (e) {
    if (abort.signal.aborted) {
      io.send({ type: "finished", ok: false, summary: "Cancelled. Every test change was undone." });
      return;
    }
    io.send({ type: "error", message: e instanceof Error ? e.message : String(e) });
  }
  if (!finished) {
    io.send({ type: "finished", ok: false, summary: "The helper stopped before finishing." });
  }
}
