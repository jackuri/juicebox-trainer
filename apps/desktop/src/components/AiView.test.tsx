import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { pendingAsk } from "../lib/ai";
import type { AiEvent, ProcessInfo } from "../lib/ipc";
import { AiView } from "./AiView";

const invoke = vi.fn();
let emit: ((e: AiEvent) => void) | null = null;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (_name: string, cb: (e: { payload: AiEvent }) => void) => {
    emit = (payload) => cb({ payload });
    return Promise.resolve(() => undefined);
  },
}));

const game: ProcessInfo = {
  pid: 77,
  parentPid: 1,
  name: "testbed_native.exe",
  exePath: "C:\\Games\\testbed_native.exe",
  arch: "x64",
  threads: 1,
};

function status(over: Record<string, unknown> = {}) {
  return { available: true, problem: null, running: false, transcript: [], signedIn: true, ...over };
}

beforeEach(() => {
  invoke.mockReset();
  emit = null;
  invoke.mockImplementation((cmd: string) =>
    cmd === "ai_status" ? Promise.resolve(status()) : Promise.resolve(undefined),
  );
});

describe("AiView", () => {
  it("explains when the helper can't start", async () => {
    invoke.mockImplementation(() =>
      Promise.resolve(status({ available: false, problem: "The AI helper needs Node.js." })),
    );
    render(<AiView processes={[game]} onRefreshProcesses={vi.fn()} />);
    expect(await screen.findByText("The AI helper needs Node.js.")).toBeInTheDocument();
  });

  it("starts with the chosen game and wishes, then walks through steps", async () => {
    render(<AiView processes={[game]} onRefreshProcesses={vi.fn()} />);
    fireEvent.change(await screen.findByLabelText("Game"), { target: { value: "77" } });
    fireEvent.change(screen.getByLabelText("Other cheats"), { target: { value: "potions" } });
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("ai_start", {
        request: {
          pid: 77,
          wishes: ["Infinite health", "Lots of money / gold", "potions"],
          notes: "",
          backend: "subscription",
          maxBudgetUsd: 5,
        },
      }),
    );

    act(() => {
      emit?.({ type: "say", text: "Connecting to your game." });
      emit?.({ type: "activity", tool: "scan_first", summary: "Searching memory" });
      emit?.({ type: "ask", id: "q1", text: "Spend some gold, then press Done.", kind: "done" });
      emit?.({ type: "cost", usd: 0.12 });
    });
    expect(screen.getByText("Connecting to your game.")).toBeInTheDocument();
    expect(screen.getByText("Cost so far: about $0.12")).toBeInTheDocument();
    const dialog = screen.getByRole("dialog", { name: "Your turn" });
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("ai_answer", { id: "q1", text: "Done" }));
    expect(dialog).not.toBeInTheDocument();

    act(() => {
      emit?.({ type: "ask", id: "q2", text: "Does your gold show 12345?", kind: "yesno" });
    });
    fireEvent.click(screen.getByRole("button", { name: "Yes" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("ai_answer", { id: "q2", text: "Yes" }));

    act(() => {
      emit?.({ type: "finished", ok: true, summary: "Saved a pack with 2 cheats." });
      emit?.({ type: "exit" });
    });
    expect(screen.getByRole("status")).toHaveTextContent("Saved a pack with 2 cheats.");
    expect(screen.getByRole("button", { name: "Start over" })).toBeInTheDocument();
  });

  it("can cancel while running", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "ai_status"
        ? Promise.resolve(status({ running: true, transcript: [{ type: "say", text: "Working on it." }] }))
        : Promise.resolve(undefined),
    );
    render(<AiView processes={[game]} onRefreshProcesses={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(invoke).toHaveBeenCalledWith("ai_cancel", undefined);
  });
});

describe("pendingAsk", () => {
  it("finds the unanswered question", () => {
    const ask: AiEvent = { type: "ask", id: "q1", text: "x", kind: "done" };
    expect(pendingAsk([ask])).toEqual(ask);
    expect(pendingAsk([ask, { type: "answered", id: "q1", text: "Done" }])).toBeNull();
    expect(pendingAsk([ask, { type: "finished", ok: false, summary: "" }])).toBeNull();
    expect(pendingAsk([])).toBeNull();
  });
});

describe("AiView sign-in", () => {
  it("asks to sign in first and can check again", async () => {
    let signedIn = false;
    invoke.mockImplementation((cmd: string) =>
      cmd === "ai_status" ? Promise.resolve(status({ signedIn })) : Promise.resolve(undefined),
    );
    render(<AiView processes={[game]} onRefreshProcesses={vi.fn()} />);
    fireEvent.change(await screen.findByLabelText("Game"), { target: { value: "77" } });
    expect(screen.getByRole("note")).toHaveTextContent("isn't signed in");
    expect(screen.getByRole("button", { name: "Start" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Sign in" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("ai_sign_in", undefined));
    signedIn = true;
    fireEvent.click(screen.getByRole("button", { name: "Check again" }));
    await waitFor(() => expect(screen.queryByRole("note")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Start" })).toBeEnabled();
  });
});
