import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Player } from "../lib/ipc";
import { levelFor, levelUp, nextTitle, standing, streakReward, streakWithToday, titleFor, utcToday, xpFor } from "../lib/levels";
import { PackerBadge } from "./PackerLevel";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));

let players: Player[];
let signedIn: boolean;
let checkIn: string;
beforeEach(() => {
  invoke.mockReset();
  localStorage.clear();
  players = [
    { login: "alex", xp: 500, packs: 40, ratings: 50 },
    { login: "jackuri", xp: 20, packs: 2, ratings: 0 },
    { login: "cy", xp: 2, packs: 0, ratings: 1 },
  ];
  signedIn = true;
  checkIn = "done";
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "community_check_in") return Promise.resolve({ day: utcToday(), status: checkIn });
    if (cmd === "github_status") return Promise.resolve({ configured: true, signedIn, login: signedIn ? "JackUri" : null });
    if (cmd === "community_xp") return Promise.resolve(players);
    if (cmd === "github_sign_in") return Promise.resolve({ userCode: "ABCD-1234", expiresIn: 900 });
    if (cmd === "github_sign_in_wait") return new Promise(() => undefined);
    return Promise.resolve();
  });
});

describe("packer levels (v1.32.0)", () => {
  it("levels up every 10 XP more than the last, to 99", () => {
    expect([1, 2, 3, 4, 5].map(xpFor)).toEqual([0, 10, 30, 60, 100]);
    expect(levelFor(0)).toBe(1);
    expect(levelFor(9)).toBe(1);
    expect(levelFor(10)).toBe(2);
    expect(levelFor(29)).toBe(2);
    expect(levelFor(30)).toBe(3);
    expect(levelFor(450)).toBe(10);
    expect(levelFor(48509)).toBe(98);
    expect(levelFor(48510)).toBe(99);
    expect(levelFor(1e9)).toBe(99);
    expect(standing(20)).toEqual({ level: 2, title: "Recruit Packer", into: 10, need: 20, max: false });
    expect(standing(1e6)).toMatchObject({ level: 99, title: "Pack God", max: true });
    expect([1, 3, 5, 10, 20, 30, 45, 60, 99].map(titleFor)).toEqual([
      "Recruit Packer",
      "Pack Rat",
      "Intern Packer",
      "Pro Packer",
      "Pack Master",
      "Leader of the Pack",
      "Pack King",
      "Pack Legend",
      "Pack God",
    ]);
    expect(titleFor(44)).toBe("Leader of the Pack");
    expect(nextTitle(2)).toEqual({ level: 3, title: "Pack Rat" });
    expect(nextTitle(99)).toBeNull();
  });

  it("celebrates only a level that went up since this PC last saw it", () => {
    expect(levelUp("Sam", 2)).toBeNull();
    expect(levelUp("sam", 2)).toBeNull();
    expect(levelUp("sam", 4)).toBe(4);
    expect(levelUp("sam", 3)).toBeNull();
  });

  it("shows your level, title and the leaderboard for your GitHub name", async () => {
    render(<PackerBadge />);
    const badge = await screen.findByRole("button", { name: "Packer level 2, Recruit Packer" });
    fireEvent.click(badge);
    const panel = screen.getByRole("dialog", { name: "Your packer level" });
    expect(within(panel).getByText(/JackUri: level 2, 20 XP \(2 packs accepted, 0 rated\) · #2 on the leaderboard\. Pack Rat at level 3\./)).toBeInTheDocument();
    expect(within(panel).getByText("XP 10/20")).toBeInTheDocument();
    const board = within(panel).getByRole("list", { name: "Leaderboard" });
    expect(within(board).getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "1alexPro PackerLVL 10500 XP",
      "2jackuriRecruit PackerLVL 220 XP",
      "3cyRecruit PackerLVL 12 XP",
    ]);
    expect(within(panel).getByText(/for each pack or web cheats you share/)).toHaveTextContent("10 XP for each pack");
    fireEvent.click(within(panel).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog", { name: "Your packer level" })).not.toBeInTheDocument();
  });

  it("says LEVEL UP when the robot counted enough new XP", async () => {
    localStorage.setItem("juicebox.packerLevel", JSON.stringify({ jackuri: 1 }));
    render(<PackerBadge />);
    const up = await screen.findByRole("dialog", { name: "Level up" });
    expect(up).toHaveTextContent("LEVEL UP!");
    expect(up).toHaveTextContent("Recruit Packer");
    fireEvent.click(within(up).getByRole("button", { name: "OK" }));
    expect(screen.queryByRole("dialog", { name: "Level up" })).not.toBeInTheDocument();
  });

  it("counts check-in days in a row, with a reward every 5th (v1.33.0)", () => {
    const today = "2026-11-13";
    const p = (lastDay: string | null, streak: number): Player => ({ login: "sam", xp: 0, packs: 0, ratings: 0, days: 9, streak, lastDay });
    expect(streakWithToday(p("2026-11-12", 4), today)).toBe(5);
    expect(streakWithToday(p("2026-11-13", 3), today)).toBe(3);
    expect(streakWithToday(p("2026-11-10", 7), today)).toBe(1);
    expect(streakWithToday(null, today)).toBe(1);
    expect([1, 4, 5, 9, 10].map(streakReward)).toEqual([false, false, true, false, true]);
    expect(utcToday(new Date("2026-11-13T23:30:00-05:00"))).toBe("2026-11-14");
  });

  it("checks in once a day when it's on, and shows the 5-day reward", async () => {
    const yesterday = new Date(Date.now() - 86400000).toISOString().slice(0, 10);
    players[1] = { ...players[1]!, days: 4, streak: 4, lastDay: yesterday };
    localStorage.setItem("juicebox.checkIn", "on");
    const { unmount } = render(<PackerBadge />);
    const card = await screen.findByRole("dialog", { name: "Daily check-in" });
    expect(card).toHaveTextContent("DAILY CHECK-IN");
    expect(card).toHaveTextContent("+2 XP");
    expect(card).toHaveTextContent("Day 5 in a row!");
    expect(card).toHaveTextContent("5-day streak: +10 XP!");
    expect(within(card).getByRole("img", { name: "5 of 5 days in a row" })).toBeInTheDocument();
    fireEvent.click(within(card).getByRole("button", { name: "OK" }));
    unmount();
    // Opened again the same day: not again.
    render(<PackerBadge />);
    await screen.findByRole("button", { name: /Packer level 2/ });
    await new Promise((r) => setTimeout(r, 50));
    expect(invoke.mock.calls.filter(([c]) => c === "community_check_in")).toHaveLength(1);
  });

  it("checks in only once the player turns it on", async () => {
    render(<PackerBadge />);
    fireEvent.click(await screen.findByRole("button", { name: /Packer level 2/ }));
    await new Promise((r) => setTimeout(r, 50));
    expect(invoke).not.toHaveBeenCalledWith("community_check_in", undefined);
    const panel = screen.getByRole("dialog", { name: "Your packer level" });
    const daily = within(panel).getByRole("group", { name: "Daily check-in" });
    expect(daily).toHaveTextContent("which anyone can see");
    fireEvent.click(within(daily).getByRole("checkbox"));
    fireEvent.click(within(panel).getByRole("button", { name: "Close" }));
    const card = await screen.findByRole("dialog", { name: "Daily check-in" });
    expect(card).toHaveTextContent("Day 1 in a row");
    expect(card).toHaveTextContent("4 more days for +10 XP");
    expect(localStorage.getItem("juicebox.checkIn")).toBe("on");
  });

  it("waits when today's check-in page isn't open yet", async () => {
    checkIn = "notYet";
    localStorage.setItem("juicebox.checkIn", "on");
    render(<PackerBadge />);
    await screen.findByRole("button", { name: /Packer level 2/ });
    await new Promise((r) => setTimeout(r, 50));
    expect(invoke).toHaveBeenCalledWith("community_check_in", undefined);
    expect(screen.queryByRole("dialog", { name: "Daily check-in" })).not.toBeInTheDocument();
  });

  it("offers the GitHub sign-in when you aren't signed in", async () => {
    signedIn = false;
    render(<PackerBadge />);
    fireEvent.click(await screen.findByRole("button", { name: "Packer level" }));
    const panel = screen.getByRole("dialog", { name: "Your packer level" });
    await act(async () => {
      fireEvent.click(within(panel).getByRole("button", { name: "Sign in with GitHub (free)" }));
    });
    expect(await within(panel).findByText("ABCD-1234")).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Level up" })).not.toBeInTheDocument();
  });
});
