import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Problem } from "../lib/ipc";
import { HelpView } from "./HelpView";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

const page: Problem = {
  id: 1_794_305_000_000,
  kind: "webPage",
  game: "Outbreak",
  title: "The Outbreak page stopped responding",
  why: "The game page is frozen. \"game.player.ammo\" was locked at 999 2 seconds before, so that may have caused it.",
  advice: ["If it doesn't, click Reload game."],
  details: ["Page problem: Frozen"],
  crashed: false,
  seen: false,
};

let problems: Problem[];
beforeEach(() => {
  problems = [page];
  invoke.mockReset();
  invoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "problems_list":
        return Promise.resolve(problems);
      case "problems_seen":
        return Promise.resolve();
      case "problems_clear":
        problems = [];
        return Promise.resolve();
      default:
        return Promise.reject({ kind: "unknown", message: cmd, verdict: null });
    }
  });
});

describe("HelpView", () => {
  it("lists recent problems with why and what to try, and marks them seen", async () => {
    const onSeen = vi.fn();
    render(<HelpView onSeen={onSeen} />);
    const card = await screen.findByRole("alert", { name: page.title });
    expect(card).toHaveTextContent("locked at 999 2 seconds before");
    expect(card).toHaveTextContent("What to try");
    expect(card).toHaveTextContent("click Reload game");
    await vi.waitFor(() => expect(onSeen).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith("problems_seen", undefined);
  });

  it("clears the list", async () => {
    render(<HelpView onSeen={vi.fn()} />);
    await screen.findByRole("alert", { name: page.title });
    fireEvent.click(screen.getByRole("button", { name: "Clear the list" }));
    expect(await screen.findByText("No problems so far.")).toBeInTheDocument();
  });

  it("answers common questions", async () => {
    render(<HelpView onSeen={vi.fn()} />);
    await screen.findByRole("alert", { name: page.title });
    expect(screen.getByText("My internet stopped working")).toBeInTheDocument();
    expect(screen.getByText(/never changes your network settings/)).toBeInTheDocument();
  });
});
