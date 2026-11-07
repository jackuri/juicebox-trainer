import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LibraryGame } from "../lib/ipc";
import { LibraryView } from "./LibraryView";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

function game(over: Partial<LibraryGame>): LibraryGame {
  return {
    id: "steam:1",
    name: "Testbed",
    store: "steam",
    folder: "C:\\Games\\Testbed",
    exe: null,
    launch: { kind: "uri", target: "steam://rungameid/1" },
    pack: null,
    presets: [],
    autoPreset: null,
    blocked: null,
    ...over,
  };
}

const session = {
  process: { pid: 7, parentPid: 1, name: "testbed_native.exe", exePath: null, arch: "x64", threads: 1 },
  modules: [],
};

beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "library_list":
        return Promise.resolve([
          game({
            id: "own:c:\\t.exe",
            name: "OpenTrainer Testbed",
            store: "own",
            pack: { key: "bundled:testbed-native", name: "OpenTrainer Testbed (x64)", ai: false, exe: "testbed_native.exe" },
            autoPreset: "Story mode",
            presets: ["Story mode"],
          }),
          game({ id: "steam:2", name: "Online Shooter", blocked: "anti-cheat found: EasyAntiCheat" }),
          game({ id: "xbox:x", name: "Xbox Game", store: "xbox", launch: { kind: "manual" } }),
        ]);
      case "library_play":
        return Promise.resolve({ session, trainer: null, preset: "Story mode" });
      case "library_add":
        return Promise.resolve();
      default:
        return Promise.resolve();
    }
  });
});

describe("LibraryView", () => {
  it("lists games with their pack, preset and anti-cheat status", async () => {
    render(<LibraryView onPlayed={vi.fn()} onOpenAi={vi.fn()} />);
    const tb = await screen.findByTestId("game-own:c:\\t.exe");
    expect(within(tb).getByText(/Pack: OpenTrainer Testbed \(x64\)/)).toBeInTheDocument();
    expect(within(tb).getByText("Starts with: Story mode")).toBeInTheDocument();
    expect(within(tb).getByRole("button", { name: "Play OpenTrainer Testbed" })).toHaveTextContent("Play with cheats");

    const online = screen.getByTestId("game-steam:2");
    expect(within(online).getByText(/EasyAntiCheat\): not supported/)).toBeInTheDocument();
    expect(within(online).getByRole("button", { name: "Play Online Shooter" })).toBeDisabled();

    const xbox = screen.getByTestId("game-xbox:x");
    expect(within(xbox).getByText(/You start it yourself/)).toBeInTheDocument();
    expect(within(xbox).getByRole("button", { name: "Play Xbox Game" })).toHaveTextContent("Play and attach");

    fireEvent.change(screen.getByLabelText("Find a game"), { target: { value: "xbox" } });
    expect(screen.queryByTestId("game-steam:2")).not.toBeInTheDocument();
  });

  it("plays a game and hands over the session", async () => {
    const played = vi.fn();
    render(<LibraryView onPlayed={played} onOpenAi={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Play OpenTrainer Testbed" }));
    await vi.waitFor(() => expect(played).toHaveBeenCalledWith({ session, trainer: null, preset: "Story mode" }));
    expect(invoke).toHaveBeenCalledWith("library_play", { id: "own:c:\\t.exe" });
  });

  it("shows why a game couldn't start and adds games by exe", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "library_list") return Promise.resolve([game({})]);
      if (cmd === "library_play") return Promise.reject({ kind: "timeout", message: "Testbed didn't start.", verdict: null });
      if (cmd === "library_add") return Promise.reject({ kind: "io", message: "C:\\x.txt is not an .exe file on this PC", verdict: null });
      return Promise.resolve();
    });
    const ai = vi.fn();
    render(<LibraryView onPlayed={vi.fn()} onOpenAi={ai} />);
    fireEvent.click(await screen.findByRole("button", { name: "Play Testbed" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Testbed didn't start.");
    fireEvent.click(screen.getByRole("button", { name: "Make one with AI" }));
    expect(ai).toHaveBeenCalled();
    fireEvent.change(screen.getByLabelText("Game exe path"), { target: { value: "C:\\x.txt" } });
    fireEvent.click(screen.getByRole("button", { name: "Add game" }));
    expect(await screen.findByText(/is not an \.exe file/)).toBeInTheDocument();
  });

  it("adds a game picked with Browse, and does nothing when cancelled", async () => {
    let picked: string | null = null;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "library_list") return Promise.resolve([game({})]);
      if (cmd === "pick_exe") return Promise.resolve(picked);
      return Promise.resolve();
    });
    render(<LibraryView onPlayed={vi.fn()} onOpenAi={vi.fn()} />);
    const browse = await screen.findByRole("button", { name: "Browse for its .exe…" });
    fireEvent.click(browse);
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("pick_exe", { titleText: "Pick the game's .exe", start: null }));
    expect(invoke).not.toHaveBeenCalledWith("library_add", expect.anything());

    picked = "C:\\Games\\Mine\\mine.exe";
    fireEvent.click(browse);
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("library_add", { exe: "C:\\Games\\Mine\\mine.exe" }));
  });
});
