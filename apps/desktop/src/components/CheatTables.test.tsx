import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LibraryGame, RecentTable } from "../lib/ipc";
import { DownloadedTables, FindTable } from "./CheatTables";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));

const hk: LibraryGame = {
  id: "steam:367520",
  name: "Hollow Knight",
  store: "steam",
  folder: String.raw`C:\Games\Hollow Knight`,
  exe: String.raw`C:\Games\Hollow Knight\hollow_knight.exe`,
  launch: { kind: "manual" },
  pack: null,
  presets: [],
  autoPreset: null,
  blocked: null,
  antiCheat: null,
  hidden: false,
  cover: null,
};

const DOWNLOADED = String.raw`C:\Users\me\Downloads\Hollow_Knight.CT`;

const table = (over: Partial<RecentTable> = {}): RecentTable => ({
  path: DOWNLOADED,
  file: "Hollow_Knight.CT",
  modified: 1_700_000_000_000,
  game: "Hollow Knight",
  exe: null,
  cheats: 12,
  error: null,
  ...over,
});

beforeEach(() => {
  invoke.mockReset();
  localStorage.clear();
});
afterEach(() => vi.useRealTimers());

describe("FindTable", () => {
  it("opens a search on the site and remembers the game", async () => {
    invoke.mockResolvedValue(undefined);
    render(<FindTable game={hk} onClose={vi.fn()} onError={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Search FearLess Revolution" }));
    expect(await screen.findByRole("status")).toHaveTextContent("thank its maker");
    expect(invoke).toHaveBeenCalledWith("tables_search", { site: "fearless", game: "Hollow Knight" });
    expect(JSON.parse(localStorage.getItem("opentrainer.tableSearch") ?? "{}")).toMatchObject({ gameId: "steam:367520" });
    fireEvent.click(screen.getByRole("button", { name: "Search the web" }));
    expect(invoke).toHaveBeenLastCalledWith("tables_search", { site: "web", game: "Hollow Knight" });
  });

  it("shows what went wrong", async () => {
    invoke.mockRejectedValue({ kind: "io", message: "no browser" });
    const onError = vi.fn();
    render(<FindTable game={hk} onClose={vi.fn()} onError={onError} />);
    fireEvent.click(screen.getByRole("button", { name: "Search the web" }));
    await vi.waitFor(() => expect(onError).toHaveBeenCalledWith("no browser"));
  });
});

describe("DownloadedTables", () => {
  it("offers a downloaded table for the matching game and converts it for that game's exe", async () => {
    invoke.mockResolvedValue([table(), table({ path: String.raw`C:\x\Broken.CT`, file: "Broken.CT", error: "not a table" })]);
    const onOpenTable = vi.fn();
    render(<DownloadedTables games={[hk]} onOpenTable={onOpenTable} />);
    expect(await screen.findByText("Hollow_Knight.CT")).toBeInTheDocument();
    expect(screen.getByText(/for Hollow Knight · 12 cheats OpenTrainer can use/)).toBeInTheDocument();
    expect(screen.queryByText("Broken.CT")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Turn into cheats" }));
    expect(onOpenTable).toHaveBeenCalledWith(DOWNLOADED, "hollow_knight.exe");
    // Not offered again.
    expect(screen.queryByText("Hollow_Knight.CT")).not.toBeInTheDocument();
  });

  it("hides a table after Not now, and says when a table is only scripts", async () => {
    invoke.mockResolvedValue([table({ cheats: 0, game: "Other Game", exe: "other.exe" })]);
    render(<DownloadedTables games={[hk]} onOpenTable={vi.fn()} />);
    expect(await screen.findByText(/for "Other Game" · only scripts/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    expect(screen.queryByRole("region", { name: "Downloaded cheat tables" })).not.toBeInTheDocument();
  });

  it("looks again every few seconds", async () => {
    vi.useFakeTimers();
    invoke.mockResolvedValue([]);
    render(<DownloadedTables games={[hk]} onOpenTable={vi.fn()} />);
    expect(invoke).toHaveBeenCalledTimes(1);
    invoke.mockResolvedValue([table()]);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(screen.getByText("Hollow_Knight.CT")).toBeInTheDocument();
  });
});
