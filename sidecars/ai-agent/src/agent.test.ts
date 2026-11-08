import type { SDKMessage } from "@anthropic-ai/claude-agent-sdk";
import { afterEach, describe, expect, it, vi } from "vitest";
import { buildOptions, OT_TOOLS, type QueryFn, runSession, shortName } from "./agent.js";
import { AskBridge, NoteBox } from "./bridge.js";
import { kickoff, SYSTEM_PROMPT } from "./prompt.js";
import { encode, MAX_NOTE, type Outbound, parseInbound, type StartMessage } from "./protocol.js";

const start: StartMessage = {
  type: "start",
  game: { pid: 42, name: "testbed_native.exe", exe: "C:\\Games\\testbed_native.exe" },
  wishes: ["Infinite health", "Lots of gold"],
  backend: "subscription",
};
const mcp = { command: "C:\\OpenTrainer\\opentrainer-desktop.exe", args: ["--mcp"] };

function io() {
  const sent: Outbound[] = [];
  const notes = new NoteBox();
  return {
    sent,
    notes,
    send: (m: Outbound) => sent.push(m),
    ask: vi.fn().mockResolvedValue("Done"),
    takeNotes: () => notes.take(),
  };
}

afterEach(() => {
  delete process.env.ANTHROPIC_API_KEY;
});

describe("protocol", () => {
  it("parses inbound messages and rejects junk", () => {
    const m = parseInbound(
      JSON.stringify({ type: "start", game: { pid: 1, name: "g.exe" }, wishes: ["a", "", 3] }),
    );
    expect(m).toEqual({
      type: "start",
      game: { pid: 1, name: "g.exe", exe: null },
      wishes: ["a"],
      notes: undefined,
      backend: "subscription",
      model: undefined,
      maxBudgetUsd: undefined,
      task: "trainer",
      backup: undefined,
    });
    expect(parseInbound('{"type":"answer","id":"q1","text":"yes"}')).toEqual({
      type: "answer",
      id: "q1",
      text: "yes",
    });
    expect(parseInbound('{"type":"cancel"}')).toEqual({ type: "cancel" });
    expect(parseInbound("not json")).toBeNull();
    expect(parseInbound('{"type":"start","game":{}}')).toBeNull();
    expect(parseInbound('{"type":"rm -rf"}')).toBeNull();
    expect(encode({ type: "say", text: "hi" })).toBe('{"type":"say","text":"hi"}\n');
  });
});

describe("prompt", () => {
  it("names the game and wishes, and keeps the rules", () => {
    const k = kickoff(start);
    expect(k).toContain("pid 42");
    expect(k).toContain("- Lots of gold");
    expect(SYSTEM_PROMPT).toContain("Single-player");
    expect(SYSTEM_PROMPT).toContain("anti-cheat guard blocked");
    expect(SYSTEM_PROMPT).toContain("never ask the player to turn off security software");
  });
});

describe("options", () => {
  it("gives the agent only the guarded tools and denies the rest", () => {
    process.env.ANTHROPIC_API_KEY = "test-key-not-real";
    const o = buildOptions(start, mcp, io(), new AbortController(), "0.8.0");
    expect(o.tools).toEqual([]);
    expect(o.permissionMode).toBe("dontAsk");
    expect(o.settingSources).toEqual([]);
    expect(o.allowedTools).toHaveLength(OT_TOOLS.length + 1);
    for (const t of o.allowedTools ?? []) {
      expect(t).toMatch(/^mcp__(ot|ui)__/);
    }
    expect(o.mcpServers?.ot).toEqual({ type: "stdio", command: mcp.command, args: ["--mcp"] });
    expect(o.model).toBe("claude-opus-5-5");
    expect(o.maxBudgetUsd).toBe(5);
    // The subscription backend never uses a stray API key.
    expect(o.env?.ANTHROPIC_API_KEY).toBeUndefined();
    expect(o.env?.CLAUDE_AGENT_SDK_CLIENT_APP).toBe("opentrainer/0.8.0");
    process.env.CLAUDE_CODE_ENTRYPOINT = "other-app";
    process.env.CLAUDECODE = "1";
    const clean = buildOptions(start, mcp, io(), new AbortController(), "0.8.0");
    expect(clean.env?.CLAUDE_CODE_ENTRYPOINT).toBeUndefined();
    expect(clean.env?.CLAUDECODE).toBeUndefined();
    delete process.env.CLAUDE_CODE_ENTRYPOINT;
    delete process.env.CLAUDECODE;
    const withKey = buildOptions({ ...start, backend: "apiKey" }, mcp, io(), new AbortController(), "0.8.0");
    expect(withKey.env?.ANTHROPIC_API_KEY).toBe("test-key-not-real");
  });
});

