import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Cheat, CheatState, PackSummary, SessionInfo, TrainerView } from "../lib/ipc";
import { TrainerPanel } from "./TrainerPanel";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

const session: SessionInfo = {
  process: {
    pid: 1234,
    parentPid: 1,
    name: "testbed_native.exe",
    exePath: "C:\\x\\testbed_native.exe",
    arch: "x64",
    threads: 1,
  },
  modules: [],
};

function summary(over: Partial<PackSummary>): PackSummary {
  return {
    key: "bundled:testbed-native",
    source: "bundled",
    path: null,
    id: "testbed-native",
    name: "OpenTrainer Testbed (x64)",
    exe: "testbed_native.exe",
    arch: "x64",
    author: "OpenTrainer",
    ai: false,
    packVersion: "1.0.0",
    cheats: 4,
    error: null,
    issues: [],
    ...over,
  };
}

const cheats: Cheat[] = [
  {
    id: "inf_health",
    name: "Infinite Health",
    category: "Player",
    ui: "toggle",
    hotkey: "F1",
    impl: { kind: "patch" },
  },
  {
    id: "add_gold",
    name: "+1000 Gold",
    category: "Inventory",
    ui: "button",
    hotkey: "F4",
    button: { add: 1000 },
    impl: { kind: "value" },
  },
  { id: "set_gold", name: "Set Gold", category: "Inventory", ui: "number", impl: { kind: "value" } },
  {
    id: "speed",
    name: "Move Speed",
    category: "Player",
    ui: "slider",
    slider: { min: 0.5, max: 5, step: 0.1, default: 1, hold: true },
    impl: { kind: "value" },
  },
  { id: "broken", name: "Broken", ui: "toggle", impl: { kind: "patch" } },
];

let states: Record<string, CheatState>;
let loaded: boolean;

function view(): TrainerView {
  return {
    key: "bundled:testbed-native",
    pack: {
      schema: 1,
      game: { id: "testbed-native", name: "OpenTrainer Testbed (x64)", exe: "testbed_native.exe", arch: "x64", builds: [] },
      meta: { author: "ai", packVersion: "1.0.0" },
      cheats,
    },
    build: { status: "unknown" },
    exeSha256: "ab".repeat(32),
    warnings: [{ severity: "warning", path: "game.builds", message: "no builds listed" }],
    cheats: cheats.map((c) => ({ ...states[c.id]! })),
    hotkeyErrors: { add_gold: "F4 is already used by another app" },
  };
}

function st(id: string, over: Partial<CheatState> = {}): CheatState {
  return {
    id,
    status: "ready",
    active: false,
    message: null,
    value: null,
    address: 0x1000,
    hotkey: null,
    touches: [`touches ${id}`],
    ...over,
  };
}

beforeEach(() => {
  loaded = false;
  states = {
    inf_health: st("inf_health", { hotkey: "F1", touches: ["testbed_native.exe+0x2224: 29 D0 → 90 90"] }),
    add_gold: st("add_gold", { hotkey: "F4", value: "250" }),
    set_gold: st("set_gold", { value: "250" }),
    speed: st("speed", { value: "1" }),
    broken: st("broken", { status: "failed", message: "needs repair: expected 29 D0", address: null }),
  };
  invoke.mockReset();
  invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "packs_list":
        return Promise.resolve([
          summary({}),
          summary({ key: "bundled:x86", name: "x86 pack", arch: "x86" }),
          summary({ key: "other", name: "Other game", exe: "other.exe" }),
        ]);
      case "packs_folder":
        return Promise.resolve("C:\\Users\\me\\AppData\\Roaming\\OpenTrainer\\packs");
      case "trainer_state":
        return Promise.resolve(loaded ? view() : null);
      case "trainer_load":
        loaded = true;
        return Promise.resolve(view());
      case "trainer_toggle":
        states[String(args?.id)]!.active = Boolean(args?.on);
        states[String(args?.id)]!.status = args?.on ? "active" : "ready";
        return Promise.resolve(view());
      case "trainer_press":
        states.add_gold!.value = "1250";
        return Promise.resolve(view());
      case "trainer_set":
        if (args?.value === "-1") {
          return Promise.reject({ kind: "outOfRange", message: "-1 is out of range for 'set_gold'", verdict: null });
        }
        states[String(args?.id)]!.value = String(args?.value);
        if (args?.id === "speed") states.speed!.active = true;
        return Promise.resolve(view());
      case "trainer_release":
        states[String(args?.id)]!.active = false;
        return Promise.resolve(view());
      case "trainer_unload":
        loaded = false;
        return Promise.resolve();
      default:
        return Promise.reject({ kind: "unknown", message: cmd, verdict: null });
    }
  });
});

