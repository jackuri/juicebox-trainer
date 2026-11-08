import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CommunityList, PackPreview, PackSummary } from "../lib/ipc";
import { PacksView } from "./PacksView";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

function pack(over: Partial<PackSummary>): PackSummary {
  return {
    key: "bundled:testbed-native",
    source: "bundled",
    path: null,
    id: "testbed-native",
    name: "OpenTrainer Testbed (x64)",
    exe: "testbed_native.exe",
    arch: "x64",
    author: "OpenTrainer",
    ai: false,
    packVersion: "1.0.0",
    cheats: 5,
    error: null,
    issues: [],
    ...over,
  };
}

const mine = pack({
  key: "C:\\packs\\my-game\\pack.json",
  source: "community",
  id: "my-game",
  name: "My Game",
  exe: "mygame.exe",
});

const list: CommunityList = {
  url: "https://example.invalid/index.json",
  contributeUrl: "https://example.invalid/#add-your-pack",
  updated: "2026-11-07",
  skipped: [],
  packs: [
    {
      id: "my-game",
      name: "My Game",
      exe: "mygame.exe",
      arch: "x64",
      engine: null,
      author: "sam",
      packVersion: "1.1.0",
      cheats: 3,
      notes: "Tested on the Steam version",
      sha256: "a".repeat(64),
      size: 100,
      installed: "update",
    },
    {
      id: "other-game",
      name: "Other Game",
      exe: "other.exe",
      arch: null,
      engine: null,
      author: "ai",
      packVersion: "1.0.0",
      cheats: 1,
      notes: null,
      sha256: "b".repeat(64),
      size: 100,
      installed: "none",
    },
  ],
};

function preview(over: Partial<PackPreview> = {}): PackPreview {
  return {
    id: "my-game",
    game: "My Game",
    exe: "mygame.exe",
    author: "sam",
    ai: false,
    version: "1.1.0",
    notes: null,
    cheats: [
      { name: "Infinite Health", ui: "toggle", description: "Health stays full", patch: false },
      { name: "No Reload", ui: "toggle", description: null, patch: true },
    ],
    warnings: [],
    existing: null,
    ...over,
  };
}

beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "packs_list":
        return Promise.resolve([pack({}), mine]);
      case "community_list":
        return Promise.resolve(list);
      case "community_preview":
        return Promise.resolve(
          preview({ existing: { key: mine.key, name: "My Game", version: "1.0.0", ai: false, same: false } }),
        );
      case "packs_preview":
        return Promise.resolve(preview({ id: "new-game", game: "New Game" }));
      case "packs_import":
      case "community_install":
        return Promise.resolve({ key: "k", installed: "new" });
      case "packs_export":
        return Promise.resolve("C:\\Users\\me\\Documents\\testbed-native-1.0.0.otpack");
      case "packs_remove":
        return Promise.resolve("C:\\packs-removed\\my-game-1");
      default:
        return Promise.resolve(null);
    }
  });
});

function view(openPath: string | null = null) {
  const opened = vi.fn();
  render(<PacksView openPath={openPath} onOpened={opened} onShowCheats={vi.fn()} />);
  return opened;
}

