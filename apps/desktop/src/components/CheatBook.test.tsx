import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { describeCheat, fixRequest, toEdit } from "../lib/cheatBook";
import type { BookView, Cheat } from "../lib/ipc";
import { CheatBook } from "./CheatBook";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));

const health: Cheat = { id: "health", name: "Infinite health", category: "Player", ui: "toggle", hotkey: "F1", toggle: { value: 100 }, impl: { kind: "value" } };
const ammo: Cheat = { id: "ammo", name: "No reload", category: "Player", ui: "toggle", hotkey: "F2", impl: { kind: "patch" } };
const speed: Cheat = { id: "speed", name: "Speed", category: "Movement", ui: "slider", slider: { min: 1, max: 5, step: 1, default: 1, hold: true }, impl: { kind: "value" } };
const gold: Cheat = { id: "gold", name: "Gold", category: "Items", ui: "number", number: { min: 0 }, impl: { kind: "value" } };

const book: BookView = {
  key: "C:/packs/hero/pack.json",
  source: "community",
  pack: {
    schema: 1,
    game: { id: "hero", name: "Hero Quest", exe: "Hero.exe", builds: [] },
    meta: { author: "Maker", packVersion: "1.0.0" },
    cheats: [health, ammo, speed, gold],
  },
  notes: { gold: "only in town" },
  favourites: [],
  keys: {},
  loaded: false,
};

beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "book_open") return Promise.resolve(book);
    if (cmd === "book_save") return Promise.resolve({ key: book.key, copied: false, reloaded: false });
    return Promise.resolve();
  });
});

