// JuiceBox Trainer AI helper sidecar. The app starts it with:
//   JB_MCP_COMMAND  the guarded tool server (juicebox-trainer.exe or jb-mcp.exe)
//   JB_MCP_ARGS     its arguments as a JSON array, e.g. ["--mcp"]
//   JB_APP_VERSION  the app version (for the User-Agent)
// and talks to it with JSON lines (see protocol.ts).

import { createInterface } from "node:readline";
import { query } from "@anthropic-ai/claude-agent-sdk";
import { type McpCommand, runSession } from "./agent.js";
import { AskBridge, NoteBox } from "./bridge.js";
import { encode, type Outbound, parseInbound } from "./protocol.js";

function send(m: Outbound): void {
  process.stdout.write(encode(m));
}

function mcpCommand(): McpCommand | null {
  const command = process.env.JB_MCP_COMMAND;
  if (!command) return null;
  let args: string[] = [];
  try {
    const parsed: unknown = JSON.parse(process.env.JB_MCP_ARGS ?? "[]");
    if (Array.isArray(parsed)) args = parsed.filter((a): a is string => typeof a === "string");
  } catch {
    // No arguments.
  }
  return { command, args };
}

const version = process.env.JB_APP_VERSION ?? "dev";
const bridge = new AskBridge(send);
const notes = new NoteBox();
const abort = new AbortController();
let running = false;

send({ type: "ready", version });

const lines = createInterface({ input: process.stdin });
lines.on("line", (line) => {
  const m = parseInbound(line);
  if (!m) return;
  if (m.type === "answer") {
    bridge.answer(m.id, m.text);
  } else if (m.type === "note") {
    notes.push(m.text);
  } else if (m.type === "cancel") {
    abort.abort();
    bridge.cancelAll();
  } else if (m.type === "start" && !running) {
    const mcp = mcpCommand();
    if (!mcp) {
      send({ type: "finished", ok: false, summary: "The app didn't say how to start its tools (JB_MCP_COMMAND)." });
      return;
    }
    running = true;
    void runSession(m, { send, ask: (t, k) => bridge.ask(t, k), takeNotes: () => notes.take() }, query, mcp, abort, version).finally(() =>
      process.exit(0),
    );
  }
});
lines.on("close", () => {
  // The app went away: stop (the tool server undoes its changes on exit).
  abort.abort();
  bridge.cancelAll();
  if (!running) process.exit(0);
});
