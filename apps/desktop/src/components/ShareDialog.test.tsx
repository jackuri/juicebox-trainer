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
      return Promise.resolve({ url: "https://github.com/jackuri/juicebox-packs/pull/7", number: 7, update: false });
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
      expect(invoke).toHaveBeenCalledWith("community_share", {
        key: pack.key,
        note: "Steam, v1.2",
        permission: false,
        // An AI-made pack: no maker badge; MIT by default.
        terms: { fromMaker: false, licence: undefined, support: undefined },
      }),
    );
    expect(await screen.findByText(/Sent! A maintainer will check it/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "See it on GitHub" }));
    expect(invoke).toHaveBeenCalledWith("github_open_pr", { url: "https://github.com/jackuri/juicebox-packs/pull/7" });
  });

  it("a table's maker shares it with their own terms and support page", async () => {
    mock({ configured: true, signedIn: true, login: "sam" });
    render(<ShareDialog pack={{ ...pack, author: "Cheat Engine table (imported)", ai: false }} onClose={vi.fn()} />);
    fireEvent.click(await screen.findByLabelText(/I tried every cheat/));
    expect(screen.getByText(/Who made the table\?/)).toBeInTheDocument();
    // Who made it must be answered first.
    expect(screen.getByRole("button", { name: "Share" })).toBeDisabled();
    fireEvent.click(screen.getByLabelText("I did"));
    fireEvent.click(screen.getByLabelText(/the maker keeps all rights/));
    fireEvent.change(screen.getByLabelText("Support page"), { target: { value: "https://ko-fi.com/sam" } });
    fireEvent.click(screen.getByRole("button", { name: "Share" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("community_share", {
        key: pack.key,
        note: "",
        permission: false,
        terms: { fromMaker: true, licence: "Shared with permission; all rights kept by the maker", support: "https://ko-fi.com/sam" },
      }),
    );
  });

  it("knows a credited table pack came from a table", async () => {
    mock({ configured: true, signedIn: true, login: "sam" });
    render(
      <ShareDialog
        pack={{ ...pack, source: "imported", author: "TableMaker", ai: false, credit: { by: "TableMaker", file: "Coin_Quest.CT" } }}
        onClose={vi.fn()}
      />,
    );
    expect(await screen.findByText(/This pack came from a Cheat Engine table. Who made the table\?/)).toBeInTheDocument();
  });

  it("someone else's pack needs their permission and the terms they agreed to", async () => {
    mock({ configured: true, signedIn: true, login: "sam" });
    render(<ShareDialog pack={{ ...pack, source: "imported", author: "pat", ai: false }} onClose={vi.fn()} />);
    fireEvent.click(await screen.findByLabelText(/I tried every cheat/));
    expect(screen.getByText(/This pack came from a file. Who made it\?/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Share" })).toBeDisabled();
    fireEvent.click(screen.getByLabelText(/Someone else, and they said it may be shared/));
    expect(screen.getByRole("button", { name: "Share" })).toBeEnabled();
    // Other terms need words.
    fireEvent.click(screen.getByLabelText(/^Other terms:/));
    expect(screen.getByRole("button", { name: "Share" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Other terms"), { target: { value: "CC BY 4.0" } });
    fireEvent.click(screen.getByRole("button", { name: "Share" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("community_share", {
        key: pack.key,
        note: "",
        permission: true,
        terms: { fromMaker: false, licence: "CC BY 4.0", support: undefined },
      }),
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

  it("shares only the table's link in link mode", async () => {
    mock({ configured: true, signedIn: true, login: "sam" }, {
      community_share_table: () =>
        Promise.resolve({ url: "https://github.com/jackuri/juicebox-packs/pull/9", number: 9, update: false }),
    });
    const linked: PackSummary = {
      ...pack,
      source: "imported",
      author: "TableMaker",
      ai: false,
      credit: { by: "TableMaker", url: "https://fearlessrevolution.com/viewtopic.php?t=5", file: "Coin_Quest.CT" },
      tableLink: true,
    };
    render(<ShareDialog pack={linked} mode="link" onClose={vi.fn()} />);
    expect(await screen.findByRole("dialog", { name: "Share the table's link" })).toHaveTextContent("made by TableMaker");
    expect(screen.getByText("https://fearlessrevolution.com/viewtopic.php?t=5")).toBeInTheDocument();
    // No permission or version questions: it's only a link.
    expect(screen.queryByLabelText("Tested on")).not.toBeInTheDocument();
    const send = screen.getByRole("button", { name: "Share the link" });
    expect(send).toBeDisabled();
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(send);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("community_share_table", { key: linked.key, file: null, licence: null }),
    );
    expect(await screen.findByText(/Sent!/)).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("community_share", expect.anything());
  });

  it("shares the table itself too, only with its licence and the maker's OK (v1.27.0)", async () => {
    mock({ configured: true, signedIn: true, login: "sam" }, {
      pick_table: () => Promise.resolve(String.raw`C:\Downloads\Coin_Quest.CT`),
      community_share_table: () =>
        Promise.resolve({ url: "https://github.com/jackuri/juicebox-packs/pull/10", number: 10, update: false }),
    });
    const linked: PackSummary = {
      ...pack,
      source: "imported",
      author: "TableMaker",
      ai: false,
      credit: { by: "TableMaker", url: "https://github.com/maker/table", file: "Coin_Quest.CT" },
      tableLink: true,
    };
    render(<ShareDialog pack={linked} mode="link" onClose={vi.fn()} />);
    fireEvent.click(await screen.findByLabelText(/the table worked for me/));
    fireEvent.click(screen.getByLabelText(/Also put the table on the list/));
    const send = screen.getByRole("button", { name: "Share the table and its link" });
    expect(send).toBeDisabled();
    fireEvent.change(screen.getByLabelText(/Its licence/), { target: { value: "MIT" } });
    fireEvent.click(screen.getByRole("button", { name: "Choose the .CT file" }));
    expect(await screen.findByText("Coin_Quest.CT")).toBeInTheDocument();
    expect(send).toBeDisabled();
    fireEvent.click(screen.getByLabelText(/Its maker allows this/));
    fireEvent.click(send);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("community_share_table", {
        key: linked.key,
        file: String.raw`C:\Downloads\Coin_Quest.CT`,
        licence: "MIT",
      }),
    );
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