describe("CheatBook", () => {
  it("lists every cheat by group with what it does, without a game", async () => {
    render(<CheatBook packKey={book.key} onClose={vi.fn()} onAskAi={vi.fn()} />);
    expect(await screen.findByText("Hero Quest")).toBeInTheDocument();
    expect(screen.getByText(/4 cheats · community · Hero.exe/)).toBeInTheDocument();
    for (const g of ["Player", "Movement", "Items"]) expect(screen.getByText(g)).toBeInTheDocument();
    expect(screen.getByText(/switch: keeps it at 100/)).toBeInTheDocument();
    expect(screen.getByText(/slider 1 to 5/)).toBeInTheDocument();
    expect(screen.getByText("only in town")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save changes" })).toBeDisabled();
    // Find a cheat by name.
    fireEvent.change(screen.getByLabelText("Find a cheat"), { target: { value: "gold" } });
    expect(screen.queryByText("Speed")).not.toBeInTheDocument();
  });

  it("saves changed names, values, keys, notes and favourites, and a removed cheat", async () => {
    render(<CheatBook packKey={book.key} onClose={vi.fn()} onAskAi={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Edit Infinite health" }));
    const edit = screen.getByRole("group", { name: "Change Infinite health" });
    fireEvent.change(within(edit).getByLabelText("Name"), { target: { value: "God mode" } });
    fireEvent.change(within(edit).getByLabelText(/Keep it at/), { target: { value: "999" } });
    fireEvent.change(within(edit).getByLabelText(/Key \(e\.g\./), { target: { value: "Ctrl+1" } });
    fireEvent.change(within(edit).getByLabelText("Your note"), { target: { value: "turn on after loading" } });
    fireEvent.click(screen.getByRole("button", { name: "Favourite God mode" }));
    fireEvent.click(screen.getByRole("button", { name: "Edit Speed" }));
    const s = screen.getByRole("group", { name: "Change Speed" });
    fireEvent.change(within(s).getByLabelText("Highest"), { target: { value: "10" } });
    fireEvent.click(within(s).getByRole("button", { name: "Remove this cheat" }));
    expect(screen.getByText("Not saved yet")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("book_save", expect.anything()));
    const sent = invoke.mock.calls.find(([c]) => c === "book_save")![1] as { key: string; edit: { cheats: Cheat[]; notes: Record<string, string>; favourites: string[]; keys: Record<string, string> } };
    expect(sent.key).toBe(book.key);
    expect(sent.edit.cheats.map((c) => c.id)).toEqual(["health", "ammo", "gold"]);
    expect(sent.edit.cheats[0]).toMatchObject({ name: "God mode", toggle: { value: 999 }, impl: { kind: "value" } });
    expect(sent.edit.notes).toEqual({ health: "turn on after loading", gold: "only in town" });
    expect(sent.edit.favourites).toEqual(["health"]);
    expect(sent.edit.keys).toEqual({ health: "Ctrl+1" });
    expect(await screen.findByRole("status")).toHaveTextContent("Saved.");
  });

  it("says when a typed number isn't a number", async () => {
    render(<CheatBook packKey={book.key} onClose={vi.fn()} onAskAi={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Edit Speed" }));
    fireEvent.change(within(screen.getByRole("group", { name: "Change Speed" })).getByLabelText("Highest"), { target: { value: "lots" } });
    fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Speed: “lots” isn't a number (highest)");
    expect(invoke).not.toHaveBeenCalledWith("book_save", expect.anything());
  });

  it("asks the AI helper to fix a cheat, in the player's words", async () => {
    const onAskAi = vi.fn();
    render(<CheatBook packKey={book.key} onClose={vi.fn()} onAskAi={onAskAi} />);
    fireEvent.click(await screen.findByRole("button", { name: "Edit Infinite health" }));
    fireEvent.click(screen.getByRole("button", { name: "It doesn't work: ask the AI to fix it" }));
    const ask = screen.getByRole("group", { name: "Ask the AI about Infinite health" });
    const open = within(ask).getByRole("button", { name: "Open the AI helper" });
    expect(open).toBeDisabled();
    fireEvent.change(within(ask).getByLabelText("What's wrong with it?"), { target: { value: "health still goes down" } });
    fireEvent.click(open);
    expect(onAskAi).toHaveBeenCalledWith({
      game: "Hero Quest",
      exe: "Hero.exe",
      cheat: "Infinite health",
      problem: "health still goes down",
      describe: "switch: keeps it at 100",
      key: "C:/packs/hero/pack.json",
      id: "health",
    });
  });

  it("says what happens when saving a pack that's in use", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "book_open") return Promise.resolve({ ...book, source: "bundled", loaded: true });
      if (cmd === "book_save") return Promise.resolve({ key: "C:/mine/pack.json", copied: true, reloaded: true });
      return Promise.resolve();
    });
    const onSaved = vi.fn();
    render(<CheatBook packKey="bundled:hero" onClose={vi.fn()} onAskAi={vi.fn()} onSaved={onSaved} />);
    expect(await screen.findByText(/saved as your own copy/)).toBeInTheDocument();
    expect(screen.getByText(/in use in Cheats right now/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Favourite Gold" }));
    fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
    expect(await screen.findByRole("status")).toHaveTextContent("your own copy of the pack now");
    expect(screen.getByRole("status")).toHaveTextContent("turn your cheats back on");
    expect(onSaved).toHaveBeenCalledWith("C:/mine/pack.json");
  });
});

describe("cheat book helpers", () => {
  it("describes cheats in plain words", () => {
    expect(describeCheat(ammo)).toBe("switch (changes how the game works)");
    expect(describeCheat({ ...health, toggle: {} })).toBe("switch: keeps it where it is");
    expect(describeCheat({ id: "b", name: "B", ui: "button", button: { add: 100 }, impl: { kind: "value" } })).toBe("button: adds 100");
  });

  it("an empty keep-at box means 'where it is'; a patch switch keeps no value", () => {
    const e = toEdit([health, ammo], { health: { keepAt: " " }, ammo: { keepAt: "5" } }, {}, [], {});
    if (typeof e === "string") throw new Error(e);
    expect(e.cheats[0]?.toggle).toEqual({});
    expect(e.cheats[1]?.toggle).toBeUndefined();
  });

  it("tells the AI helper what to fix", () => {
    const r = fixRequest({ game: "Hero Quest", exe: "Hero.exe", cheat: "Gold", problem: "goes back to 0", describe: "type a number" });
    expect(r.wishes).toEqual(["Fix my cheat “Gold”"]);
    expect(r.notes).toContain("goes back to 0");
    expect(r.notes).toContain("Hero.exe");
  });
});
