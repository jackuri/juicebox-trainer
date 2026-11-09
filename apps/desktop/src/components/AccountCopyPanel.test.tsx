import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AccountsView, CopyPlan, SteamAccount } from "../lib/ipc";
import { accountLabel } from "../lib/format";
import { AccountCopyPanel } from "./AccountCopyPanel";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));

const main: SteamAccount = {
  id64: "76561197971489072",
  id32: 11223344,
  name: "Main",
  login: "main_login",
  onThisPc: true,
};
const alt: SteamAccount = {
  id64: "76561198015933516",
  id32: 55667788,
  name: "Alt",
  login: "alt_login",
  onThisPc: true,
};

const view: AccountsView = {
  withSaves: [
    {
      account: main,
      folders: [{ path: "C:\\Steam\\userdata\\11223344\\4242\\remote", listed: "x" }],
      files: 2,
      bytes: 2048,
    },
  ],
  accounts: [main, alt],
};

const plan: CopyPlan = {
  from: main,
  to: alt,
  files: [
    {
      from: "C:\\Steam\\userdata\\11223344\\4242\\remote\\slot1.json",
      to: "C:\\Steam\\userdata\\55667788\\4242\\remote\\slot1.json",
      size: 1024,
      replaces: true,
      ids: 1,
    },
    {
      from: "C:\\Steam\\userdata\\11223344\\4242\\remote\\settings.ini",
      to: "C:\\Steam\\userdata\\55667788\\4242\\remote\\settings.ini",
      size: 1024,
      replaces: false,
      ids: 0,
    },
  ],
  newFolders: [],
  cloudWarning: null,
};

beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "saves_accounts":
        return Promise.resolve(view);
      case "saves_account_plan":
        return Promise.resolve(plan);
      case "saves_account_copy":
        return Promise.resolve({
          backup: "5000",
          copied: 2,
          replaced: 1,
          ids: 1,
          addFolders: [],
          cloudWarning: null,
        });
      default:
        return Promise.resolve(undefined);
    }
  });
});

describe("AccountCopyPanel", () => {
  it("labels accounts by name and sign-in name", () => {
    expect(accountLabel(main)).toBe("Main (main_login)");
    expect(accountLabel({ ...main, name: null, login: null })).toBe(main.id64);
  });

  it("checks first, then copies to the other account", async () => {
    const onCopied = vi.fn();
    render(<AccountCopyPanel gameId="g" running={false} reloadKey="1" onCopied={onCopied} />);
    fireEvent.click(await screen.findByRole("button", { name: "Check what will be copied" }));
    expect(invoke).toHaveBeenCalledWith("saves_account_plan", {
      id: "g",
      from: main.id64,
      to: alt.id64,
    });
    expect(await screen.findByText(/2 files will be copied to Alt \(alt_login\)/)).toBeInTheDocument();
    expect(screen.getByText(/already has 1 of these file;/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Copy to Alt (alt_login)" }));
    expect(await screen.findByText(/Copied 2 files \(1 replaced\)/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("saves_account_copy", {
      id: "g",
      from: main.id64,
      to: alt.id64,
    });
    expect(onCopied).toHaveBeenCalled();
  });

  it("won't copy while the game runs", async () => {
    render(<AccountCopyPanel gameId="g" running={true} reloadKey="1" onCopied={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Check what will be copied" }));
    expect(await screen.findByRole("button", { name: "Copy to Alt (alt_login)" })).toBeDisabled();
    expect(screen.getByText("Close the game first.")).toBeInTheDocument();
  });

  it("is hidden when only one account is known", async () => {
    invoke.mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "saves_accounts" ? { ...view, accounts: [main] } : undefined),
    );
    const { container } = render(
      <AccountCopyPanel gameId="g" running={false} reloadKey="1" onCopied={vi.fn()} />,
    );
    await Promise.resolve();
    await Promise.resolve();
    expect(container).toBeEmptyDOMElement();
  });
});