describe("PacksView", () => {
  it("lists your packs with where they came from, and exports one", async () => {
    view();
    const builtIn = await screen.findByTestId("pack-bundled:testbed-native");
    expect(within(builtIn).getByText(/built-in example/)).toBeInTheDocument();
    expect(within(builtIn).queryByRole("button", { name: /Remove/ })).not.toBeInTheDocument();
    expect(within(screen.getByTestId(`pack-${mine.key}`)).getByText(/community/)).toBeInTheDocument();

    fireEvent.click(within(builtIn).getByRole("button", { name: "Export OpenTrainer Testbed (x64)" }));
    expect(await screen.findByText(/Send this file to share it/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("packs_export", { key: "bundled:testbed-native" });
    fireEvent.click(screen.getByRole("button", { name: "Show in folder" }));
    expect(invoke).toHaveBeenCalledWith("packs_show_file", {
      path: "C:\\Users\\me\\Documents\\testbed-native-1.0.0.otpack",
    });
  });

  it("asks before removing a pack and says where the copy is", async () => {
    view();
    fireEvent.click(await screen.findByRole("button", { name: "Remove My Game" }));
    expect(invoke).not.toHaveBeenCalledWith("packs_remove", expect.anything());
    fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    expect(await screen.findByText(/A copy is kept in C:\\packs-removed\\my-game-1/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("packs_remove", { key: mine.key });
  });

  it("shows community packs, what's inside, and replaces only after saying so", async () => {
    view();
    const row = await screen.findByTestId("community-my-game");
    expect(within(row).getByText("Update available")).toBeInTheDocument();
    expect(within(screen.getByTestId("community-other-game")).getByText("AI-made")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Find a community pack"), { target: { value: "other" } });
    expect(screen.queryByTestId("community-my-game")).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Find a community pack"), { target: { value: "" } });

    fireEvent.click(within(screen.getByTestId("community-my-game")).getByRole("button", { name: "See My Game" }));
    const dialog = await screen.findByRole("dialog", { name: "Install pack" });
    expect(within(dialog).getByText("Infinite Health")).toBeInTheDocument();
    expect(within(dialog).getByText(/changes game code/)).toBeInTheDocument();
    expect(within(dialog).getByText(/This replaces your pack for this game: My Game v1.0.0/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Replace and install" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("community_install", { id: "my-game", replace: true }));
    expect(await screen.findByText(/Installed the pack for My Game/)).toBeInTheDocument();
  });

  it("opens a pack file from Explorer and installs it", async () => {
    const opened = view("C:\\Downloads\\new-game-1.0.0.otpack");
    const dialog = await screen.findByRole("dialog", { name: "Install pack" });
    expect(opened).toHaveBeenCalled();
    expect(within(dialog).getByText(/Install the pack for New Game/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Install" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("packs_import", { path: "C:\\Downloads\\new-game-1.0.0.otpack", replace: false, exe: null }),
    );
  });

  it("says when the pack is already there, and when the list can't load", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "packs_list") return Promise.resolve([]);
      if (cmd === "community_list")
        return Promise.reject({ kind: "offline", message: "the pack list isn't online yet", verdict: null });
      if (cmd === "packs_pick_otpack") return Promise.resolve("C:\\a.otpack");
      if (cmd === "packs_preview")
        return Promise.resolve(preview({ existing: { key: "k", name: "My Game", version: "1.1.0", ai: false, same: true } }));
      return Promise.resolve(null);
    });
    view();
    expect(await screen.findByText(/Couldn't load the community list: the pack list isn't online yet/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Open a pack or table…" }));
    const dialog = await screen.findByRole("dialog", { name: "Install pack" });
    expect(within(dialog).getByText("You already have this exact pack.")).toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: "Install" })).not.toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("opens a Cheat Engine table, asks for the game's exe and says what was left out", async () => {
    const table = preview({
      id: "coin-quest",
      game: "Coin Quest",
      exe: "unknown.exe",
      author: "Cheat Engine table (imported)",
      cheats: [{ name: "Coins", ui: "number", description: null, patch: false }],
      skipped: [{ name: "God mode", why: "runs its own code (a script); OpenTrainer never runs code from a table" }],
      needsExe: true,
    });
    invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "packs_list") return Promise.resolve([]);
      if (cmd === "community_list") return Promise.resolve(list);
      if (cmd === "packs_preview")
        return Promise.resolve(args?.exe ? { ...table, exe: args.exe, needsExe: false } : table);
      if (cmd === "pick_exe") return Promise.resolve("D:\\Games\\Coin Quest\\CoinQuest.exe");
      if (cmd === "packs_import") return Promise.resolve({ key: "k", installed: "new" });
      return Promise.resolve(null);
    });
    view("C:\\Downloads\\Coin_Quest.CT");
    const dialog = await screen.findByRole("dialog", { name: "Install pack" });
    expect(within(dialog).getByText(/doesn't say which game/)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Install" })).toBeDisabled();
    expect(within(dialog).getByText("1 entry not imported")).toBeInTheDocument();
    expect(within(dialog).getByText(/God mode: runs its own code/)).toBeInTheDocument();

    fireEvent.click(within(dialog).getByRole("button", { name: "Choose the game's exe…" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("packs_preview", { path: "C:\\Downloads\\Coin_Quest.CT", exe: "CoinQuest.exe" }),
    );
    const again = await screen.findByRole("dialog", { name: "Install pack" });
    await waitFor(() => expect(within(again).getByRole("button", { name: "Install" })).toBeEnabled());
    expect(within(again).getByText("CoinQuest.exe")).toBeInTheDocument();
    fireEvent.click(within(again).getByRole("button", { name: "Install" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("packs_import", {
        path: "C:\\Downloads\\Coin_Quest.CT",
        replace: false,
        exe: "CoinQuest.exe",
      }),
    );
  });

  it("can't install a table with nothing usable in it", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "packs_list") return Promise.resolve([]);
      if (cmd === "community_list") return Promise.resolve(list);
      if (cmd === "packs_preview")
        return Promise.resolve(
          preview({ cheats: [], skipped: [{ name: "God mode", why: "runs its own code" }], needsExe: true }),
        );
      return Promise.resolve(null);
    });
    view("C:\\Downloads\\scripts.CT");
    const dialog = await screen.findByRole("dialog", { name: "Install pack" });
    expect(within(dialog).getByText(/Nothing in this table can be used/)).toBeInTheDocument();
    expect(within(dialog).queryByText(/doesn't say which game/)).not.toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Install" })).toBeDisabled();
  });
});