describe("TrainerPanel", () => {
  it("lists only packs for this exe and arch, then loads one", async () => {
    render(<TrainerPanel session={session} onSessionLost={vi.fn()} />);
    const load = await screen.findByRole("button", { name: "Load OpenTrainer Testbed (x64)" });
    expect(screen.queryByText("x86 pack")).not.toBeInTheDocument();
    expect(screen.queryByText("Other game")).not.toBeInTheDocument();
    fireEvent.click(load);
    expect(await screen.findByRole("switch", { name: "Infinite Health" })).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("trainer_load", { key: "bundled:testbed-native" });
    expect(screen.getByText("AI-made")).toBeInTheDocument();
    expect(screen.getByText("Unknown build")).toBeInTheDocument();
    expect(screen.getByText(/not one the pack lists/)).toBeInTheDocument();
    // Grouped by category, in pack order.
    expect(screen.getAllByRole("heading", { level: 3 }).map((h) => h.textContent)).toEqual([
      "Player",
      "Inventory",
      "General",
    ]);
  });

  it("toggles, presses, sets, slides, releases and shows failures", async () => {
    loaded = true;
    render(<TrainerPanel session={session} onSessionLost={vi.fn()} />);
    const toggle = await screen.findByRole("switch", { name: "Infinite Health" });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText("testbed_native.exe+0x2224: 29 D0 → 90 90")).toBeInTheDocument();

    fireEvent.click(toggle);
    await vi.waitFor(() => expect(toggle).toHaveAttribute("aria-checked", "true"));
    expect(invoke).toHaveBeenCalledWith("trainer_toggle", { id: "inf_health", on: true });

    fireEvent.click(screen.getByRole("button", { name: "+1000 Gold" }));
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("trainer_press", { id: "add_gold" }));
    // The F4 conflict is shown on the row.
    const gold = screen.getByTestId("cheat-add_gold");
    expect(within(gold).getByText("F4 is already used by another app")).toBeInTheDocument();

    const input = screen.getByLabelText("Set Gold value");
    fireEvent.change(input, { target: { value: "-1" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(await screen.findByText("-1 is out of range for 'set_gold'")).toBeInTheDocument();
    fireEvent.change(input, { target: { value: "777" } });
    fireEvent.click(screen.getByRole("button", { name: "Set" }));
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("trainer_set", { id: "set_gold", value: "777" }));

    fireEvent.change(screen.getByRole("slider", { name: "Move Speed" }), { target: { value: "2.5" } });
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("trainer_set", { id: "speed", value: "2.5" }));
    const speed = screen.getByTestId("cheat-speed");
    await within(speed).findByText("held");
    fireEvent.click(within(speed).getByRole("button", { name: "Release" }));
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("trainer_release", { id: "speed" }));

    const broken = screen.getByTestId("cheat-broken");
    expect(within(broken).getByRole("switch")).toBeDisabled();
    expect(within(broken).getByRole("alert")).toHaveTextContent("needs repair");

    fireEvent.click(screen.getByRole("button", { name: "Unload pack" }));
    expect(await screen.findByRole("button", { name: "Load OpenTrainer Testbed (x64)" })).toBeInTheDocument();
  });

  it("explains where packs go when none match", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "packs_list") return Promise.resolve([summary({ exe: "other.exe" })]);
      if (cmd === "packs_folder") return Promise.resolve("C:\\packs");
      if (cmd === "trainer_state") return Promise.resolve(null);
      return Promise.resolve();
    });
    render(<TrainerPanel session={session} onSessionLost={vi.fn()} />);
    expect(await screen.findByText(/No pack for this game yet/)).toBeInTheDocument();
    expect(await screen.findByText(/C:\\packs/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Open packs folder" }));
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("open_packs_folder", undefined));
  });

  it("reports a lost session from polling", async () => {
    const lost = vi.fn();
    invoke.mockImplementation((cmd: string) =>
      cmd === "trainer_state"
        ? Promise.reject({ kind: "processExited", message: "gone", verdict: null })
        : Promise.resolve([]),
    );
    render(<TrainerPanel session={session} onSessionLost={lost} />);
    await vi.waitFor(() => expect(lost).toHaveBeenCalled());
  });
});
