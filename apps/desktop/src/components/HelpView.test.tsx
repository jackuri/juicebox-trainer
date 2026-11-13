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
    render(<HelpView onSeen={onSeen} start="problems" />);
    const card = await screen.findByRole("alert", { name: page.title });
    expect(card).toHaveTextContent("locked at 999 2 seconds before");
    expect(card).toHaveTextContent("What to try");
    expect(card).toHaveTextContent("click Reload game");
    await vi.waitFor(() => expect(onSeen).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith("problems_seen", undefined);
  });

  it("clears the list", async () => {
    render(<HelpView onSeen={vi.fn()} start="problems" />);
    await screen.findByRole("alert", { name: page.title });
    fireEvent.click(screen.getByRole("button", { name: "Clear the list" }));
    expect(await screen.findByText("No problems so far.")).toBeInTheDocument();
  });

  it("answers common questions, with search and topics (v1.34.0)", () => {
    render(<HelpView onSeen={vi.fn()} />);
    fireEvent.click(screen.getByRole("tab", { name: "Common questions" }));
    expect(screen.getByText("My internet stopped working")).toBeInTheDocument();
    expect(screen.getByText(/never changes your network settings/)).toBeInTheDocument();
    fireEvent.change(screen.getByRole("searchbox", { name: "Search the questions" }), { target: { value: "antivirus" } });
    expect(screen.getByText("My antivirus warns about JuiceBox Trainer")).toBeInTheDocument();
    expect(screen.queryByText("My internet stopped working")).not.toBeInTheDocument();
    fireEvent.change(screen.getByRole("searchbox", { name: "Search the questions" }), { target: { value: "qqqq" } });
    expect(screen.getByText(/No question matches that/)).toBeInTheDocument();
    fireEvent.change(screen.getByRole("searchbox", { name: "Search the questions" }), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "Saves" }));
    expect(screen.getByText("How do I get an old save back?")).toBeInTheDocument();
    expect(screen.queryByText("My internet stopped working")).not.toBeInTheDocument();
  });

  it("opens on the tutorial and doesn't mark problems seen until they're looked at (v1.34.0)", async () => {
    const onSeen = vi.fn();
    render(<HelpView onSeen={onSeen} />);
    expect(screen.getByRole("tab", { name: "Tutorial" })).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByLabelText("New problems")).toBeInTheDocument();
    expect(onSeen).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("tab", { name: /Recent problems/ }));
    await screen.findByRole("alert", { name: page.title });
    await vi.waitFor(() => expect(onSeen).toHaveBeenCalled());
  });

  it("walks through the lessons: Show me opens the screen, Done goes on and is remembered (v1.34.0)", () => {
    localStorage.removeItem("juicebox.tutorial");
    const onGo = vi.fn();
    const { unmount } = render(<HelpView onSeen={vi.fn()} onGo={onGo} />);
    expect(screen.getByRole("progressbar", { name: "Lessons done" })).toHaveAttribute("aria-valuenow", "0");
    const first = screen.getByRole("region", { name: "Your first cheats" });
    expect(first).toHaveTextContent("Play with cheats");
    fireEvent.click(screen.getByRole("button", { name: "Show me: Open Games" }));
    expect(onGo).toHaveBeenCalledWith("games");
    fireEvent.click(screen.getByRole("button", { name: "Done: next lesson" }));
    expect(screen.queryByRole("region", { name: "Your first cheats" })).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Make a cheat yourself (no AI)" })).toHaveTextContent("There's no number on screen");
    expect(screen.getByRole("progressbar", { name: "Lessons done" })).toHaveAttribute("aria-valuenow", "1");
    unmount();

    render(<HelpView onSeen={vi.fn()} onGo={onGo} />);
    expect(screen.getByRole("progressbar", { name: "Lessons done" })).toHaveAttribute("aria-valuenow", "1");
    expect(screen.getByRole("region", { name: "Make a cheat yourself (no AI)" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Make cheats with AI/ }));
    expect(screen.getByRole("region", { name: "Make cheats with AI" })).toHaveTextContent("Download the AI engine");
    fireEvent.click(screen.getByRole("button", { name: /^1\. Your first cheats \(done\)/ }));
    fireEvent.click(screen.getByRole("button", { name: "Not done yet" }));
    expect(screen.getByRole("progressbar", { name: "Lessons done" })).toHaveAttribute("aria-valuenow", "0");
    localStorage.removeItem("juicebox.tutorial");
  });
});
