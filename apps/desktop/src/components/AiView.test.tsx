import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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

const testbedGame = { pid: 77, name: "OpenTrainer testbed", libraryId: null };

function status(over: Record<string, unknown> = {}) {
  return {
    available: true,
    problem: null,
    running: false,
    transcript: [],
    signedIn: true,
    download: "notNeeded",
    installing: false,
    ...over,
  };
}

beforeEach(() => {
  invoke.mockReset();
  emit = null;
  localStorage.clear();
  invoke.mockImplementation((cmd: string) =>
    cmd === "ai_status" ? Promise.resolve(status()) : Promise.resolve(undefined),
  );
});

describe("AiView", () => {
  it("explains when the helper can't start", async () => {
    invoke.mockImplementation(() =>
      Promise.resolve(status({ available: false, problem: "The AI helper needs Node.js." })),
    );
    render(<AiView processes={[game]} games={[testbedGame]} onRefreshProcesses={vi.fn()} />);
    expect(await screen.findByText("The AI helper needs Node.js.")).toBeInTheDocument();
  });

  it("offers to download the engine in the installed app", async () => {
    let downloaded = false;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "ai_install") {
        downloaded = true;
        return Promise.resolve();
      }
      return Promise.resolve(
        downloaded
          ? status({ download: "ready" })
          : status({ available: false, problem: "The AI helper's engine isn't downloaded yet.", download: "missing" }),
      );
    });
    render(<AiView processes={[game]} games={[testbedGame]} onRefreshProcesses={vi.fn()} />);
    expect(screen.queryByRole("button", { name: "Open nodejs.org" })).not.toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: "Download the AI engine" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("ai_install", undefined));
    expect(await screen.findByRole("button", { name: "Start" })).toBeInTheDocument();
  });

  it("points to Node.js when it's missing", async () => {
    invoke.mockImplementation(() =>
      Promise.resolve(status({ available: false, problem: "The AI helper needs Node.js (free).", download: "missing" })),
    );
    render(<AiView processes={[game]} games={[testbedGame]} onRefreshProcesses={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Open nodejs.org" }));
    expect(invoke).toHaveBeenCalledWith("open_link", { which: "node" });
    expect(screen.queryByRole("button", { name: "Download the AI engine" })).not.toBeInTheDocument();
  });

  it("starts with the chosen game and wishes, then walks through steps", async () => {
    render(<AiView processes={[game]} games={[testbedGame]} onRefreshProcesses={vi.fn()} />);
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
          analysisArgs: [],
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

  it("makes cheats for the open web game", async () => {
    let open: unknown = null;
    invoke.mockImplementation((cmd: string) =>
      cmd === "ai_status"
        ? Promise.resolve(status())
        : cmd === "web_current"
          ? Promise.resolve(open)
          : Promise.resolve(undefined),
    );
    render(<AiView processes={[game]} games={[testbedGame]} onRefreshProcesses={vi.fn()} webFirst />);
    // No game window yet: it says how to open one, and can't start.
    expect(await screen.findByText(/Open the game first/)).toBeInTheDocument();
    expect(screen.queryByLabelText("Game")).not.toBeInTheDocument();
    expect(screen.queryByText(/code analysis tool/)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start" })).toBeDisabled();

    open = { id: "zombies-1", name: "Zombie Run", url: "https://zombies.example/", added: "2026-11-10" };
    expect(await screen.findByText("Zombie Run", {}, { timeout: 3000 })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("ai_start", {
        request: expect.objectContaining({ pid: 0, task: "webTrainer", webGame: "zombies-1" }) as unknown,
      }),
    );
    act(() => {
      emit?.({ type: "finished", ok: true, summary: "Saved 2 cheats." });
      emit?.({ type: "exit" });
    });
    expect(screen.getByRole("status")).toHaveTextContent("Open Web games");
  });

  it("can use the cheaper model and keep the game window private", async () => {
    render(<AiView processes={[game]} games={[testbedGame]} onRefreshProcesses={vi.fn()} />);
    fireEvent.change(await screen.findByLabelText("Game"), { target: { value: "77" } });
    fireEvent.click(screen.getByLabelText(/Cheaper and faster/));
    fireEvent.click(screen.getByRole("checkbox", { name: /look at the game window/ }));
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("ai_start", {
        request: expect.objectContaining({ model: "claude-sonnet-5-5", pictures: false }) as unknown,
      }),
    );
  });

  it("can cancel while running", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "ai_status"
        ? Promise.resolve(status({ running: true, transcript: [{ type: "say", text: "Working on it." }] }))
        : Promise.resolve(undefined),
    );
    render(<AiView processes={[game]} games={[testbedGame]} onRefreshProcesses={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(invoke).toHaveBeenCalledWith("ai_cancel", undefined);
  });

  it("takes the player's own words instead of Yes / No, and messages while it works", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "ai_status"
        ? Promise.resolve(status({ running: true, transcript: [{ type: "say", text: "Testing your cheats." }] }))
        : Promise.resolve(undefined),
    );
    render(<AiView processes={[game]} games={[testbedGame]} onRefreshProcesses={vi.fn()} />);
    await screen.findByText("Testing your cheats.");

    // While it works: a message box, no question.
    const box = screen.getByLabelText("Message the helper", { selector: "input" });
    fireEvent.change(box, { target: { value: "  Infinite ammo only works with the pistol " } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("ai_note", { text: "Infinite ammo only works with the pistol" }),
    );
    expect(await screen.findByText("Infinite ammo only works with the pistol")).toBeInTheDocument();
    expect(screen.getByLabelText("Message the helper", { selector: "input" })).toHaveValue("");

    // A yes/no question: the player writes what really happens instead.
    act(() => {
      emit?.({ type: "ask", id: "q3", text: "Does Infinite Health work?", kind: "yesno" });
    });
    expect(screen.queryByLabelText("Message the helper", { selector: "input" })).not.toBeInTheDocument();
    const dialog = screen.getByRole("dialog", { name: "Your turn" });
    expect(within(dialog).getByRole("button", { name: "Yes" })).toBeInTheDocument();
    fireEvent.change(within(dialog).getByLabelText("Your own answer"), {
      target: { value: "Yes, but fire damage still hurts" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Send" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("ai_answer", { id: "q3", text: "Yes, but fire damage still hurts" }),
    );
    expect(screen.queryByRole("dialog", { name: "Your turn" })).not.toBeInTheDocument();
  });
});

describe("AiView settings", () => {
  it("uses a local model when chosen, and remembers it", async () => {
    render(<AiView processes={[game]} games={[testbedGame]} onRefreshProcesses={vi.fn()} />);
    fireEvent.change(await screen.findByLabelText("Game"), { target: { value: "77" } });
    fireEvent.click(screen.getByLabelText(/free AI running on this PC/));
    expect(screen.getByRole("button", { name: "Start" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Local AI model"), { target: { value: "qwen3-coder" } });
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("ai_start", {
        request: expect.objectContaining({
          backend: "local",
          localUrl: "http://localhost:11434",
          model: "qwen3-coder",
        }),
      }),
    );
    expect(JSON.parse(localStorage.getItem("opentrainer.ai.settings") ?? "{}")).toMatchObject({
      backend: "local",
      model: "qwen3-coder",
    });
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
    render(<AiView processes={[game]} games={[testbedGame]} onRefreshProcesses={vi.fn()} />);
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
