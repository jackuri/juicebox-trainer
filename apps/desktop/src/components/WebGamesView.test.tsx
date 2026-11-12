import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WebCheat, WebKey, WebPack, WebSearch } from "../lib/ipc";
import { WebGamesView } from "./WebGamesView";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
const listeners = new Map<string, (e: { payload: unknown }) => void>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, cb: (e: { payload: unknown }) => void) => {
    listeners.set(name, cb);
    return Promise.resolve(() => undefined);
  },
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
    expect(screen.getByRole("note")).toHaveTextContent("cheats are fine when you play alone, with friends, or in online games that have no anti-cheat");
    expect(await screen.findByText(/None yet/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Game name"), { target: { value: "Web Testbed" } });
    fireEvent.change(screen.getByLabelText("Web address"), { target: { value: "http://127.0.0.1:8080/" } });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("web_game_add", { name: "Web Testbed", url: "http://127.0.0.1:8080/" }));
  });

  it("keeps working when a found value goes away (a swapped weapon): it shows ? (v1.27.1)", async () => {
    const gone = (value: unknown) => ({
      count: 2,
      tooMany: false,
      results: [
        { id: 0, target: { k: "j", path: ["game", "player", "weapon", "rockets"] }, where: "game.player.weapon.rockets", value },
        { id: 1, target: memTarget, where: "memory 1 at 0x1000 (i32)" },
      ],
    });
    const base = invoke.getMockImplementation()!;
    invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      const method = (args as { method?: string } | undefined)?.method;
      // What 1.27.0's page helper sent for a gone value (NaN arrives as null), then nothing at all.
      if (cmd === "web_call" && method === "next") return Promise.resolve(gone(null));
      if (cmd === "web_call" && method === "results") return Promise.resolve(gone("empty"));
      return base(cmd, args);
    });
    render(<WebGamesView />);
    fireEvent.click(await screen.findByRole("button", { name: "Play Web Testbed" }));
    const panel = await screen.findByRole("region", { name: "Cheats for Web Testbed" });
    await within(panel).findByText("1 game memory found");
    fireEvent.change(within(panel).getByLabelText("Value"), { target: { value: "7" } });
    fireEvent.click(within(panel).getByRole("button", { name: "Find" }));
    await within(panel).findByText(/2 found/);
    fireEvent.click(within(panel).getByRole("button", { name: "Changed" }));
    const results = await within(panel).findByRole("list", { name: "Results" });
    await waitFor(() => expect(within(results).getAllByText("?")).toHaveLength(2));
    expect(within(results).getByText("game.player.weapon.rockets")).toBeInTheDocument();
    // The list refreshes every second; still no crash.
    await new Promise((r) => setTimeout(r, 1200));
    expect(screen.getByRole("region", { name: "Cheats for Web Testbed" })).toBeInTheDocument();
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
      expect(invoke).toHaveBeenCalledWith("web_call", { method: "lock", args: { id: "Health|memory 1 at 0x1000 (i32)", target: memTarget, value: 999, how: "at" } }),
    );
    expect(await within(panel).findByRole("button", { name: "Locked" })).toHaveAttribute("aria-pressed", "true");
  });

  it("opens the AI helper for the open game and shows the cheats it saved", async () => {
    open = game;
    const onOpenAi = vi.fn();
    render(<WebGamesView onOpenAi={onOpenAi} />);
    fireEvent.click(await screen.findByRole("button", { name: "Find cheats with AI" }));
    expect(onOpenAi).toHaveBeenCalled();
    // The helper saved a cheat: the panel shows it without reopening.
    saved = [{ name: "Infinite ammo", target: { k: "j", path: ["game", "ammo"] }, value: 99 }];
    await waitFor(() => expect(listeners.has("web-cheats-changed")).toBe(true));
    listeners.get("web-cheats-changed")?.({ payload: "other-game" });
    listeners.get("web-cheats-changed")?.({ payload: game.id });
    expect(await screen.findByText("Infinite ammo")).toBeInTheDocument();
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
          { id: "Health|memory 1 at 0x1000 (i32)", name: "Health", key: "F5", does: "lock", target: memTarget, value: 75, how: "at" },
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
      if (cmd === "community_share_web") return Promise.resolve({ url: "https://github.com/jackuri/juicebox-packs/pull/9", number: 9, update: false });
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

  it("puts the weak enemy cheats together into one that keeps them all at most at 1 HP (v1.31.0)", async () => {
    const enemy = (n: number): WebCheat => ({ name: `Weak enemy ${n} (1 HP)`, target: { ...memTarget, off: 0x6cb554 + 0xd90 * (n - 1), t: "i32" }, value: 1 });
    saved = [{ name: "Health", target: memTarget, value: 75 }, enemy(1), enemy(2), enemy(3)];
    pageLocks = ["Weak enemy 2 (1 HP)|memory 1 at 0x6CC2E4 (i32)"];
    const base = invoke.getMockImplementation()!;
    invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      const a = args as { method?: string; args?: { targets?: unknown[] } } | undefined;
      if (cmd === "web_call" && a?.method === "read") return Promise.resolve((a.args?.targets ?? []).map(() => ({ value: 75 })));
      return base(cmd, args);
    });
    render(<WebGamesView />);
    fireEvent.click(await screen.findByRole("button", { name: "Play Web Testbed" }));
    const panel = await screen.findByRole("region", { name: "Cheats for Web Testbed" });
    fireEvent.click(await within(panel).findByRole("button", { name: "Combine…" }));
    const box = within(panel).getByRole("group", { name: "Combine cheats" });
    fireEvent.click(within(panel).getByLabelText("Combine Health"));
    fireEvent.click(within(panel).getByLabelText("Combine Weak enemy 1 (1 HP)"));
    expect(within(box).getByText(/aren't evenly spaced/)).toBeInTheDocument();
    fireEvent.click(within(panel).getByLabelText("Combine Health"));
    fireEvent.click(within(panel).getByLabelText("Combine Weak enemy 2 (1 HP)"));
    fireEvent.click(within(panel).getByLabelText("Combine Weak enemy 3 (1 HP)"));
    await waitFor(() => expect(within(panel).getByLabelText("Weak enemy 3 (1 HP) value")).toHaveTextContent("75"), { timeout: 3000 });
    fireEvent.click(within(box).getByRole("button", { name: "Combine 3 into one" }));
    expect(within(box).getByLabelText("Combined cheat name")).toHaveValue("Weak enemy (1 HP)");
    // The game shows 75 for them now (more than 1): only lowering is picked, so dead ones stay dead.
    expect(within(box).getByLabelText("How the lock keeps the number")).toHaveValue("atMost");
    expect(within(box).getByText(/One cheat for 3 places/)).toBeInTheDocument();
    fireEvent.change(within(box).getByLabelText("Combined cheat name"), { target: { value: "Weak enemies (1 HP)" } });
    fireEvent.click(within(box).getByRole("button", { name: "Save the combined cheat" }));
    await waitFor(() => expect(saved.map((c) => c.name)).toEqual(["Health", "Weak enemies (1 HP)"]));
    expect(saved[1]).toEqual({ name: "Weak enemies (1 HP)", target: { ...memTarget, off: 0x6cb554, t: "i32", every: 0xd90, count: 3 }, value: 1, keep: "atMost" });
    // The old cheat's lock was lifted.
    expect(invoke).toHaveBeenCalledWith("web_call", { method: "unlock", args: { id: "Weak enemy 2 (1 HP)|memory 1 at 0x6CC2E4 (i32)" } });
    expect(await within(panel).findByText(/3 places 0xD90 apart/)).toBeInTheDocument();
    expect(within(panel).getByText(/never above 1/)).toBeInTheDocument();
    // Locking it sends how it keeps the number.
    fireEvent.click(within(panel).getAllByRole("button", { name: "Lock" })[1]!);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("web_call", expect.objectContaining({ method: "lock", args: expect.objectContaining({ how: "atMost" }) })));
  });

  it("asks the AI about the cheats, or to fix one, while playing (v1.31.0)", async () => {
    saved = [{ name: "Infinite pistol ammo", target: memTarget, value: 99 }];
    const base = invoke.getMockImplementation()!;
    invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) =>
      cmd === "ai_status"
        ? Promise.resolve({ available: true, problem: null, running: false, transcript: [], signedIn: true, download: "notNeeded", installing: false })
        : base(cmd, args),
    );
    render(<WebGamesView />);
    fireEvent.click(await screen.findByRole("button", { name: "Play Web Testbed" }));
    const panel = await screen.findByRole("region", { name: "Cheats for Web Testbed" });
    fireEvent.click(await within(panel).findByRole("button", { name: "Fix Infinite pistol ammo with AI" }));
    const helper = await within(panel).findByRole("region", { name: "AI helper" });
    fireEvent.change(await within(helper).findByLabelText("What's wrong"), { target: { value: "not the shotgun" } });
    fireEvent.click(within(helper).getByRole("button", { name: "Fix it now" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("ai_start", {
        request: expect.objectContaining({ pid: 0, task: "webTrainer", webGame: "testbed-1", wishes: [], webFix: { cheat: "Infinite pistol ammo", problem: "not the shotgun" } }),
      }),
    );
    fireEvent.click(within(panel).getByRole("button", { name: "Ask the AI" }));
    const ask = await within(panel).findByRole("region", { name: "AI helper" });
    expect(await within(ask).findByText("What do you want, or what are you stuck on?")).toBeInTheDocument();
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
