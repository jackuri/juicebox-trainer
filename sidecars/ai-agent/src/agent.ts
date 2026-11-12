import {
  createSdkMcpServer,
  type HookCallback,
  type Options,
  type SDKMessage,
  tool,
} from "@anthropic-ai/claude-agent-sdk";
import { z } from "zod";
import { ANALYSIS_NOTE, EXPLAIN_SAVE_PROMPT, kickoff, LIVE_NOTE, LOCAL_PROMPT, SYSTEM_PROMPT, WEB_PROMPT } from "./prompt.js";
import type { AskKind, Outbound, StartMessage } from "./protocol.js";

/** The guarded tools jb-mcp offers (crates/jb-mcp/src/tools.rs). */
export const JB_TOOLS = [
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
  "game_recon",
  "game_classes",
  "unreal_find",
  "watch_writes_start",
  "watch_writes_stop",
  "pack_heal",
  "game_screenshot",
  "read_struct",
  "known_cheats",
  "note_save",
] as const;

/** A smaller set for local models (v1.21.0): the basic steps only. */
export const LOCAL_TOOLS = [
  "list_processes",
  "attach",
  "game_info",
  "scan_first",
  "scan_next",
  "read",
  "write_test",
  "read_struct",
  "pack_validate",
  "pack_try",
  "cheat_set",
  "revert_all",
  "pack_write",
  "known_cheats",
  "note_save",
] as const;

/** The tools that show the AI a picture of the game window. */
export const PICTURE_TOOLS = ["game_screenshot", "web_screenshot"] as const;

/** The web game tools (v1.20.0), through the app's link to its game window. */
export const WEB_TOOLS = [
  "web_game",
  "web_names",
  "web_find",
  "web_next",
  "web_results",
  "web_read",
  "web_set",
  "web_lock",
  "web_unlock",
  "web_near",
  "web_screenshot",
  "web_cheats_save",
  "known_cheats",
  "note_save",
  "revert_all",
] as const;

/** The read-only save tools the "explain a save" task gets. */
export const EXPLAIN_TOOLS = ["save_diff", "save_read"] as const;

export const DEFAULT_MODEL = "claude-opus-5-5";
/** Ollama's default address; LM Studio uses http://localhost:1234. */
export const DEFAULT_LOCAL_URL = "http://localhost:11434";
export const DEFAULT_BUDGET_USD = 5;

export interface Io {
  send(m: Outbound): void;
  /** Shows a step to the player and waits for the answer. */
  ask(text: string, kind: AskKind): Promise<string>;
  /** Messages the player typed since the last call (oldest first). */
  takeNotes(): string[];
}

/** How the player's messages reach the model. */
export function notesText(notes: string[]): string {
  const quoted = notes.map((n) => `"${n}"`).join("\n");
  return (
    `The player sent you ${notes.length === 1 ? "a message" : "messages"} while you worked ` +
    `(their own words about what they see in the game):\n${quoted}\n` +
    "Take it into account now: say briefly in your next text what you'll do about it, then go on."
  );
}

/** How to start the guarded tool server. */
export interface McpCommand {
  command: string;
  args: string[];
}

export type QueryFn = (params: { prompt: string; options?: Options }) => AsyncIterable<SDKMessage>;

