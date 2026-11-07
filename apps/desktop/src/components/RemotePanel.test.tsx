import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RemoteStatus } from "../lib/ipc";
import { Overlay } from "./Overlay";
import { RemotePanel } from "./RemotePanel";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

const on: RemoteStatus = {
  pairUrl: "http://192.168.1.20:47110/pair?code=abc",
  qrSvg: "<svg></svg>",
  address: "http://192.168.1.20:47110/",
  devices: 0,
};

let running: RemoteStatus | null;

beforeEach(() => {
  running = null;
  invoke.mockReset();
  invoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "remote_status":
        return Promise.resolve(running);
      case "remote_start":
        running = on;
        return Promise.resolve(on);
      case "remote_forget":
        return Promise.resolve({ ...on, devices: 0 });
      case "remote_stop":
        running = null;
        return Promise.resolve();
      default:
        return Promise.resolve(null);
    }
  });
});

describe("RemotePanel", () => {
  it("is off until turned on, then shows the QR code", async () => {
    render(<RemotePanel />);
    expect(await screen.findByText("Off")).toBeInTheDocument();
    expect(screen.getByText(/same Wi-Fi/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Turn on" }));
    expect(await screen.findByAltText("QR code to pair your phone")).toBeInTheDocument();
    expect(screen.getByText("http://192.168.1.20:47110/")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Unpair phones" }));
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("remote_forget", undefined));
    await vi.waitFor(() => expect(screen.getByRole("button", { name: "Turn off" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "Turn off" }));
    expect(await screen.findByRole("button", { name: "Turn on" })).toBeInTheDocument();
  });

  it("explains a failure to start", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "remote_start"
        ? Promise.reject({ kind: "remote", message: "this PC isn't on a network", verdict: null })
        : Promise.resolve(null),
    );
    render(<RemotePanel />);
    fireEvent.click(await screen.findByRole("button", { name: "Turn on" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("isn't on a network");
  });
});

describe("Overlay", () => {
  it("shows nothing without a pack, then the cheats with keys", async () => {
    let view: unknown = null;
    invoke.mockImplementation((cmd: string) => (cmd === "trainer_state" ? Promise.resolve(view) : Promise.resolve(null)));
    const { rerender } = render(<Overlay />);
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("trainer_state", undefined));
    expect(screen.queryByTestId("overlay")).not.toBeInTheDocument();
    view = {
      pack: {
        game: { name: "Testbed" },
        cheats: [
          { id: "inf_health", name: "Infinite Health", ui: "toggle" },
          { id: "speed", name: "Move Speed", ui: "slider" },
        ],
      },
      cheats: [
        { id: "inf_health", status: "active", active: true, hotkey: "F1", value: null },
        { id: "speed", status: "ready", active: false, hotkey: null, value: "1" },
      ],
      pads: { inf_health: "LB+RB+DPadUp" },
      pending: [],
    };
    rerender(<Overlay />);
    expect(await screen.findByTestId("overlay", {}, { timeout: 2000 })).toHaveTextContent("Testbed");
    expect(screen.getByText("F1 · LB+RB+DPadUp")).toBeInTheDocument();
  });
});
