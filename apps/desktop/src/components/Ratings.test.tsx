import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Rating } from "../lib/ipc";
import { bestFirst, myRating, ratingWords, starGlyphs } from "../lib/ratings";
import { RatePack, Stars } from "./Ratings";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));

const r = (stars: number, count: number, votes: [number, number, number] = [0, 0, count], issue = 12): Rating => ({ issue, count, stars, votes });

beforeEach(() => {
  invoke.mockReset();
  localStorage.clear();
});

describe("ratings", () => {
  it("shows stars and what players say", () => {
    const { rerender } = render(<Stars rating={r(2.7, 12, [1, 2, 9])} />);
    expect(screen.getByLabelText("Rated 2.7 of 3 stars by 12 players")).toHaveTextContent("★★★ Works great · 2.7 from 12 players");
    rerender(<Stars rating={r(1.2, 5, [4, 1, 0])} />);
    expect(screen.getByText(/Broken · 1.2 from 5 players/)).toBeInTheDocument();
    rerender(<Stars />);
    expect(screen.getByText("Not rated yet")).toBeInTheDocument();
    expect(ratingWords(2)).toBe("Works, some bugs");
    expect(starGlyphs(1.4)).toBe("★☆☆");
  });

  it("puts the best rated first, then the most rated, unrated last", () => {
    const list = [
      { id: "a", name: "Alpha" },
      { id: "b", name: "Beta" },
      { id: "c", name: "Coin" },
      { id: "d", name: "Delta" },
    ];
    const ratings = { a: r(2, 3), b: r(3, 1), c: r(3, 8), d: r(0, 0, [0, 0, 0]) };
    expect(bestFirst(list, ratings).map((p) => p.id)).toEqual(["c", "b", "a", "d"]);
  });

  it("rates a pack and remembers it", async () => {
    invoke.mockResolvedValue(3);
    render(<RatePack id="coin-quest" name="Coin Quest" rating={r(2, 1)} />);
    fireEvent.click(screen.getByRole("button", { name: "3 stars: Works great" }));
    expect(await screen.findByRole("status")).toHaveTextContent("counts it within the hour");
    expect(invoke).toHaveBeenCalledWith("community_rate", { issue: 12, stars: 3 });
    expect(screen.getByRole("button", { name: "3 stars: Works great" })).toHaveAttribute("aria-pressed", "true");
    expect(myRating("coin-quest")).toBe(3);
  });

  it("signs in to GitHub first when needed, then sends the rating", async () => {
    let signedIn = false;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "community_rate")
        return signedIn ? Promise.resolve(1) : Promise.reject({ kind: "signedOut", message: "sign in to GitHub first", verdict: null });
      if (cmd === "github_sign_in") return Promise.resolve({ userCode: "ABCD-1234", verificationUri: "https://github.com/login/device", expiresIn: 900 });
      if (cmd === "github_sign_in_wait")
        return new Promise((ok) =>
          setTimeout(() => {
            signedIn = true;
            ok("sam");
          }, 50),
        );
      return Promise.resolve();
    });
    render(<RatePack id="coin-quest" name="Coin Quest" rating={r(2, 1)} />);
    fireEvent.click(screen.getByRole("button", { name: "1 star: Broken" }));
    expect(await screen.findByText("ABCD-1234")).toBeInTheDocument();
    expect(screen.getByText(/your 1-star rating is sent after that/)).toBeInTheDocument();
    await waitFor(() => expect(invoke.mock.calls.filter(([c]) => c === "community_rate")).toHaveLength(2));
    expect(await screen.findByText(/counts it within the hour/)).toBeInTheDocument();
    expect(myRating("coin-quest")).toBe(1);
  });

  it("says when a new pack can't be rated yet", () => {
    render(<RatePack id="new" name="New" />);
    expect(screen.getByText(/open within an hour/)).toBeInTheDocument();
  });
});