/**
 * Query options. The agent gets no built-in tools (no shell, files or web):
 * only the guarded jb-mcp tools and ask_player, all pre-approved, and
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
  const explain = start.task === "explainSave";
  const web = start.task === "webTrainer";

  const env: Record<string, string | undefined> = { ...process.env };
  // Settings inherited from another Claude program (if JuiceBox Trainer was
  // started from one) would make the engine use that program's login.
  for (const key of Object.keys(env)) {
    if (/^(CLAUDECODE$|CLAUDE_CODE_|CLAUDE_AGENT_SDK_)/i.test(key)) delete env[key];
  }
  env.CLAUDE_AGENT_SDK_CLIENT_APP = `juicebox-trainer/${appVersion}`;
  if (start.backend === "local") {
    // Ollama and LM Studio speak the same Messages API; any token works.
    env.ANTHROPIC_BASE_URL = start.localUrl?.trim() || DEFAULT_LOCAL_URL;
    env.ANTHROPIC_AUTH_TOKEN = "local";
    env.ANTHROPIC_API_KEY = "";
    env.CLAUDE_CODE_ATTRIBUTION_HEADER = "0";
  } else if (start.backend !== "apiKey") {
    // Use the player's Claude login, not a key or gateway that happens to be set.
    delete env.ANTHROPIC_API_KEY;
    delete env.ANTHROPIC_BASE_URL;
  }
  const local = start.backend === "local";
  // After every tool call, and when the helper is about to stop, hand over
  // what the player typed meanwhile.
  const deliver =
    (event: "PostToolUse" | "Stop"): HookCallback =>
    async () => {
      const notes = io.takeNotes();
      return notes.length === 0
        ? {}
        : { hookSpecificOutput: { hookEventName: event, additionalContext: notesText(notes) } };
    };
  // Static analysis reads a PC game's files: not for the other tasks.
  const analysis = start.task !== "explainSave" && start.task !== "webTrainer" && start.analysis ? start.analysis : undefined;
  // The app's link to its game window goes to the tool server only.
  const link =
    web && process.env.JB_WEB_LINK && process.env.JB_WEB_TOKEN
      ? { JB_WEB_LINK: process.env.JB_WEB_LINK, JB_WEB_TOKEN: process.env.JB_WEB_TOKEN }
      : undefined;
  delete env.JB_WEB_LINK;
  delete env.JB_WEB_TOKEN;
  // The tool server lists only this session's tools (smaller context, v1.21.0).
  const pictures = start.pictures !== false;
  const keep = (list: readonly string[]) =>
    list.filter((t) => pictures || !(PICTURE_TOOLS as readonly string[]).includes(t));
  const tools = keep(explain ? EXPLAIN_TOOLS : web ? WEB_TOOLS : local ? LOCAL_TOOLS : JB_TOOLS);
  const serverEnv: Record<string, string> = { ...(link ?? {}), JB_MCP_TOOLS: tools.join(",") };
  // While the player plays (v1.30.0): the tools save into their pack and
  // leave the cheats that are on alone.
  const live = !explain && !web ? start.live : undefined;
  if (live) {
    serverEnv.JB_LIVE_PACK = live.pack;
    serverEnv.JB_LIVE_ON = live.on.map((c) => c.id).join(",");
  }
  const base = local ? LOCAL_PROMPT : analysis ? `${SYSTEM_PROMPT}\n\n${ANALYSIS_NOTE}` : SYSTEM_PROMPT;

  return {
    model: start.model ?? DEFAULT_MODEL,
    // Local servers may not know the effort setting.
    effort: local ? undefined : "high",
    systemPrompt: explain ? EXPLAIN_SAVE_PROMPT : web ? WEB_PROMPT : live ? `${base}\n\n${LIVE_NOTE}` : base,
    tools: [],
    mcpServers: {
      jb: { type: "stdio", command: mcp.command, args: mcp.args, env: serverEnv },
      ui,
      ...(analysis ? { rea: { type: "stdio" as const, command: analysis.command, args: analysis.args } } : {}),
    },
    allowedTools: explain
      ? tools.map((t) => `mcp__jb__${t}`)
      : web
        ? [...tools.map((t) => `mcp__jb__${t}`), "mcp__ui__ask_player"]
        : [
          ...tools.map((t) => `mcp__jb__${t}`),
          "mcp__ui__ask_player",
          // Every tool of the player's analysis server (server-level rule).
          ...(analysis ? ["mcp__rea"] : []),
        ],
    permissionMode: "dontAsk",
    settingSources: [],
    persistSession: false,
    maxTurns: explain ? 20 : web ? 200 : 300,
    maxBudgetUsd: start.maxBudgetUsd ?? DEFAULT_BUDGET_USD,
    abortController: abort,
    env,
    hooks: {
      PostToolUse: [{ hooks: [deliver("PostToolUse")] }],
      Stop: [{ hooks: [deliver("Stop")] }],
    },
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
  game_recon: "Checking the game engine",
  game_classes: "Reading the game's value names",
  unreal_find: "Reading the game's objects (this can take a few minutes)",
  watch_writes_start: "Watching what changes the value",
  watch_writes_stop: "Finding the game code",
  pack_heal: "Repairing the cheat pack",
  web_game: "Looking at the game page",
  web_names: "Looking for values by name",
  web_find: "Searching the game page",
  web_next: "Narrowing the search",
  web_results: "Reading the results",
  web_read: "Reading a value",
  web_set: "Testing a value",
  web_lock: "Testing a lock",
  web_unlock: "Stopping a test lock",
  web_cheats_save: "Saving the cheats",
  web_near: "Looking at the values next to it",
  web_screenshot: "Looking at the game",
  game_screenshot: "Looking at the game",
  read_struct: "Looking at the values next to it",
  known_cheats: "Checking what's already known",
  note_save: "Keeping a note for next time",
};

/** `mcp__jb__scan_first` → `scan_first`. */
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
  if (start.backend === "local" && !start.model?.trim()) {
    io.send({
      type: "finished",
      ok: false,
      summary: "Pick which local model to use (its name in Ollama or LM Studio, e.g. qwen3-coder).",
    });
    return;
  }
  if (start.task === "webTrainer" && (!start.web || !process.env.JB_WEB_LINK)) {
    io.send({
      type: "finished",
      ok: false,
      summary: "The helper couldn't reach the game window. Open the game in Web games, then start again.",
    });
    return;
  }
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
