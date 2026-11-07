import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Backup, ProcessInfo, SaveDiff, SaveGame, SavesOverview } from "../lib/ipc";
import { SavesView } from "./SavesView";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

const game: SaveGame = {
  id: "testbednative-1234abcd",
  name: "Testbed",
  exe: "C:\\Games\\Testbed\\testbed_native.exe",
  steam: null,
  cloud: { synced: false, reason: null },
  folders: [
    {
      path: "C:\\Users\\me\\AppData\\LocalLow\\TestCo\\Testbed",
      reason: "Unity save folder (TestCo / Testbed)",
      userAdded: false,
      steamCloud: false,
    },
  ],
  addedMs: 1,
  running: false,
  cloudSynced: false,
  backups: 2,
  latestMs: Date.now() - 60_000,
};

const backups: Backup[] = [
  {
    id: "2000",
    createdMs: 2000,
    reason: "exit",
    note: "",
    files: 1,
    totalSize: 160,
    skipped: [],
    folders: [game.folders[0]!.path],
  },
  {
    id: "1000",
    createdMs: 1000,
    reason: "launch",
    note: "before boss",
    files: 1,
    totalSize: 159,
    skipped: [],
    folders: [game.folders[0]!.path],
  },
];

const goldDiff: SaveDiff = {
  unchanged: 0,
  files: [
    {
      folder: game.folders[0]!.path,
      path: "save1.json",
      change: "changed",
      oldSize: 159,
      newSize: 160,
      detail: {
        kind: "text",
        more: 0,
        hunks: [
          {
            oldLine: 6,
            newLine: 6,
            lines: [
              { tag: "removed", text: '    "gold": 250,' },
              { tag: "added", text: '    "gold": 1250,' },
            ],
          },
        ],
      },
    },
  ],
};

const testbedProcess: ProcessInfo = {
  pid: 77,
  parentPid: 1,
  name: "testbed_native.exe",
  exePath: game.exe,
  arch: "x64",
  threads: 1,
};

function overview(g: Partial<SaveGame> = {}): SavesOverview {
  return { games: [{ ...game, ...g }], steamRunning: false, root: "C:\\Users\\me\\AppData\\Local\\OpenTrainer\\saves" };
}

beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "saves_overview":
        return Promise.resolve(overview());
      case "saves_backups":
        return Promise.resolve(backups);
      case "saves_diff":
        return Promise.resolve(goldDiff);
      case "saves_backup_now":
        return Promise.resolve({ snapshot: { ...backups[0], id: "3000", reason: "manual" }, new: true });
      case "saves_restore":
        return Promise.resolve({
          snapshot: "1000",
          safetySnapshot: "2000",
          written: 1,
          unchanged: 0,
          removed: [],
          extra: [],
          cloudWarning: null,
        });
      default:
        return Promise.resolve(undefined);
    }
  });
});

describe("SavesView", () => {
  it("lists games with their folders and backups", async () => {
    render(<SavesView processes={[]} onRefreshProcesses={vi.fn()} />);
    expect(await screen.findByText("Unity save folder (TestCo / Testbed)")).toBeInTheDocument();
    const list = screen.getByRole("list", { name: "Games with backups" });
    expect(within(list).getByText(/2 backups · last 1 min ago/)).toBeInTheDocument();
    expect(await screen.findByText("before boss")).toBeInTheDocument();
    expect(screen.getByText("Game started")).toBeInTheDocument();
    expect(screen.getByText("Game closed")).toBeInTheDocument();
  });

  it("backs up now with a note", async () => {
    render(<SavesView processes={[]} onRefreshProcesses={vi.fn()} />);
    await screen.findByText("before boss");
    fireEvent.change(screen.getByLabelText("Note for the backup"), { target: { value: "boss" } });
    fireEvent.click(screen.getByRole("button", { name: "Back up now" }));
    expect(await screen.findByText("Backed up.")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("saves_backup_now", { id: game.id, note: "boss" });
  });

  it("compares a backup with the saves now", async () => {
    render(<SavesView processes={[]} onRefreshProcesses={vi.fn()} />);
    await screen.findByText("before boss");
    fireEvent.click(screen.getAllByRole("button", { name: "Compare with now" })[1]!);
    const section = await screen.findByRole("region", { name: "Comparison" });
    expect(within(section).getByText(/"gold": 250,/)).toBeInTheDocument();
    expect(within(section).getByText(/"gold": 1250,/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("saves_diff", { id: game.id, from: "1000", to: null });
  });

  it("asks before restoring and reports how to undo", async () => {
    render(<SavesView processes={[]} onRefreshProcesses={vi.fn()} />);
    await screen.findByText("before boss");
    fireEvent.click(screen.getAllByRole("button", { name: "Restore" })[1]!);
    const dialog = screen.getByRole("dialog", { name: "Restore backup" });
    expect(invoke).not.toHaveBeenCalledWith("saves_restore", expect.anything());
    fireEvent.click(within(dialog).getByRole("button", { name: "Restore" }));
    expect(await screen.findByText(/Restored\. 1 file put back/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("saves_restore", {
      id: game.id,
      backup: "1000",
      removeExtra: false,
    });
  });

  it("can't restore while the game runs and warns about Steam Cloud", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "saves_overview"
        ? Promise.resolve(overview({ running: true, cloudSynced: true }))
        : cmd === "saves_backups"
          ? Promise.resolve(backups)
          : Promise.resolve(undefined),
    );
    render(<SavesView processes={[]} onRefreshProcesses={vi.fn()} />);
    expect(await screen.findByText(/Close the game to restore a backup/)).toBeInTheDocument();
    expect(screen.getByText(/Steam Cloud syncs this game/)).toBeInTheDocument();
    await screen.findByText("before boss");
    for (const b of screen.getAllByRole("button", { name: "Restore" })) {
      expect(b).toBeDisabled();
    }
  });

  it("adds a running game and shows errors", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "saves_overview") return Promise.resolve({ games: [], steamRunning: false, root: null });
      if (cmd === "saves_add_process") return Promise.resolve(game);
      if (cmd === "saves_add_exe")
        return Promise.reject({ kind: "badExe", message: "enter the full path of the game's .exe file" });
      return Promise.resolve([]);
    });
    render(<SavesView processes={[testbedProcess]} onRefreshProcesses={vi.fn()} />);
    expect(await screen.findByText(/No games yet/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Running program"), { target: { value: "77" } });
    fireEvent.click(screen.getAllByRole("button", { name: "Add" })[0]!);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("saves_add_process", { pid: 77 }));

    fireEvent.change(screen.getByLabelText("Game exe path"), { target: { value: "nope" } });
    fireEvent.click(screen.getAllByRole("button", { name: "Add" })[1]!);
    expect(await screen.findByRole("alert")).toHaveTextContent("full path of the game's .exe");
  });
});
