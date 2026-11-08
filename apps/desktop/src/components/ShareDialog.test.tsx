import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { GithubStatus, PackSummary } from "../lib/ipc";
import { ShareDialog } from "./ShareDialog";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

const pack: PackSummary = {
  key: "C:\\packs\\coin-quest\\pack.json",
  source: "user",
  path: "C:\\packs\\coin-quest\\pack.json",
  id: "coin-quest",
  name: "Coin Quest",
  exe: "coinquest.exe",
  arch: "x64",
  author: "ai",
  ai: true,
  packVersion: "1.0.0",
  cheats: 3,
  error: null,
  issues: [],
};

function mock(status: GithubStatus, extra: Record<string, () => Promise<unknown>> = {}) {
  invoke.mockImplementation((cmd: string) => {
    if (extra[cmd]) return extra[cmd]();
    if (cmd === "github_status") return Promise.resolve(status);
    if (cmd === "community_share")
      return Promise.resolve({ url: "https://github.com/jackuri/opentrainer-packs/pull/7", number: 7, update: false });
    return Promise.resolve(undefined);
  });
}

beforeEach(() => {
  invoke.mockReset();
  Object.assign(navigator, { clipboard: { writeText: vi.fn().mockResolvedValue(undefined) } });
});

describe("ShareDialog", () => {
  it("signs in with a code, then shares after the player confirms", async () => {
    let approve: (login: string) => void = () => undefined;
    mock(
      { configured: true, signedIn: false, login: null },
      {
        github_sign_in: () => Promise.resolve({ userCode: "ABCD-1234", expiresIn: 900 }),
        github_sign_in_wait: () => new Promise((r) => (approve = r)),
      },
    );
    render(<ShareDialog pack={pack} onClose={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Sign in with GitHub" }));
    expect(await screen.findByTestId("device-code")).toHaveTextContent("ABCD-1234");
    fireEvent.click(screen.getByRole("button", { name: "Copy the code and open GitHub" }));
    await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalledWith("ABCD-1234"));
    expect(invoke).toHaveBeenCalledWith("github_open_device_page", undefined);

    approve("sam");
    expect(await screen.findByText(/Signed in to GitHub as sam/)).toBeInTheDocument();
    const shareButton = screen.getByRole("button", { name: "Share" });
    expect(shareButton).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Tested on"), { target: { value: "Steam, v1.2" } });
    fireEvent.click(screen.getByLabelText(/I tried every cheat/));
    fireEvent.click(shareButton);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("community_share", { key: pack.key, note: "Steam, v1.2", tableOk: false }),
    );
    expect(await screen.findByText(/Sent! A maintainer will check it/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "See it on GitHub" }));
    expect(invoke).toHaveBeenCalledWith("github_open_pr", { url: "https://github.com/jackuri/opentrainer-packs/pull/7" });
  });

  it("asks for the table author's permission for a pack made from a Cheat Engine table", async () => {
    mock({ configured: true, signedIn: true, login: "sam" });
    render(<ShareDialog pack={{ ...pack, author: "Cheat Engine table (imported)", ai: false }} onClose={vi.fn()} />);
    fireEvent.click(await screen.findByLabelText(/I tried every cheat/));
    expect(screen.getByRole("button", { name: "Share" })).toBeDisabled();
    fireEvent.click(screen.getByLabelText(/I made that table/));
    fireEvent.click(screen.getByRole("button", { name: "Share" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("community_share", { key: pack.key, note: "", tableOk: true }),
    );
  });

  it("asks to sign in again when GitHub forgot the sign-in", async () => {
    mock(
      { configured: true, signedIn: true, login: "sam" },
      { community_share: () => Promise.reject({ kind: "signedOut", message: "sign in again", verdict: null }) },
    );
    render(<ShareDialog pack={pack} onClose={vi.fn()} />);
    fireEvent.click(await screen.findByLabelText(/I tried every cheat/));
    fireEvent.click(screen.getByRole("button", { name: "Share" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("sign in again");
    expect(screen.getByRole("button", { name: "Sign in with GitHub" })).toBeInTheDocument();
  });

  it("says when this version can't share from the app", async () => {
    mock({ configured: false, signedIn: false, login: null });
    render(<ShareDialog pack={pack} onClose={vi.fn()} />);
    expect(await screen.findByText(/isn't set up in this version/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Show me how" }));
    expect(invoke).toHaveBeenCalledWith("open_link", { which: "contribute" });
  });

  it("cancels a sign-in in progress when closed", async () => {
    const onClose = vi.fn();
    mock(
      { configured: true, signedIn: false, login: null },
      {
        github_sign_in: () => Promise.resolve({ userCode: "WXYZ-0000", expiresIn: 900 }),
        github_sign_in_wait: () => new Promise(() => undefined),
      },
    );
    render(<ShareDialog pack={pack} onClose={onClose} />);
    fireEvent.click(await screen.findByRole("button", { name: "Sign in with GitHub" }));
    await screen.findByTestId("device-code");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(invoke).toHaveBeenCalledWith("github_sign_in_cancel", undefined);
    expect(onClose).toHaveBeenCalled();
  });
});
