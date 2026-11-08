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
  it("in a copy that can't update itself, Update opens the newest release", async () => {
    invoke.mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "update_status" ? status(false) : { ...ai, download: "notNeeded" }),
    );
    render(<AboutView />);
    expect(await screen.findByText("Automatic updates are off in this copy.")).toBeInTheDocument();
    expect(screen.getByText(/any game with anti-cheat/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Update" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("open_link", { which: "latest" }));
    expect(invoke).not.toHaveBeenCalledWith("update_check", undefined);
    fireEvent.click(screen.getByRole("button", { name: "Releases page" }));
    expect(invoke).toHaveBeenCalledWith("open_link", { which: "releases" });
    expect(screen.queryByRole("button", { name: "Delete the AI engine" })).not.toBeInTheDocument();
  });

  it("Update finds the newest version and installs it in one click", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "update_status") return Promise.resolve(status(true));
      if (cmd === "update_check") return Promise.resolve({ version: "1.0.1", notes: "Fixes", date: "2026-11-20" });
      if (cmd === "ai_status") return Promise.resolve({ ...ai, download: "ready" });
      return Promise.resolve();
    });
    render(<AboutView />);
    fireEvent.click(await screen.findByRole("button", { name: "Update" }));
    expect(await screen.findByText("Updating to OpenTrainer 1.0.1 (2026-11-20)…")).toBeInTheDocument();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("update_install", undefined));
    expect(screen.getByRole("button", { name: "Updating…" })).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "Delete the AI engine" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("ai_uninstall", undefined));
  });

  it("says when this is the newest version, and shows a failed download", async () => {
    invoke.mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "update_status" ? status(true) : cmd === "update_check" ? null : { ...ai, download: "notNeeded" }),
    );
    render(<AboutView />);
    fireEvent.click(await screen.findByRole("button", { name: "Update" }));
    expect(await screen.findByText("You have the newest version.")).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("update_install", undefined);

    invoke.mockImplementation((cmd: string) => {
      if (cmd === "update_check") return Promise.resolve({ version: "1.0.1", notes: null, date: null });
      if (cmd === "update_install")
        return Promise.reject({ kind: "update", message: "the download failed or wasn't signed by us", verdict: null });
      return Promise.resolve();
    });
    fireEvent.click(screen.getByRole("button", { name: "Update" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("wasn't signed by us");
    expect(screen.getByRole("button", { name: "Update" })).toBeEnabled();
  });
});