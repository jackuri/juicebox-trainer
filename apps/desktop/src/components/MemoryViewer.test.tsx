import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MemoryView } from "../lib/ipc";
import { MemoryViewer } from "./MemoryViewer";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));

const BASE = 0x7ff6_1234_0000;

function memory(gold: number): MemoryView {
  const bytes = new Array<number>(256).fill(0);
  const readable = new Array<boolean>(256).fill(true);
  [gold & 0xff, (gold >> 8) & 0xff, (gold >> 16) & 0xff, (gold >> 24) & 0xff].forEach((b, i) => {
    bytes[0x10 + i] = b;
  });
  "Hero".split("").forEach((c, i) => (bytes[0x20 + i] = c.charCodeAt(0)));
  for (let i = 0xf0; i < 0x100; i++) readable[i] = false;
  return {
    address: BASE,
    bytes,
    readable,
    region: {
      base: BASE,
      size: 0x1000,
      protect: 4,
      readable: true,
      writable: true,
      executable: false,
      image: true,
    },
    module: "game.exe+0x49200",
  };
}

let gold = 250;

beforeEach(() => {
  gold = 250;
  invoke.mockReset();
  invoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "memory_view":
        return Promise.resolve(memory(gold));
      case "memory_edit":
        gold = 1000;
        return Promise.resolve("data");
      default:
        return Promise.resolve(undefined);
    }
  });
});

afterEach(() => vi.useRealTimers());

function renderViewer(active = true) {
  const onAddToList = vi.fn();
  render(
    <MemoryViewer
      active={active}
      arch="x64"
      request={null}
      onAddToList={onAddToList}
      onSessionLost={vi.fn()}
    />,
  );
  return { onAddToList };
}

describe("MemoryViewer", () => {
  it("shows rows of bytes, text and the region", async () => {
    renderViewer();
    fireEvent.change(screen.getByLabelText("Memory address"), { target: { value: "game.exe+0x49200" } });
    fireEvent.click(screen.getByRole("button", { name: "Go" }));
    expect(await screen.findByText(/readable, writable · game.exe\+0x49200/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("memory_view", { address: "game.exe+0x49200", len: 256 });
    expect(screen.getByRole("button", { name: `Byte at 0x${(BASE + 0x10).toString(16).toUpperCase()}` })).toHaveTextContent("FA");
    expect(screen.getByText(/Hero/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: `Byte at 0x${(BASE + 0xff).toString(16).toUpperCase()}` })).toHaveTextContent("??");
  });

  it("inspects a byte, writes new bytes and adds it to the list", async () => {
    const { onAddToList } = renderViewer();
    fireEvent.change(screen.getByLabelText("Memory address"), { target: { value: "0x1" } });
    fireEvent.click(screen.getByRole("button", { name: "Go" }));
    const at = BASE + 0x10;
    fireEvent.click(await screen.findByRole("button", { name: `Byte at 0x${at.toString(16).toUpperCase()}` }));
    expect(screen.getByText("32-bit").nextElementSibling).toHaveTextContent("250 / 250");
    expect(screen.getByLabelText("New bytes")).toHaveValue("FA 00 00 00");
    fireEvent.change(screen.getByLabelText("New bytes"), { target: { value: "E8 03 00 00" } });
    fireEvent.click(screen.getByRole("button", { name: "Write bytes" }));
    expect(await screen.findByText("Changed.")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("memory_edit", {
      address: `0x${at.toString(16).toUpperCase()}`,
      hex: "E8 03 00 00",
    });
    fireEvent.click(screen.getByRole("button", { name: "Add to address list" }));
    expect(onAddToList).toHaveBeenCalledWith(at, "i32");
  });

  it("refreshes every second and lights up changed bytes", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    renderViewer();
    fireEvent.change(screen.getByLabelText("Memory address"), { target: { value: "0x1" } });
    fireEvent.click(screen.getByRole("button", { name: "Go" }));
    const cell = await screen.findByRole("button", { name: `Byte at 0x${(BASE + 0x10).toString(16).toUpperCase()}` });
    expect(cell).toHaveTextContent("FA");
    gold = 1000;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1100);
    });
    const now = screen.getByRole("button", { name: `Byte at 0x${(BASE + 0x10).toString(16).toUpperCase()}` });
    expect(now).toHaveTextContent("E8");
    expect(now.className).toContain("text-amber-300");
  });

  it("opens an address sent from the address list", async () => {
    render(
      <MemoryViewer
        active
        arch="x64"
        request={{ address: "0x12345678", n: 1 }}
        onAddToList={vi.fn()}
        onSessionLost={vi.fn()}
      />,
    );
    expect(await screen.findByText(/game.exe\+0x49200/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("memory_view", { address: "0x12345678", len: 256 });
  });
});
