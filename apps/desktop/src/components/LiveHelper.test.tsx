import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AiEvent } from "../lib/ipc";
import { LiveHelper } from "./LiveHelper";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
let emit: ((e: AiEvent) => void) | null = null;
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, cb: (e: { payload: AiEvent }) => void) => {
    if (name === "ai-event") emit = (e) => cb({ payload: e });
    return Promise.resolve(() => undefined);
  },
}));

const ready = { available: true, problem: null, running: false, transcript: [], signedIn: true, download: "notNeeded", installing: false };

function send(e: AiEvent) {
  act(() => emit?.(e));
}

beforeEach(() => {
  invoke.mockReset();
  localStorage.clear();
  emit = null;
  invoke.mockImplementation((cmd: string) => (cmd === "ai_status" ? Promise.resolve(ready) : Promise.resolve()));
});

describe("LiveHelper", () => {
  it("fixes a cheat while the player plays, then says what's back on", async () => {
    render(<LiveHelper pid={7} packKey="C:/packs/su/pack.json" fix={{ id: "ammo", name: "Infinite ammo" }} onClose={vi.fn()} />);
    fireEvent.change(await screen.findByLabelText("What's wrong"), { target: { value: "only the first gun" } });
    fireEvent.click(screen.getByRole("button", { name: "Fix it now" }));
    await screen.findByRole("button", { name: "Stop" });
    const request = invoke.mock.calls.find(([c]) => c === "ai_start")![1]!.request as Record<string, unknown>;
    expect(request.pid).toBe(7);
    expect(request.wishes).toEqual([]);
    expect(request.live).toEqual({ packKey: "C:/packs/su/pack.json", fix: "ammo", problem: "only the first gun" });
    expect(request.backend).toBe("subscription");

    send({ type: "say", text: "Looking at your guns." });
    send({ type: "ask", id: "q1", text: "Pick up another gun, then press Done.", kind: "done" });
    fireEvent.click(await screen.findByRole("button", { name: "Done" }));
    expect(invoke).toHaveBeenCalledWith("ai_answer", { id: "q1", text: "Done" });

    send({ type: "finished", ok: true, summary: "Infinite ammo now follows the gun in your hands." });
    send({ type: "liveDone", reloaded: true, backOn: ["Infinite health", "Infinite ammo"], problems: [] });
    send({ type: "exit" });
    expect(await screen.findByText(/Your cheats are loaded again/)).toHaveTextContent("Back on: Infinite health, Infinite ammo.");
    expect(screen.getByRole("button", { name: "Ask again" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument();
  });

  it("makes a new cheat from the player's words", async () => {
    render(<LiveHelper pid={7} packKey="k" fix={null} onClose={vi.fn()} />);
    fireEvent.change(await screen.findByLabelText("The cheat you want"), { target: { value: "Infinite flashlight battery" } });
    fireEvent.click(screen.getByRole("button", { name: "Make it now" }));
    await screen.findByRole("button", { name: "Stop" });
    const request = invoke.mock.calls.find(([c]) => c === "ai_start")![1]!.request as Record<string, unknown>;
    expect(request.wishes).toEqual(["Infinite flashlight battery"]);
    expect(request.live).toEqual({ packKey: "k", fix: undefined, problem: undefined });
    send({ type: "activity", tool: "scan_first", summary: "Searching memory" });
    expect(await screen.findByText("Searching memory…")).toBeInTheDocument();
    fireEvent.change(screen.getByRole("textbox", { name: "Message the helper" }), { target: { value: "it only drains with the light on" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(invoke).toHaveBeenCalledWith("ai_note", { text: "it only drains with the light on" });
  });

  it("sends the player to set the helper up first", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "ai_status" ? Promise.resolve({ ...ready, available: false, problem: "The AI helper needs Node.js (free)." }) : Promise.resolve(),
    );
    const onOpenAi = vi.fn();
    render(<LiveHelper pid={7} packKey="k" fix={null} onClose={vi.fn()} onOpenAi={onOpenAi} />);
    expect(await screen.findByText("The AI helper needs Node.js (free).")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Set up the AI helper" }));
    expect(onOpenAi).toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Make it now" })).not.toBeInTheDocument();
  });

  it("shows a session that's already running", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "ai_status"
        ? Promise.resolve({ ...ready, running: true, live: true, transcript: [{ type: "say", text: "Checking your health cheat." }] })
        : Promise.resolve(),
    );
    render(<LiveHelper pid={7} packKey="k" fix={null} resume onClose={vi.fn()} />);
    expect(await screen.findByText("Checking your health cheat.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Stop" })).toBeInTheDocument();
  });
});
