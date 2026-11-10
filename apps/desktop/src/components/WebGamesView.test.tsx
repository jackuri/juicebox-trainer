import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WebCheat, WebSearch } from "../lib/ipc";
import { WebGamesView } from "./WebGamesView";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));

const game = { id: "testbed-1", name: "Web Testbed", url: "http://127.0.0.1:8080/", added: "2026-11-09" };
const memTarget = { k: "m" as const, mem: 0, label: "module memory", size: 65536, off: 4096, t: "i32" as const };
const found: WebSearch = { count: 2, tooMany: false, results: [{ id: 0, target: memTarget, where: "memory 1 at 0x1000 (i32)", value: 75 }, { id: 1, target: { k: "j", path: ["game", "player", "level"] }, where: "game.player.level", value: 75 }] };
let open: typeof game | null;
let saved: WebCheat[];

beforeEach(() => {
  invoke.mockReset();
  open = null;
  saved = [];
  invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "web_games":
        return Promise.resolve([game]);
      case "web_current":
        return Promise.resolve(open);
      case "web_open":
        open = game;
        return Promise.resolve(game);
      case "web_cheats":
        return Promise.resolve(saved);
      case "web_cheats_save":
        saved = args?.cheats as WebCheat[];
        return Promise.resolve();
      case "web_call": {
        const method = args?.method;
        if (method === "info") return Promise.resolve({ url: game.url, title: "t", memories: [{ index: 0, label: "module memory", bytes: 65536 }], locks: 0 });
        if (method === "find") return Promise.resolve(found);
        if (method === "next") return Promise.resolve({ count: 1, tooMany: false, results: [found.results[0]] });
        if (method === "read") return Promise.resolve([{ value: 75 }]);
        if (method === "set") return Promise.resolve({ value: 999 });
        return Promise.resolve({});
      }
      default:
        return Promise.resolve();
    }
  });
});

describe("WebGamesView", () => {
  it("warns to play fair, adds a game by its address and refuses nothing silently", async () => {
    invoke.mockImplementationOnce(() => Promise.resolve([]));
    render(<WebGamesView />);
    expect(screen.getByRole("note")).toHaveTextContent("only use cheats when you play alone, or in a private game with friends you invited");
    expect(await screen.findByText(/None yet/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Game name"), { target: { value: "Web Testbed" } });
    fireEvent.change(screen.getByLabelText("Web address"), { target: { value: "http://127.0.0.1:8080/" } });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("web_game_add", { name: "Web Testbed", url: "http://127.0.0.1:8080/" }));
  });

  it("opens a game, finds a value, narrows it, saves it and sets it", async () => {
    render(<WebGamesView />);
    fireEvent.click(await screen.findByRole("button", { name: "Play Web Testbed" }));
    const panel = await screen.findByRole("region", { name: "Cheats for Web Testbed" });
    expect(await within(panel).findByText("1 game memory found")).toBeInTheDocument();
    // The memory list is looked at again while the game runs.
    await waitFor(() => expect(invoke.mock.calls.filter(([c, a]) => c === "web_call" && (a as { method: string }).method === "info").length).toBeGreaterThan(1), { timeout: 5000 });

    fireEvent.change(within(panel).getByLabelText("Value"), { target: { value: "75" } });
    fireEvent.click(within(panel).getByRole("button", { name: "Find" }));
    expect(await within(panel).findByText("game.player.level")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("web_call", { method: "find", args: { value: 75, kind: "any" } });

    fireEvent.click(within(panel).getByRole("button", { name: "Went down" }));
    await waitFor(() => expect(within(panel).queryByText("game.player.level")).not.toBeInTheDocument());
    expect(invoke).toHaveBeenCalledWith("web_call", { method: "next", args: { how: "decreased", value: null } });

    fireEvent.click(within(panel).getByRole("button", { name: "Add to my cheats" }));
    fireEvent.change(within(panel).getByLabelText("Cheat name"), { target: { value: "Health" } });
    fireEvent.click(within(panel).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(saved).toEqual([{ name: "Health", target: memTarget, value: 75 }]));

    fireEvent.change(await within(panel).findByLabelText("New value for Health"), { target: { value: "999" } });
    fireEvent.click(within(panel).getByRole("button", { name: "Set" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("web_call", { method: "set", args: { target: memTarget, value: 999 } }));
    fireEvent.click(within(panel).getByRole("button", { name: "Lock" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("web_call", { method: "lock", args: { id: "Health|memory 1 at 0x1000 (i32)", target: memTarget, value: 999 } }),
    );
    expect(await within(panel).findByRole("button", { name: "Locked" })).toHaveAttribute("aria-pressed", "true");
  });

  it("removes a game only after asking", async () => {
    render(<WebGamesView />);
    fireEvent.click(await screen.findByRole("button", { name: "Remove Web Testbed" }));
    expect(screen.getByText("Remove it and its saved cheats?")).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("web_game_remove", expect.anything());
    fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("web_game_remove", { id: "testbed-1" }));
  });
});
