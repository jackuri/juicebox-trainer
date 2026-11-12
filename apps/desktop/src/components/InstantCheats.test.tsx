import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { exeName, madeNote } from "../lib/instant";
import { InstantCheats } from "./InstantCheats";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));

const made = {
  key: "user:engine-hero",
  packName: "Hero (instant cheats)",
  cheats: ["Game speed", "Move speed", "Infinite health"],
  notFound: ["Ammo", "Money"],
  loaded: true,
};

describe("InstantCheats", () => {
  beforeEach(() => invoke.mockReset());

  it("makes instant cheats for an Unreal game, with the library's name for it", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "engine_cheats_support")
        return Promise.resolve({ engine: "Unreal Engine 5.4", supported: true, why: null, packKey: null });
      if (cmd === "library_list")
        return Promise.resolve([{ name: "Hero Game", exe: "C:\\Games\\Hero\\Hero-Win64-Shipping.exe" }]);
      if (cmd === "engine_cheats_make") return Promise.resolve(made);
      return Promise.resolve(null);
    });
    const onMade = vi.fn();
    render(<InstantCheats processName="Hero-Win64-Shipping.exe" onMade={onMade} />);
    expect(await screen.findByText("Instant cheats for Unreal Engine 5.4 games")).toBeTruthy();
    fireEvent.click(screen.getByText("Make instant cheats"));
    await waitFor(() => expect(onMade).toHaveBeenCalledWith(made));
    expect(invoke).toHaveBeenCalledWith("engine_cheats_make", { gameName: "Hero Game" });
  });

  it("says why when the game's engine isn't supported yet, and shows nothing for others", async () => {
    invoke.mockResolvedValue({ engine: "Unity 2022.3.5f1", supported: false, why: "Coming later.", packKey: null });
    const { unmount } = render(<InstantCheats processName="game.exe" onMade={vi.fn()} />);
    expect(await screen.findByText("Instant cheats: Coming later.")).toBeTruthy();
    expect(screen.queryByText("Make instant cheats")).toBeNull();
    unmount();

    invoke.mockResolvedValue({ engine: null, supported: false, why: null, packKey: null });
    const { container } = render(<InstantCheats processName="game.exe" onMade={vi.fn()} />);
    await waitFor(() => expect(invoke).toHaveBeenCalled());
    expect(container.textContent).toBe("");
  });

  it("shows the problem when it can't make them", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "engine_cheats_support")
        return Promise.resolve({ engine: "Unreal Engine 5.4", supported: true, why: null, packKey: "k" });
      if (cmd === "engine_cheats_make")
        return Promise.reject({ kind: "debugger", message: "your character isn't in the world yet" });
      return Promise.resolve([]);
    });
    render(<InstantCheats processName="Hero.exe" onMade={vi.fn()} />);
    fireEvent.click(await screen.findByText("Make them again"));
    expect((await screen.findByRole("alert")).textContent).toContain("isn't in the world yet");
  });

  it("explains what was made", () => {
    expect(madeNote(made)).toBe(
      "Made 3 instant cheats: Game speed, Move speed, Infinite health. Not found in this game: ammo, money (the AI helper can look for them). They work in many Unreal games, not every one: try each.",
    );
    expect(exeName("C:\\G\\A.EXE")).toBe("a.exe");
    expect(exeName("/x/b.exe")).toBe("b.exe");
  });
});
