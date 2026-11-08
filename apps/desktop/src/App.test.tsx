import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import type { ProcessInfo, SessionInfo, Verdict } from "./lib/ipc";

const testbed: ProcessInfo = {
  pid: 1234,
  parentPid: 1,
  name: "testbed_native.exe",
  exePath: "C:\\dev\\opentrainer\\target\\debug\\testbed_native.exe",
  arch: "x64",
  threads: 1,
};
const onlineGame: ProcessInfo = {
  pid: 5678,
  parentPid: 1,
  name: "cs2.exe",
  exePath: "D:\\Steam\\cs2\\cs2.exe",
  arch: "x64",
  threads: 30,
};

const allowed: Verdict = {
  pid: 1234,
  target: "testbed_native.exe",
  allowed: true,
  findings: [],
  signaturesVersion: 1,
};
const blocked: Verdict = {
  pid: 5678,
  target: "cs2.exe",
  allowed: false,
  findings: [{ kind: "onlineOnly", product: "Counter-Strike 2", detail: "cs2.exe" }],
  signaturesVersion: 1,
};
const session: SessionInfo = {
  process: testbed,
  modules: [
    { name: "testbed_native.exe", path: testbed.exePath!, base: 0x7ff6a0000000, size: 0x40000 },
    { name: "ntdll.dll", path: "C:\\Windows\\System32\\ntdll.dll", base: 0x7ffb10000000, size: 0x200000 },
  ],
};

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "app_info":
        return Promise.resolve({ version: "9.9.9", coreVersion: "9.9.9" });
      case "list_processes":
        return Promise.resolve([testbed, onlineGame]);
      case "running_games":
        return Promise.resolve([
          { pid: 1234, name: "OpenTrainer testbed", libraryId: null },
          { pid: 5678, name: "Counter-Strike 2", libraryId: "steam:730" },
        ]);
      case "current_session":
        return Promise.resolve(null);
      case "check_guard":
        return Promise.resolve(args?.pid === 1234 ? allowed : blocked);
      case "attach":
        return Promise.resolve(session);
      case "read_value":
        return Promise.resolve({ address: 0x1f0a10, value: "250", bytes: "FA 00 00 00" });
      case "address_list":
        return Promise.resolve([]);
      case "recovery_report":
        return Promise.resolve([]);
      case "pointer_results":
        return Promise.resolve(null);
      case "saves_overview":
        return Promise.resolve({ games: [], steamRunning: false, root: null });
      case "write_value":
        return Promise.resolve({ address: 0x1f0a10, value: String(args?.value), bytes: "0F 27 00 00" });
      default:
        return Promise.reject({ kind: "unknown", message: `unexpected ${cmd}`, verdict: null });
    }
  });
});

describe("App", () => {
  it("shows the version and the process list", async () => {
    render(<App />);
    expect(await screen.findByTestId("version")).toHaveTextContent("v9.9.9");
    expect(await screen.findByRole("option", { name: /testbed_native\.exe/ })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /cs2\.exe/ })).toBeInTheDocument();
  });

  it("reports code changes undone after a crash, once", async () => {
    const base = invoke.getMockImplementation()!;
    invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) =>
      cmd === "recovery_report"
        ? Promise.resolve([{ exe: "testbed_native.exe", pid: 77, gameClosed: false, restored: 2, problems: [] }])
        : base(cmd, args),
    );
    render(<App />);
    const note = await screen.findByText(/closed unexpectedly/);
    expect(note.parentElement).toHaveTextContent("testbed_native.exe (PID 77): 2 changes undone");
    fireEvent.click(screen.getByRole("button", { name: "OK" }));
    expect(screen.queryByText(/closed unexpectedly/)).not.toBeInTheDocument();
  });

  it("blocks attaching when the guard finds anti-cheat or an online-only game", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("option", { name: /cs2\.exe/ }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("will not attach");
    expect(alert).toHaveTextContent("Counter-Strike 2: online-only game");
    expect(screen.getByRole("button", { name: "Attach" })).toBeDisabled();
  });

  it("attaches to an allowed process, then reads and writes a value", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("option", { name: /testbed_native\.exe/ }));
    expect(await screen.findByRole("status")).toHaveTextContent("No anti-cheat found");

    const attach = screen.getByRole("button", { name: "Attach" });
    expect(attach).toBeEnabled();
    fireEvent.click(attach);

    expect(await screen.findByText(/Attached to testbed_native\.exe/)).toBeInTheDocument();
    // The trainer opens first; with no matching pack it says where packs go.
    expect(screen.getByRole("tab", { name: "Trainer" })).toHaveAttribute("aria-selected", "true");
    fireEvent.click(screen.getByRole("tab", { name: "Modules" }));
    const modules = screen.getByRole("region", { name: "Modules" });
    expect(within(modules).getByText("0x7FF6A0000000")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "Read / write" }));
    fireEvent.change(screen.getByLabelText("Address"), { target: { value: "0x1F0A10" } });
    fireEvent.click(screen.getByRole("button", { name: "Read" }));
    expect(await screen.findByDisplayValue("250")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("read_value", { address: "0x1F0A10", ty: "i32" });

    fireEvent.change(screen.getByLabelText("Value"), { target: { value: "9999" } });
    fireEvent.click(screen.getByRole("button", { name: "Write" }));
    expect(await screen.findByDisplayValue("9999")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("write_value", {
      address: "0x1F0A10",
      ty: "i32",
      value: "9999",
    });
  });

  it("drops the session and shows the verdict when the guard blocks mid-session", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("option", { name: /testbed_native\.exe/ }));
    fireEvent.click(await screen.findByRole("button", { name: "Attach" }));
    await screen.findByText(/Attached to/);

    // Only the read is blocked: background calls (trainer polling) must not
    // take the rejection first.
    const base = invoke.getMockImplementation()!;
    invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) =>
      cmd === "read_value"
        ? Promise.reject({
            kind: "blocked",
            message: "blocked",
            verdict: {
              ...allowed,
              allowed: false,
              findings: [{ kind: "process", product: "Easy Anti-Cheat", detail: "EasyAntiCheat.exe" }],
            },
          })
        : base(cmd, args),
    );
    fireEvent.click(screen.getByRole("tab", { name: "Read / write" }));
    fireEvent.change(screen.getByLabelText("Address"), { target: { value: "0x10" } });
    fireEvent.click(screen.getByRole("button", { name: "Read" }));

    expect(await screen.findByText("Not attached")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("Easy Anti-Cheat: process running");
  });

  it("switches to save backups and back", async () => {
    render(<App />);
    await screen.findByRole("option", { name: /testbed_native\.exe/ });
    fireEvent.click(screen.getByRole("button", { name: "Save backups" }));
    expect(await screen.findByText(/No games yet/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save backups" })).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: "Cheats" }));
    expect(await screen.findByRole("option", { name: /testbed_native\.exe/ })).toBeInTheDocument();
  });
});