function fakeQuery(messages: unknown[]): QueryFn {
  return async function* () {
    for (const m of messages) yield m as SDKMessage;
  };
}

describe("local models and analysis tools", () => {
  it("points the engine at a local server with the chosen model", () => {
    const o = buildOptions(
      { ...start, backend: "local", model: "qwen3-coder", localUrl: "http://localhost:1234" },
      mcp,
      io(),
      new AbortController(),
      "0.9.1",
    );
    expect(o.model).toBe("qwen3-coder");
    expect(o.env?.ANTHROPIC_BASE_URL).toBe("http://localhost:1234");
    expect(o.env?.ANTHROPIC_AUTH_TOKEN).toBe("local");
    expect(o.env?.ANTHROPIC_API_KEY).toBe("");
    expect(o.effort).toBeUndefined();
    const dflt = buildOptions({ ...start, backend: "local", model: "m" }, mcp, io(), new AbortController(), "0.9.1");
    expect(dflt.env?.ANTHROPIC_BASE_URL).toBe("http://localhost:11434");
  });

  it("needs a model name for local models", async () => {
    const x = io();
    await runSession({ ...start, backend: "local" }, x, fakeQuery([]), mcp, new AbortController(), "0.9.1");
    expect((x.sent[0] as { summary: string }).summary).toContain("local model");
  });

  it("adds the player's analysis server and allows only its tools", () => {
    const analysis = { command: "C:\\Tools\\rea.exe", args: ["--stdio"] };
    const o = buildOptions({ ...start, analysis }, mcp, io(), new AbortController(), "0.9.1");
    expect(o.mcpServers?.rea).toEqual({ type: "stdio", command: analysis.command, args: ["--stdio"] });
    expect(o.allowedTools).toContain("mcp__rea");
    expect(String(o.systemPrompt)).toContain("mcp__rea__");
    // Not for explaining saves, and not without one.
    const e = buildOptions({ ...start, analysis, task: "explainSave" }, mcp, io(), new AbortController(), "0.9.1");
    expect(e.mcpServers?.rea).toBeUndefined();
    expect(buildOptions(start, mcp, io(), new AbortController(), "0.9.1").allowedTools).not.toContain("mcp__rea");
    expect(
      parseInbound(JSON.stringify({ type: "start", game: { pid: 1, name: "g" }, analysis: { command: " " } })),
    ).toMatchObject({ analysis: undefined });
  });
});

describe("explain a save", () => {
  it("gets only the read-only save tools and its own prompt", () => {
    const o = buildOptions(
      { ...start, task: "explainSave", backup: "1700000000000" },
      mcp,
      io(),
      new AbortController(),
      "0.9.0",
    );
    expect(o.allowedTools).toEqual(["mcp__ot__save_diff", "mcp__ot__save_read"]);
    expect(String(o.systemPrompt)).toContain("what changed in the save");
    expect(kickoff({ ...start, task: "explainSave", backup: "17", notes: "bought a sword" })).toContain(
      "bought a sword",
    );
  });

  it("parses the task", () => {
    const m = parseInbound(
      JSON.stringify({ type: "start", task: "explainSave", backup: "9", game: { pid: 0, name: "g.exe", exe: "C:/g.exe" } }),
    );
    expect(m).toMatchObject({ task: "explainSave", backup: "9" });
  });
});

