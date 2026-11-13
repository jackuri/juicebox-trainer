import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import type { Cheat, Pc, View } from "./lib/ipc";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
// No camera here (like the PC build).
vi.mock("@tauri-apps/plugin-barcode-scanner", () => {
  throw new Error("not a phone");
});

const pc: Pc = { id: "pc1", name: "DESKTOP-1", address: "192.168.1.20:47110" };
const cheat = (c: Partial<Cheat> & Pick<Cheat, "id" | "name" | "ui">): Cheat => ({
  category: null,
  active: false,
  status: "ready",
  message: null,
  value: null,
  min: null,
  max: null,
  step: null,
  label: null,
  ...c,
});
const loaded: View = {
  pack: { game: "Testbed" },
  cheats: [
    cheat({ id: "inf_health", name: "Infinite Health", ui: "toggle", category: "Player" }),
    cheat({ id: "add_gold", name: "Add Gold", ui: "button", label: "+100", category: "Items" }),
    cheat({ id: "speed", name: "Move Speed", ui: "slider", min: 1, max: 5, step: 0.5, value: "1", category: "Player" }),
    cheat({ id: "ammo", name: "Ammo", ui: "number", value: "30", category: "Items" }),
    cheat({ id: "fly", name: "Fly", ui: "toggle", status: "failed", message: "not found in this version" }),
  ],
  presets: ["Boss fight"],
  pending: [],
};

let pcs: Pc[];
let view: View | Error;

function answer(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  switch (cmd) {
    case "pcs_list":
      return Promise.resolve(pcs);
    case "pc_pair":
      if (!String(args?.link).startsWith("http://")) {
        return Promise.reject({ kind: "badLink", message: "that isn't a JuiceBox Trainer pairing link" });
      }
      pcs = [pc];
      return Promise.resolve(pc);
    case "pc_state":
      return view instanceof Error
        ? Promise.reject({ kind: "unreachable", message: "can't reach the PC. Is JuiceBox Trainer open there?" })
        : Promise.resolve(view);
    case "pc_action":
    case "pc_preset":
      return Promise.resolve(view);
    case "pc_forget":
      pcs = [];
      return Promise.resolve();
    default:
      return Promise.reject(new Error(cmd));
  }
}

beforeEach(() => {
  localStorage.clear();
  window.history.replaceState(null, "");
  pcs = [];
  view = { pack: null };
  invoke.mockReset();
  invoke.mockImplementation(answer);
});

describe("JuiceBox Remote", () => {
  it("starts on pairing, explains a bad link, pairs by pasting and opens the remote", async () => {
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Pair with your PC" })).toBeInTheDocument();
    expect(screen.getByText(/same Wi-Fi/)).toBeInTheDocument();
    // No camera: it says so and offers the link box.
    fireEvent.click(screen.getByRole("button", { name: "Scan the QR code" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't use the camera");
    const box = screen.getByRole("textbox", { name: /link/ });
    fireEvent.change(box, { target: { value: "nonsense" } });
    fireEvent.click(screen.getByRole("button", { name: "Pair" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("That isn't a JuiceBox Trainer pairing link");

    fireEvent.change(screen.getByRole("textbox", { name: /phone's name/ }), { target: { value: "Pixel" } });
    fireEvent.change(box, { target: { value: "http://192.168.1.20:47110/pair?code=x&fp=y" } });
    fireEvent.click(screen.getByRole("button", { name: "Pair" }));
    expect(await screen.findByRole("heading", { name: "No game yet" })).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("pc_pair", { link: "http://192.168.1.20:47110/pair?code=x&fp=y", phone: "Pixel" });
    expect(await screen.findByText(/Connected to DESKTOP-1/)).toBeInTheDocument();
    expect(localStorage.getItem("juicebox.remote.phoneName")).toBe("Pixel");
  });

  it("shows the cheats by group and sends what the player does", async () => {
    pcs = [pc];
    localStorage.setItem("juicebox.remote.lastPc", "pc1");
    view = loaded;
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Testbed" })).toBeInTheDocument();
    const player = screen.getByRole("region", { name: "Player" });
    expect(within(player).getAllByText(/Infinite Health|Move Speed/)).toHaveLength(2);

    fireEvent.click(screen.getByRole("switch", { name: "Infinite Health" }));
    await vi.waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("pc_action", { id: "pc1", action: { id: "inf_health", op: "toggle", on: true } }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Add Gold: +100" }));
    await vi.waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("pc_action", { id: "pc1", action: { id: "add_gold", op: "press" } }),
    );
    fireEvent.change(screen.getByRole("spinbutton", { name: "Ammo: new value" }), { target: { value: "999" } });
    fireEvent.click(screen.getByRole("button", { name: "Set" }));
    await vi.waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("pc_action", { id: "pc1", action: { id: "ammo", op: "set", value: "999" } }),
    );
    // A cheat that doesn't work in this game version says why and can't be used.
    expect(screen.getByText("not found in this version")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Fly" })).toBeDisabled();

    await vi.waitFor(() => expect(screen.getByRole("button", { name: "Boss fight" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "Boss fight" }));
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("pc_preset", { id: "pc1", name: "Boss fight" }));
    expect(await screen.findByRole("status")).toHaveTextContent('Turned on "Boss fight"');

    // Groups fold away.
    fireEvent.click(screen.getByRole("button", { name: /Items/ }));
    expect(screen.queryByText("Add Gold")).not.toBeInTheDocument();
  });

  it("sends a slider's value once it stops moving", async () => {
    vi.useFakeTimers();
    try {
      pcs = [pc];
      localStorage.setItem("juicebox.remote.lastPc", "pc1");
      view = loaded;
      render(<App />);
      for (let i = 0; i < 20 && !screen.queryByRole("slider"); i++) {
        await act(async () => {
          await vi.advanceTimersByTimeAsync(20);
        });
      }
      const slider = screen.getByRole("slider", { name: "Move Speed" });
      fireEvent.change(slider, { target: { value: "2" } });
      fireEvent.change(slider, { target: { value: "3.5" } });
      expect(invoke).not.toHaveBeenCalledWith("pc_action", expect.anything());
      await act(async () => {
        await vi.advanceTimersByTimeAsync(400);
      });
      expect(invoke).toHaveBeenCalledWith("pc_action", { id: "pc1", action: { id: "speed", op: "set", value: "3.5" } });
      expect(invoke.mock.calls.filter(([c]) => c === "pc_action")).toHaveLength(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("says when the game closed and when the PC can't be reached", async () => {
    pcs = [pc];
    localStorage.setItem("juicebox.remote.lastPc", "pc1");
    view = { pack: null, closed: "Outbreak" };
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Outbreak closed" })).toBeInTheDocument();
    view = new Error("off");
    expect(await screen.findByText("Can't reach the PC", {}, { timeout: 3000 })).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("keeps trying by itself");
  });

  it("lists the PCs and forgets one", async () => {
    pcs = [pc, { id: "pc2", name: "LAPTOP", address: "192.168.1.30:47110" }];
    render(<App />);
    expect(await screen.findByRole("button", { name: /LAPTOP/ })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /DESKTOP-1/ }));
    expect(await screen.findByRole("heading", { name: "No game yet" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Forget this PC" }));
    fireEvent.click(screen.getByRole("button", { name: "Forget it" }));
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("pc_forget", { id: "pc1" }));
    expect(await screen.findByRole("heading", { name: "Pair with your PC" })).toBeInTheDocument();
  });
});
