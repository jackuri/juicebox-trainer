import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AboutView } from "./AboutView";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

function status(enabled: boolean) {
  return {
    version: "1.0.0",
    enabled,
    offReason: enabled ? null : "Automatic updates are off in this copy.",
    releasesUrl: "https://example.invalid/releases",
  };
}

const ai = { available: true, problem: null, running: false, transcript: [], signedIn: true, installing: false };

beforeEach(() => invoke.mockReset());

describe("AboutView", () => {
  it("says why updates are off and links the releases page", async () => {
    invoke.mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "update_status" ? status(false) : { ...ai, download: "notNeeded" }),
    );
    render(<AboutView />);
    expect(await screen.findByText("Automatic updates are off in this copy.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Check for updates" })).not.toBeInTheDocument();
    expect(screen.getByText(/any game with anti-cheat/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Releases page" }));
    expect(invoke).toHaveBeenCalledWith("open_link", { which: "releases" });
    expect(screen.queryByRole("button", { name: "Delete the AI engine" })).not.toBeInTheDocument();
  });

  it("checks for an update and installs it", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "update_status") return Promise.resolve(status(true));
      if (cmd === "update_check") return Promise.resolve({ version: "1.0.1", notes: "Fixes", date: "2026-11-20" });
      if (cmd === "ai_status") return Promise.resolve({ ...ai, download: "ready" });
      return Promise.resolve();
    });
    render(<AboutView />);
    fireEvent.click(await screen.findByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("OpenTrainer 1.0.1 is available (2026-11-20).")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Install and restart" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("update_install", undefined));

    fireEvent.click(screen.getByRole("button", { name: "Delete the AI engine" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("ai_uninstall", undefined));
  });

  it("says when this is the newest version", async () => {
    invoke.mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "update_status" ? status(true) : cmd === "update_check" ? null : { ...ai, download: "notNeeded" }),
    );
    render(<AboutView />);
    fireEvent.click(await screen.findByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("You have the newest version.")).toBeInTheDocument();
  });
});
