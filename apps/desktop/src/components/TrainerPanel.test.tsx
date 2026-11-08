import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Cheat, CheatState, GameSettings, PackSummary, SessionInfo, TrainerView } from "../lib/ipc";
import { chosenPacks } from "../lib/trainer";
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
let settings: GameSettings;
let pending: string[];
let overlayOn: boolean;

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
    pads: settings.pads,
    settings,
    pending,
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
  chosenPacks.clear();
  loaded = false;
  settings = { presets: [], favourites: [], notes: {}, keys: {}, pads: {} };
  pending = [];
  overlayOn = false;
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
          summary({ key: "user:copy", source: "user", name: "My copy" }),
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
      case "preset_save":
        settings.presets = [
          ...settings.presets.filter((p) => p.name !== args?.name),
          { name: String(args?.name), cheats: [{ id: "inf_health" }] },
        ];
        return Promise.resolve(view());
      case "preset_apply":
        states.inf_health!.active = true;
        pending = ["speed"];
        return Promise.resolve(view());
      case "preset_auto":
        settings.autoPreset = (args?.name as string | null) ?? undefined;
        return Promise.resolve(view());
      case "preset_delete":
        settings.presets = settings.presets.filter((p) => p.name !== args?.name);
        return Promise.resolve(view());
      case "cheat_favourite":
        settings.favourites = args?.on ? [String(args.id)] : [];
        return Promise.resolve(view());
      case "cheat_note":
        settings.notes = { [String(args?.id)]: String(args?.note) };
        return Promise.resolve(view());
      case "cheat_key":
        if (args?.key === null) delete settings.keys[String(args?.id)];
        else settings.keys[String(args?.id)] = String(args?.key);
        states[String(args?.id)]!.hotkey = (args?.key as string | null) || null;
        return Promise.resolve(view());
      case "cheat_pad":
        settings.pads = { [String(args?.id)]: String(args?.chord) };
        return Promise.resolve(view());
      case "overlay_visible":
        return Promise.resolve(overlayOn);
      case "overlay_set":
        overlayOn = Boolean(args?.show);
        return Promise.resolve(overlayOn);
      case "remote_status":
        return Promise.resolve(null);
      case "trainer_repair":
        states.broken = st("broken");
        return Promise.resolve({
          fixed: [{ cheat: "broken", oldAob: "AA", newAob: "BB", at: "testbed_native.exe+0x10", matched: 10, of: 11 }],
          broken: [],
          saved: "C:/Users/me/packs/testbed-native/pack.json",
          view: view(),
        });
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
      "Phone remote",
    ]);
  });

  it("loads the only pack for the game by itself, and not again after Unload", async () => {
    const base = invoke.getMockImplementation()!;
    invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) =>
      cmd === "packs_list" ? Promise.resolve([summary({}), summary({ key: "other", exe: "other.exe" })]) : base(cmd, args),
    );
    const { unmount } = render(<TrainerPanel session={session} onSessionLost={vi.fn()} onShowPacks={vi.fn()} />);
    expect(await screen.findByRole("switch", { name: "Infinite Health" })).toBeInTheDocument();
    expect(screen.getByText(/Loaded automatically/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("trainer_load", { key: "bundled:testbed-native" });

    fireEvent.click(screen.getByRole("button", { name: "Unload pack" }));
    expect(await screen.findByRole("button", { name: "Load OpenTrainer Testbed (x64)" })).toBeInTheDocument();
    await new Promise((r) => setTimeout(r, 50));
    expect(invoke.mock.calls.filter(([c]) => c === "trainer_load")).toHaveLength(1);
    expect(screen.queryByRole("switch", { name: "Infinite Health" })).not.toBeInTheDocument();

    // Going to another tab and back (the panel is rebuilt) still remembers the choice.
    unmount();
    render(<TrainerPanel session={session} onSessionLost={vi.fn()} onShowPacks={vi.fn()} />);
    expect(await screen.findByRole("button", { name: "Load OpenTrainer Testbed (x64)" })).toBeInTheDocument();
    await new Promise((r) => setTimeout(r, 50));
    expect(invoke.mock.calls.filter(([c]) => c === "trainer_load")).toHaveLength(1);
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

  it("repairs cheats that stopped matching after an update", async () => {
    loaded = true;
    render(<TrainerPanel session={session} onSessionLost={vi.fn()} />);
    expect(await screen.findByText(/don't match this version of the game/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Try to repair" }));
    expect(await screen.findByText(/Repaired: Broken\. Saved as your copy/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("trainer_repair", undefined);
    expect(screen.queryByRole("button", { name: "Try to repair" })).not.toBeInTheDocument();
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

  it("sends the player to Packs when there is no pack", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "packs_list") return Promise.resolve([]);
      if (cmd === "trainer_state") return Promise.resolve(null);
      return Promise.resolve();
    });
    const show = vi.fn();
    render(<TrainerPanel session={session} onSessionLost={vi.fn()} onShowPacks={show} />);
    fireEvent.click(await screen.findByRole("button", { name: "Get a pack…" }));
    expect(show).toHaveBeenCalled();
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

describe("TrainerPanel extras", () => {
  it("saves, applies and auto-starts presets, and says what's waiting", async () => {
    loaded = true;
    render(<TrainerPanel session={session} onSessionLost={vi.fn()} />);
    expect(await screen.findByText(/save them as a preset/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("New preset name"), { target: { value: "Story mode" } });
    fireEvent.click(screen.getByRole("button", { name: "Save what's on now" }));
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("preset_save", { name: "Story mode" }));
    expect(await screen.findByLabelText("Preset")).toHaveValue("Story mode");
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("preset_apply", { name: "Story mode" }));
    expect(await screen.findByText(/Waiting for the game: Move Speed/)).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("Use when I start the game from Games"));
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("preset_auto", { name: "Story mode" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete preset Story mode" }));
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("preset_delete", { name: "Story mode" }));
  });

  it("puts favourites first and keeps notes", async () => {
    loaded = true;
    render(<TrainerPanel session={session} onSessionLost={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Favourite +1000 Gold" }));
    await vi.waitFor(() =>
      expect(screen.getAllByRole("heading", { level: 3 })[0]).toHaveTextContent("Favourites"),
    );
    expect(screen.getByRole("button", { name: "Favourite +1000 Gold" })).toHaveAttribute("aria-pressed", "true");
    const note = screen.getByLabelText("Note for Infinite Health");
    fireEvent.change(note, { target: { value: "off before the boss" } });
    fireEvent.blur(note);
    await vi.waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("cheat_note", { id: "inf_health", note: "off before the boss" }),
    );
    expect(await screen.findByText("off before the boss", { selector: "p" })).toBeInTheDocument();
  });

  it("sets own keys and controller combos", async () => {
    loaded = true;
    render(<TrainerPanel session={session} onSessionLost={vi.fn()} />);
    const key = await screen.findByLabelText("Key for Infinite Health");
    fireEvent.focus(key);
    expect(key).toHaveValue("Press a key…");
    fireEvent.keyDown(key, { key: "F5", code: "F5", ctrlKey: true });
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("cheat_key", { id: "inf_health", key: "Ctrl+F5" }));
    fireEvent.click(await screen.findByRole("button", { name: "Use the pack's key" }));
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("cheat_key", { id: "inf_health", key: null }));
    const pad = screen.getByLabelText("Controller combo for Infinite Health");
    fireEvent.change(pad, { target: { value: "LB+RB+DPadUp" } });
    fireEvent.keyDown(pad, { key: "Enter" });
    await vi.waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("cheat_pad", { id: "inf_health", chord: "LB+RB+DPadUp" }),
    );
    expect(await screen.findByTitle("Controller: LB+RB+DPadUp")).toBeInTheDocument();
  });

  it("shows and hides the overlay", async () => {
    loaded = true;
    render(<TrainerPanel session={session} onSessionLost={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Show overlay" }));
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("overlay_set", { show: true }));
    fireEvent.click(await screen.findByRole("button", { name: "Hide overlay" }));
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("overlay_set", { show: false }));
  });
});