describe("runSession", () => {
  it("relays text, tool activity and the result", async () => {
    const x = io();
    const run = fakeQuery([
      {
        type: "assistant",
        message: {
          content: [
            { type: "text", text: "Connecting to your game." },
            { type: "tool_use", id: "t1", name: "mcp__ot__attach", input: { pid: 42 } },
            { type: "tool_use", id: "t2", name: "mcp__ui__ask_player", input: {} },
          ],
        },
      },
      { type: "result", subtype: "success", is_error: false, result: "Pack saved.", total_cost_usd: 0.42 },
    ]);
    await runSession(start, x, run, mcp, new AbortController(), "0.8.0");
    expect(x.sent).toEqual([
      { type: "say", text: "Connecting to your game." },
      { type: "activity", tool: "attach", summary: "Connecting to the game" },
      { type: "cost", usd: 0.42 },
      { type: "finished", ok: true, summary: "Pack saved." },
    ]);
  });

  it("explains sign-in problems, limits and missing keys", async () => {
    const x = io();
    await runSession(
      start,
      x,
      fakeQuery([
        { type: "assistant", error: "authentication_failed", message: { content: [] } },
        { type: "result", subtype: "error_max_budget_usd", total_cost_usd: 5 },
      ]),
      mcp,
      new AbortController(),
      "0.8.0",
    );
    expect(x.sent[0]).toMatchObject({ type: "error" });
    expect((x.sent[0] as { message: string }).message).toContain("log in");
    expect(x.sent.at(-1)).toMatchObject({ type: "finished", ok: false });
    expect((x.sent.at(-1) as { summary: string }).summary).toContain("spending limit");

    const y = io();
    await runSession({ ...start, backend: "apiKey" }, y, fakeQuery([]), mcp, new AbortController(), "0.8.0");
    expect(y.sent).toHaveLength(1);
    expect((y.sent[0] as { summary: string }).summary).toContain("ANTHROPIC_API_KEY");
  });

  it("reports a cancel", async () => {
    const x = io();
    const abort = new AbortController();
    const run: QueryFn = async function* () {
      abort.abort();
      throw new Error("aborted");
    };
    await runSession(start, x, run, mcp, abort, "0.8.0");
    expect(x.sent).toEqual([
      { type: "finished", ok: false, summary: "Cancelled. Every test change was undone." },
    ]);
  });

  it("shortens tool names", () => {
    expect(shortName("mcp__ot__scan_first")).toBe("scan_first");
    expect(shortName("other")).toBe("other");
  });
});

describe("AskBridge", () => {
  it("matches answers to questions and cancels the rest", async () => {
    const sent: Outbound[] = [];
    const b = new AskBridge((m) => sent.push(m));
    const a = b.ask("Note your gold.", "done");
    const c = b.ask("Is it 12345?", "yesno");
    expect(sent).toEqual([
      { type: "ask", id: "q1", text: "Note your gold.", kind: "done" },
      { type: "ask", id: "q2", text: "Is it 12345?", kind: "yesno" },
    ]);
    expect(b.answer("q1", "Done")).toBe(true);
    expect(b.answer("q1", "again")).toBe(false);
    expect(await a).toBe("Done");
    b.cancelAll();
    expect(await c).toBe("The player cancelled.");
    expect(b.waiting).toBe(0);
  });
});

describe("player messages", () => {
  it("parses notes, trimmed and capped", () => {
    expect(parseInbound(JSON.stringify({ type: "note", text: "  only the pistol  " }))).toEqual({
      type: "note",
      text: "only the pistol",
    });
    expect(parseInbound(JSON.stringify({ type: "note", text: "   " }))).toBeNull();
    expect(parseInbound(JSON.stringify({ type: "note", text: 3 }))).toBeNull();
    const long = parseInbound(JSON.stringify({ type: "note", text: "x".repeat(MAX_NOTE + 50) }));
    expect(long?.type === "note" && long.text.length).toBe(MAX_NOTE);
  });

  it("hands what the player typed to the helper after a tool call, once", async () => {
    const x = io();
    const o = buildOptions(start, mcp, x, new AbortController(), "1.3.0");
    const post = o.hooks?.PostToolUse?.[0]?.hooks[0];
    const stop = o.hooks?.Stop?.[0]?.hooks[0];
    expect(post && stop).toBeTruthy();
    const signal = new AbortController().signal;
    const input = { hook_event_name: "PostToolUse" } as never;
    expect(await post!(input, "t1", { signal })).toEqual({});
    x.notes.push("Yes, but fire damage still hurts");
    x.notes.push("ammo works with one gun only");
    const out = (await post!(input, "t2", { signal })) as {
      hookSpecificOutput: { hookEventName: string; additionalContext: string };
    };
    expect(out.hookSpecificOutput.hookEventName).toBe("PostToolUse");
    expect(out.hookSpecificOutput.additionalContext).toContain('"Yes, but fire damage still hurts"');
    expect(out.hookSpecificOutput.additionalContext).toContain('"ammo works with one gun only"');
    expect(await post!(input, "t3", { signal })).toEqual({});
    // A message sent just as the helper is finishing keeps it going.
    x.notes.push("wait, gold doesn't save");
    const s = (await stop!({ hook_event_name: "Stop" } as never, undefined, { signal })) as {
      hookSpecificOutput: { hookEventName: string; additionalContext: string };
    };
    expect(s.hookSpecificOutput).toMatchObject({ hookEventName: "Stop" });
    expect(s.hookSpecificOutput.additionalContext).toContain("gold doesn't save");
  });

  it("tells the helper about typed answers and messages", () => {
    expect(SYSTEM_PROMPT).toContain("The player sent you a message");
  });
});
