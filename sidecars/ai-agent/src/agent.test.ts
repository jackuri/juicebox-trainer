import type { SDKMessage } from "@anthropic-ai/claude-agent-sdk";
import { afterEach, describe, expect, it, vi } from "vitest";
import { buildOptions, OT_TOOLS, type QueryFn, runSession, shortName } from "./agent.js";
import { AskBridge } from "./bridge.js";
import { kickoff, SYSTEM_PROMPT } from "./prompt.js";
import { encode, type Outbound, parseInbound, type StartMessage } from "./protocol.js";

const start: StartMessage = {
  type: "start",
  game: { pid: 42, name: "testbed_native.exe", exe: "C:\\Games\\testbed_native.exe" },
  wishes: ["Infinite health", "Lots of gold"],
  backend: "subscription",
};
const mcp = { command: "C:\\OpenTrainer\\opentrainer-desktop.exe", args: ["--mcp"] };

function io() {
  const sent: Outbound[] = [];
  return { sent, send: (m: Outbound) => sent.push(m), ask: vi.fn().mockResolvedValue("Done") };
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
