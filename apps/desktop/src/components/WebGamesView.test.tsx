import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WebCheat, WebKey, WebPack, WebSearch } from "../lib/ipc";
import { WebGamesView } from "./WebGamesView";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));

const game = { id: "testbed-1", name: "Web Testbed", url: "http://127.0.0.1:8080/", added: "2026-11-09" };
const memTarget = { k: "m" as const, mem: 0, label: "module memory", size: 65536, off: 4096, t: "i32" as const };
const found: WebSearch = { count: 2, tooMany: false, results: [{ id: 0, target: memTarget, where: "memory 1 at 0x1000 (i32)", value: 75 }, { id: 1, target: { k: "j", path: ["game", "player", "level"] }, where: "game.player.level", value: 75 }] };
const shared: WebPack = {
  format: 1,
  id: "zombie-run-1",
  game: "Zombie Run",
  url: "https://zombies.example/play",
  by: "Alex",
  notes: "works from level 2",
  cheats: [{ name: "Ammo", target: { k: "j", path: ["game", "ammo"] }, value: 99, key: "F3" }],
};
let open: typeof game | null;
let saved: WebCheat[];
let pageKeys: { list: WebKey[]; tag: string } | null;
let pageLocks: string[];

beforeEach(() => {
  invoke.mockReset();
  open = null;
  saved = [];
  pageKeys = null;
  pageLocks = [];
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
      case "community_list":
        return Promise.resolve({ url: "", contributeUrl: "", updated: null, packs: [], tables: [], web: [shared], skipped: [] });
      case "agreement_status":
        return Promise.resolve({ accepted: true, version: 1, signed: { version: 1, name: "Sam Player", date: "2026-11-09" } });
      case "web_export":
        return Promise.resolve("C:/Users/sam/Documents/web-testbed.otweb");
      case "web_pick_file":
        return Promise.resolve("C:/Downloads/zombie.otweb");
      case "web_read_file":
        return Promise.resolve(shared);
      case "web_install":
        return Promise.resolve({ game: { ...game, name: "Zombie Run" }, added: 1, skipped: 0 });
      case "web_call": {
        const method = args?.method;
        if (method === "info") return Promise.resolve({ url: game.url, title: "t", memories: [{ index: 0, label: "module memory", bytes: 65536 }], locks: 0 });
        if (method === "find") return Promise.resolve(found);
        if (method === "next") return Promise.resolve({ count: 1, tooMany: false, results: [found.results[0]] });
        if (method === "read") return Promise.resolve([{ value: 75 }]);
        if (method === "set") return Promise.resolve({ value: 999 });
        if (method === "keys") {
          pageKeys = args?.args as { list: WebKey[]; tag: string };
          return Promise.resolve({ keys: pageKeys.list.length });
        }
        if (method === "state") return Promise.resolve({ locks: pageLocks, keysTag: pageKeys?.tag ?? "" });
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

  it("gives a cheat a key, sends it to the game page and shows what the key locked", async () => {
    saved = [{ name: "Health", target: memTarget, value: 75 }];
    render(<WebGamesView />);
    fireEvent.click(await screen.findByRole("button", { name: "Play Web Testbed" }));
    const panel = await screen.findByRole("region", { name: "Cheats for Web Testbed" });
    const box = await within(panel).findByLabelText("Key for Health");
    fireEvent.keyDown(box, { key: "F5", code: "F5" });
    await waitFor(() => expect(saved[0]).toMatchObject({ key: "F5", keyDoes: "lock" }));
    // The helper in the page gets the key.
    await waitFor(
      () =>
        expect(pageKeys?.list).toEqual([
          { id: "Health|memory 1 at 0x1000 (i32)", name: "Health", key: "F5", does: "lock", target: memTarget, value: 75 },
        ]),
      { timeout: 4000 },
    );
    fireEvent.change(within(panel).getByLabelText("What the key for Health does"), { target: { value: "set" } });
    await waitFor(() => expect(saved[0]?.keyDoes).toBe("set"));

    // The player pressed the key in the game: the panel shows it locked.
    pageLocks = ["Health|memory 1 at 0x1000 (i32)"];
    expect(await within(panel).findByRole("button", { name: "Locked" }, { timeout: 4000 })).toHaveAttribute("aria-pressed", "true");

    fireEvent.keyDown(within(panel).getByLabelText("Key for Health"), { key: "Backspace", code: "Backspace" });
    await waitFor(() => expect(saved[0]).toMatchObject({ key: null, keyDoes: null }));
  });

  it("saves the cheats to a file for friends; a game on this PC isn't shared with everyone", async () => {
    render(<WebGamesView />);
    fireEvent.click(await screen.findByRole("button", { name: "Share the cheats for Web Testbed" }));
    const dialog = await screen.findByRole("dialog", { name: "Share the cheats for Web Testbed" });
    // The name the player signed with is suggested.
    await waitFor(() => expect(within(dialog).getByLabelText("Who found these cheats")).toHaveValue("Sam Player"));
    fireEvent.change(within(dialog).getByLabelText("Notes"), { target: { value: "level 2" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save to a file" }));
    expect(await within(dialog).findByText(/Saved: .*web-testbed\.otweb/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("web_export", { id: "testbed-1", by: "Sam Player", notes: "level 2" });
    expect(within(dialog).getByText(/runs on your own PC/)).toBeInTheDocument();
  });

  it("shares a game from the web with everyone after the player says the cheats work", async () => {
    const online = { ...game, url: "https://games.example/play" };
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "web_games") return Promise.resolve([online]);
      if (cmd === "agreement_status") return Promise.resolve({ accepted: true, version: 1, signed: { version: 1, name: "Sam", date: "2026-11-09" } });
      if (cmd === "github_status") return Promise.resolve({ configured: true, signedIn: true, login: "sam" });
      if (cmd === "community_share_web") return Promise.resolve({ url: "https://github.com/jackuri/opentrainer-packs/pull/9", number: 9, update: false });
      if (cmd === "community_list") return Promise.resolve({ web: [] });
      return Promise.resolve(null);
    });
    render(<WebGamesView />);
    fireEvent.click(await screen.findByRole("button", { name: "Share the cheats for Web Testbed" }));
    const dialog = await screen.findByRole("dialog", { name: "Share the cheats for Web Testbed" });
    await waitFor(() => expect(within(dialog).getByLabelText("Who found these cheats")).toHaveValue("Sam"));
    fireEvent.click(within(dialog).getByRole("button", { name: "Share with everyone…" }));
    const share = await within(dialog).findByRole("button", { name: "Share" });
    expect(share).toBeDisabled();
    fireEvent.click(within(dialog).getByRole("checkbox"));
    fireEvent.click(share);
    expect(await within(dialog).findByText(/Sent!/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("community_share_web", { id: "testbed-1", by: "Sam", notes: null });
  });

  it("opens a friend's cheats file, shows what's in it and adds it", async () => {
    render(<WebGamesView />);
    fireEvent.click(await screen.findByRole("button", { name: "Open a cheats file" }));
    const offer = await screen.findByRole("dialog", { name: "Cheats for Zombie Run" });
    expect(within(offer).getByText(/Found by Alex/)).toHaveTextContent("zombies.example");
    expect(within(offer).getByText("works from level 2")).toBeInTheDocument();
    expect(within(offer).getByRole("list", { name: "Cheats in it" })).toHaveTextContent("Ammo (F3)");
    expect(within(offer).getByText(/Only add cheats from people you trust/)).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("web_install", expect.anything());
    fireEvent.click(within(offer).getByRole("button", { name: "Add to my web games" }));
    expect(await screen.findByText(/Added 1 cheat to Zombie Run/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("web_install", { pack: shared });
  });

  it("lists web cheats from the community and gets them", async () => {
    render(<WebGamesView />);
    const list = await screen.findByRole("region", { name: "Web cheats from the community" });
    expect(await within(list).findByText("Zombie Run")).toBeInTheDocument();
    expect(within(list).getByText(/1 cheat · found by Alex/)).toBeInTheDocument();
    fireEvent.click(within(list).getByRole("button", { name: "Get the cheats for Zombie Run" }));
    const offer = await screen.findByRole("dialog", { name: "Cheats for Zombie Run" });
    expect(within(offer).getByText(/A maintainer checked these/)).toBeInTheDocument();
  });
});
