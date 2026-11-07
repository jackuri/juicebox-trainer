import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PointerResults } from "../lib/ipc";
import { PointerPanel } from "./PointerPanel";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));

const path = (expression: string, address: number | null) => ({
  expression,
  module: "testbed_native.exe",
  moduleOffset: 0x3a000,
  offsets: [0x10, 0x10, 0xc],
  address,
});

const scanned: PointerResults = {
  exe: "testbed_native.exe",
  target: 0x1f0a1c,
  rounds: 1,
  count: 2,
  paths: [
    path("[[[testbed_native.exe+0x3A000]+0x10]+0x10]+0xC", 0x1f0a1c),
    path("[[testbed_native.exe+0x3B000]+0x8]+0x1C", 0x1f0a1c),
  ],
};

beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "pointer_results":
        return Promise.resolve(null);
      case "pointer_scan":
        return Promise.resolve(scanned);
      case "pointer_filter":
        return Promise.resolve({ ...scanned, rounds: 2, count: 1, target: 0x2f0a1c, paths: [scanned.paths[0]] });
      case "scan_progress":
        return Promise.resolve({ running: true, done: 1, total: 2 });
      case "pointer_clear":
        return Promise.resolve();
      default:
        return Promise.reject({ kind: "unknown", message: `unexpected ${cmd} ${JSON.stringify(args)}`, verdict: null });
    }
  });
});

describe("PointerPanel", () => {
  it("scans, narrows after a restart and adds a path to the list", async () => {
    const add = vi.fn();
    render(<PointerPanel onAddToList={add} onSessionLost={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("Target address"), { target: { value: "0x1F0A1C" } });
    fireEvent.change(screen.getByLabelText("Pointer levels"), { target: { value: "3" } });
    fireEvent.click(screen.getByRole("button", { name: "Find pointers" }));
    expect(await screen.findByTestId("pointer-count")).toHaveTextContent("2 paths");
    expect(invoke).toHaveBeenCalledWith("pointer_scan", {
      target: "0x1F0A1C",
      options: { maxDepth: 3, maxOffset: 0x1000, maxResults: 5000 },
    });

    fireEvent.change(screen.getByLabelText("New address"), { target: { value: "0x2F0A1C" } });
    fireEvent.click(screen.getByRole("button", { name: "Check paths" }));
    expect(await screen.findByText("1 path")).toBeInTheDocument();
    expect(screen.getByText(/checked 1 time/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("pointer_filter", { target: "0x2F0A1C" });

    fireEvent.change(screen.getByLabelText("Value type to add"), { target: { value: "f32" } });
    fireEvent.click(
      screen.getByRole("button", { name: "Add [[[testbed_native.exe+0x3A000]+0x10]+0x10]+0xC to address list" }),
    );
    expect(add).toHaveBeenCalledWith("[[[testbed_native.exe+0x3A000]+0x10]+0x10]+0xC", "f32");

    fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(await screen.findByRole("button", { name: "Find pointers" })).toBeInTheDocument();
    expect(screen.queryByTestId("pointer-count")).not.toBeInTheDocument();
  });

  it("takes an address from the Scanner and copies a path as a pack cheat", async () => {
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const { rerender } = render(
      <PointerPanel onAddToList={vi.fn()} onSessionLost={vi.fn()} request={{ address: 0x1f0a1c, ty: "f32" }} />,
    );
    expect(screen.getByLabelText("Target address")).toHaveValue("0x1F0A1C");
    // A new request replaces what's typed.
    rerender(<PointerPanel onAddToList={vi.fn()} onSessionLost={vi.fn()} request={{ address: 0x2000, ty: "f32" }} />);
    expect(screen.getByLabelText("Target address")).toHaveValue("0x2000");

    fireEvent.click(screen.getByRole("button", { name: "Find pointers" }));
    expect(await screen.findByTestId("pointer-count")).toHaveTextContent("2 paths");
    expect(screen.getByLabelText("Value type to add")).toHaveValue("f32");
    fireEvent.click(
      screen.getByRole("button", { name: "Copy [[[testbed_native.exe+0x3A000]+0x10]+0x10]+0xC as a pack cheat" }),
    );
    await vi.waitFor(() => expect(writeText).toHaveBeenCalled());
    const copied = JSON.parse((writeText.mock.calls[0] as unknown as [string])[0]);
    expect(copied.impl).toEqual({
      kind: "value",
      type: "f32",
      address: { base: "testbed_native.exe+0x3A000", offsets: ["0x10", "0x10", "0xC"] },
    });
    expect(await screen.findByText("Copied")).toBeInTheDocument();
  });

  it("shows errors and refuses a bad offset", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "pointer_scan"
        ? Promise.reject({ kind: "invalidAddress", message: "invalid address 'zz'", verdict: null })
        : Promise.resolve(null),
    );
    render(<PointerPanel onAddToList={vi.fn()} onSessionLost={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("Target address"), { target: { value: "zz" } });
    fireEvent.change(screen.getByLabelText("Largest offset"), { target: { value: "xyz" } });
    expect(screen.getByRole("button", { name: "Find pointers" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Largest offset"), { target: { value: "800" } });
    fireEvent.click(screen.getByRole("button", { name: "Find pointers" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("invalid address 'zz'");
  });

  it("reports a lost session", async () => {
    const lost = vi.fn();
    invoke.mockImplementation((cmd: string) =>
      cmd === "pointer_scan"
        ? Promise.reject({ kind: "processExited", message: "gone", verdict: null })
        : Promise.resolve(null),
    );
    render(<PointerPanel onAddToList={vi.fn()} onSessionLost={lost} />);
    fireEvent.change(screen.getByLabelText("Target address"), { target: { value: "0x10" } });
    fireEvent.click(screen.getByRole("button", { name: "Find pointers" }));
    await vi.waitFor(() => expect(lost).toHaveBeenCalled());
  });
});
