import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FinderHit, SessionInfo } from "../lib/ipc";
import { FindValuePanel } from "./FindValuePanel";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

const MODULE = { name: "game.exe", path: "C:\\g\\game.exe", base: 0x140000000, size: 0x100000 };
const HEAP = 0x2a000010; // moves every start
const STATIC = 0x140001230; // inside game.exe

function session(pid = 100): SessionInfo {
  return {
    process: { pid, parentPid: 1, name: "game.exe", exePath: "C:\\g\\game.exe", arch: "x64", threads: 1 },
    modules: [MODULE],
  };
}

const hit = (address: number): FinderHit => ({ address, ty: "i32", previous: "250", current: "250" });

/** A fake game: `real` is where the gold really is; memory starts at 250. */
function fakeGame(real: number, others: number[] = [0x3000]) {
  const memory = new Map<number, string>([...others, real].map((a) => [a, "250"]));
  const writes: [number, string][] = [];
  invoke.mockImplementation(async (cmd: string, a: Record<string, unknown> = {}) => {
    switch (cmd) {
      case "finder_start":
      case "finder_next":
        return { count: others.length + 1, rounds: cmd === "finder_start" ? 1 : 2 };
      case "finder_results":
        return [...others, real].map(hit);
      case "read_value":
        return { address: Number(a.address), value: memory.get(Number(a.address)) ?? "0", bytes: "" };
      case "write_value":
        writes.push([Number(a.address), String(a.value)]);
        memory.set(Number(a.address), String(a.value));
        return { address: Number(a.address), value: String(a.value), bytes: "" };
      case "scan_progress":
        return { running: false, done: 0, total: 0 };
      case "refresh_modules":
        return [MODULE];
      case "address_add":
        return { id: 7, label: a.label, address: real, addressExpr: "", ty: "i32", value: "250", error: null, frozen: false };
      case "address_write":
      case "address_set_frozen":
        return {};
      case "my_cheat_save":
        return { key: "k", packName: "Big Game (my cheats)", hotkey: "F1", loaded: true };
      case "pointer_scan":
      case "pointer_filter":
        return {
          exe: "game.exe",
          target: real,
          rounds: 1,
          count: 1,
          paths: [{ expression: "[game.exe+0x500]+0x10", module: "game.exe", moduleOffset: 0x500, offsets: [0x10], address: real }],
        };
      default:
        return undefined;
    }
  });
  return { memory, writes };
}

function renderPanel(pid = 100) {
  const props = { onSessionLost: vi.fn(), onListChanged: vi.fn(), onShowTrainer: vi.fn() };
  const r = render(<FindValuePanel session={session(pid)} gameName="Big Game" {...props} />);
  return { ...props, ...r };
}

async function findGold() {
  fireEvent.click(screen.getByRole("button", { name: "Gold" }));
  fireEvent.change(screen.getByLabelText("Number shown in the game"), { target: { value: "250" } });
  fireEvent.click(screen.getByRole("button", { name: "Search" }));
  // The first place is tried: the game doesn't show it.
  await screen.findByText(/Trying place 1 of 2/);
  expect(screen.getByText("125")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "No" }));
  await screen.findByText(/Trying place 2 of 2/);
  fireEvent.click(screen.getByRole("button", { name: "Yes" }));
  await screen.findByText("Found your Gold!");
}

