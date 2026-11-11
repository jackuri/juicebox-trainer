import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LibraryGame, Wanted } from "../lib/ipc";
import { wantedFor } from "../lib/wanted";
import { AskCommunity, MostWanted } from "./Wanted";

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
const wanted: Wanted[] = [
  { game: "Hollow Knight: Voidheart", exe: "hollow_knight.exe", votes: 3, number: 4, asked: "2026-11-11" },
  { game: "Coin Quest", exe: null, votes: 1, number: 5, asked: null },
];

beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation((cmd: string) =>
    cmd === "community_wanted" ? Promise.resolve(wanted) : cmd === "library_list" ? Promise.resolve([hk]) : Promise.resolve(),
  );
});

describe("asking the community (v1.25.0)", () => {
  it("matches requests by exe, else by name", () => {
    expect(wantedFor(hk, wanted)?.number).toBe(4);
    expect(wantedFor({ name: "coin quest!", exe: null }, wanted)?.number).toBe(5);
    expect(wantedFor({ name: "Other", exe: "other.exe" }, wanted)).toBeNull();
  });

  it("adds a vote when players already asked", async () => {
    render(<AskCommunity game={hk} wanted={wanted} onError={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Add your vote for Hollow Knight" }));
    expect(screen.getByText("Asked by 3 players: add your vote")).toBeInTheDocument();
    await screen.findByText(/click 👍 on the request/);
    expect(invoke).toHaveBeenCalledWith("community_open_request", { number: 4 });
  });

  it("asks with the game's name and exe file name only", async () => {
    render(<AskCommunity game={hk} wanted={[]} onError={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Ask the community for cheats for Hollow Knight" }));
    await screen.findByText(/click Create/);
    expect(invoke).toHaveBeenCalledWith("community_ask", { game: "Hollow Knight", exe: "hollow_knight.exe" });
  });

  it("lists the most wanted, and offers the AI for games on this PC", async () => {
    const onOpenAi = vi.fn();
    render(<MostWanted onOpenAi={onOpenAi} />);
    expect(await screen.findByText("Hollow Knight: Voidheart")).toBeInTheDocument();
    expect(screen.getByText(/asked by 3 players/)).toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: "You have it: make cheats with AI" }));
    expect(onOpenAi).toHaveBeenCalled();
    expect(screen.getAllByRole("button", { name: "You have it: make cheats with AI" })).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "Vote for Coin Quest" }));
    expect(invoke).toHaveBeenCalledWith("community_open_request", { number: 5 });
  });
});