describe("FindValuePanel", () => {
  beforeEach(() => {
    invoke.mockReset();
    localStorage.clear();
  });

  it("finds a value by trying each place and puts the real number back", async () => {
    const { writes } = fakeGame(STATIC);
    renderPanel();
    await findGold();
    expect(invoke).toHaveBeenCalledWith("finder_start", { number: "250" });
    // Each tried place got a test number, then its own number back.
    expect(writes).toEqual([
      [0x3000, "125"],
      [0x3000, "250"],
      [STATIC, "125"],
      [STATIC, "250"],
    ]);
  });

  it("keeps the value while playing", async () => {
    const props = (fakeGame(STATIC), renderPanel());
    await findGold();
    fireEvent.change(screen.getByLabelText("Cheat value"), { target: { value: "9999" } });
    fireEvent.click(screen.getByRole("button", { name: "Use it now" }));
    await screen.findByText(/kept at 9999/);
    expect(invoke).toHaveBeenCalledWith("address_write", { id: 7, value: "9999" });
    expect(invoke).toHaveBeenCalledWith("address_set_frozen", { id: 7, frozen: true });
    expect(props.onListChanged).toHaveBeenCalled();
  });

  it("saves a value at a fixed place as a cheat right away", async () => {
    fakeGame(STATIC);
    renderPanel();
    await findGold();
    fireEvent.click(screen.getByRole("button", { name: /Save as a cheat/ }));
    await screen.findByText(/Saved "Gold" in "Big Game \(my cheats\)" \(hotkey F1\)/);
    expect(invoke).toHaveBeenCalledWith("my_cheat_save", {
      gameName: "Big Game",
      cheat: { name: "Gold", ty: "i32", value: "250", keep: true, module: "game.exe", moduleOffset: 0x1230, offsets: [] },
      expect: STATIC,
    });
  });

  it("saves a moving value after the game is restarted", async () => {
    fakeGame(HEAP);
    const first = renderPanel(100);
    await findGold();
    fireEvent.change(screen.getByLabelText("Cheat value"), { target: { value: "9999" } });
    fireEvent.click(screen.getByRole("button", { name: /Save as a cheat/ }));
    await screen.findByText(/close the game, start it again/);
    expect(invoke).toHaveBeenCalledWith("pointer_scan", expect.objectContaining({ target: "0x2A000010" }));
    expect(invoke).not.toHaveBeenCalledWith("my_cheat_save", expect.anything());
    first.unmount();

    // The game was restarted (a new process): find it once more, then finish.
    renderPanel(200);
    expect(screen.getByText(/Almost done saving/)).toBeInTheDocument();
    await findGold();
    // It shows what the cheat being finished will do (chosen before the restart).
    expect(screen.getByLabelText("Cheat value")).toHaveValue("9999");
    fireEvent.click(screen.getByRole("button", { name: "Finish saving Gold" }));
    await screen.findByText(/Saved "Gold"/);
    expect(invoke).toHaveBeenCalledWith("pointer_filter", { target: "0x2A000010" });
    expect(invoke).toHaveBeenCalledWith(
      "my_cheat_save",
      expect.objectContaining({
        cheat: expect.objectContaining({ module: "game.exe", moduleOffset: 0x500, offsets: [0x10] }),
      }),
    );
    expect(screen.queryByText(/Almost done saving/)).not.toBeInTheDocument();
  });

  it("asks for the new number while there are many places", async () => {
    fakeGame(STATIC, Array.from({ length: 50 }, (_, i) => 0x4000 + i * 4));
    renderPanel();
    fireEvent.change(screen.getByLabelText("Number shown in the game"), { target: { value: "1,250" } });
    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    expect(await screen.findByTestId("finder-count")).toHaveTextContent("51 possible places");
    fireEvent.change(screen.getByLabelText("New number shown in the game"), { target: { value: "1,300" } });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Search" }));
    });
    expect(invoke).toHaveBeenCalledWith("finder_next", { step: { kind: "number", value: "1,300" } });
  });

  it("works without a number on screen: went up or down", async () => {
    fakeGame(STATIC, Array.from({ length: 50 }, (_, i) => 0x4000 + i * 4));
    renderPanel();
    fireEvent.click(screen.getByRole("button", { name: /There's no number on screen/ }));
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    await screen.findByText("Memory remembered.");
    expect(invoke).toHaveBeenCalledWith("finder_start", { number: null });
    fireEvent.click(screen.getByRole("button", { name: "It went down" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("finder_next", { step: { kind: "decreased" } }));
  });
});
