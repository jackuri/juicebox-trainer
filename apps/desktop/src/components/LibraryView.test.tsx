import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LibraryGame } from "../lib/ipc";
import { coverBytes } from "../lib/format";
import { LibraryView } from "./LibraryView";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

function game(over: Partial<LibraryGame>): LibraryGame {
  return {
    id: "steam:1",
    name: "Testbed",
    store: "steam",
    folder: "C:\\Games\\Testbed",
    exe: null,
    launch: { kind: "uri", target: "steam://rungameid/1" },
    pack: null,
    presets: [],
    autoPreset: null,
    blocked: null,
    antiCheat: null,
    hidden: false,
    cover: null,
    ...over,
  };
}

const session = {
  process: { pid: 7, parentPid: 1, name: "testbed_native.exe", exePath: null, arch: "x64", threads: 1 },
  modules: [],
};

beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "library_list":
        return Promise.resolve([
          game({
            id: "own:c:\\t.exe",
            name: "JuiceBox Trainer Testbed",
            store: "own",
            pack: { key: "bundled:testbed-native", name: "JuiceBox Trainer Testbed (x64)", ai: false, exe: "testbed_native.exe" },
            autoPreset: "Story mode",
            presets: ["Story mode"],
          }),
          game({ id: "steam:2", name: "Online Shooter", blocked: "anti-cheat found: Counter-Strike 2" }),
          game({ id: "steam:3", name: "Collection", antiCheat: "Easy Anti-Cheat" }),
          game({ id: "xbox:x", name: "Xbox Game", store: "xbox", launch: { kind: "manual" } }),
        ]);
      case "library_play":
        return Promise.resolve({ session, trainer: null, preset: "Story mode" });
      case "library_add":
        return Promise.resolve();
      default:
        return Promise.resolve();
    }
  });
});

describe("LibraryView", () => {
  it("lists games with their pack, preset and anti-cheat status", async () => {
    render(<LibraryView onPlayed={vi.fn()} onOpenAi={vi.fn()} onOpenTable={vi.fn()} />);
    const tb = await screen.findByTestId("game-own:c:\\t.exe");
    expect(within(tb).getByText(/Pack: JuiceBox Trainer Testbed \(x64\)/)).toBeInTheDocument();
    expect(within(tb).getByText("Starts with: Story mode")).toBeInTheDocument();
    expect(within(tb).getByRole("button", { name: "Play JuiceBox Trainer Testbed" })).toHaveTextContent("Play with cheats");

    const online = screen.getByTestId("game-steam:2");
    expect(within(online).getByText(/Counter-Strike 2\): not supported/)).toBeInTheDocument();
    expect(within(online).getByRole("button", { name: "Play Online Shooter" })).toBeDisabled();

    // Anti-cheat that only comes with the game: playable, with a note.
    const ac = screen.getByTestId("game-steam:3");
    expect(within(ac).getByText("Has Easy Anti-Cheat: works only when the game runs without it")).toBeInTheDocument();
    expect(within(ac).getByRole("button", { name: "Play Collection" })).toBeEnabled();

    const xbox = screen.getByTestId("game-xbox:x");
    expect(within(xbox).getByText(/You start it yourself/)).toBeInTheDocument();
    expect(within(xbox).getByRole("button", { name: "Play Xbox Game" })).toHaveTextContent("Play and attach");

    fireEvent.change(screen.getByLabelText("Find a game"), { target: { value: "xbox" } });
    expect(screen.queryByTestId("game-steam:2")).not.toBeInTheDocument();
  });

  it("plays a game and hands over the session", async () => {
    const played = vi.fn();
    render(<LibraryView onPlayed={played} onOpenAi={vi.fn()} onOpenTable={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Play JuiceBox Trainer Testbed" }));
    await vi.waitFor(() => expect(played).toHaveBeenCalledWith({ session, trainer: null, preset: "Story mode" }));
    expect(invoke).toHaveBeenCalledWith("library_play", { id: "own:c:\\t.exe" });
  });

  it("shows why a game couldn't start and adds games by exe", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "library_list") return Promise.resolve([game({})]);
      if (cmd === "library_play") return Promise.reject({ kind: "timeout", message: "Testbed didn't start.", verdict: null });
      if (cmd === "library_add") return Promise.reject({ kind: "io", message: "C:\\x.txt is not an .exe file on this PC", verdict: null });
      return Promise.resolve();
    });
    const ai = vi.fn();
    render(<LibraryView onPlayed={vi.fn()} onOpenAi={ai} onOpenTable={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Play Testbed" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Testbed didn't start.");
    fireEvent.click(screen.getByRole("button", { name: "Make one with AI" }));
    expect(ai).toHaveBeenCalled();
    fireEvent.change(screen.getByLabelText("Game exe path"), { target: { value: "C:\\x.txt" } });
    fireEvent.click(screen.getByRole("button", { name: "Add game" }));
    expect(await screen.findByText(/is not an \.exe file/)).toBeInTheDocument();
  });

  it("shows store artwork, icons, and initials when there is no picture", async () => {
    const created = vi.fn(() => "blob:cover-1");
    Object.defineProperty(URL, "createObjectURL", { value: created, configurable: true });
    invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "library_list")
        return Promise.resolve([
          game({ id: "steam:art", name: "Art Game", cover: "art" }),
          game({ id: "gog:icon", name: "Icon Game", store: "gog", cover: "icon" }),
          game({ id: "epic:none", name: "No Picture", store: "epic", cover: null }),
          game({ id: "steam:empty", name: "Empty Answer", cover: "art" }),
        ]);
      if (cmd === "library_cover")
        // The real app hands bytes over as a plain number list; tests also cover an ArrayBuffer.
        return Promise.resolve(
          args?.id === "steam:empty" ? [] : args?.id === "gog:icon" ? new Uint8Array([4, 5]).buffer : [255, 216, 255],
        );
      return Promise.resolve();
    });
    render(<LibraryView onPlayed={vi.fn()} onOpenAi={vi.fn()} onOpenTable={vi.fn()} />);
    const art = await screen.findByTestId("game-steam:art");
    await vi.waitFor(() => expect(art.querySelector("img")).toHaveAttribute("src", "blob:cover-1"));
    expect(art.querySelector("img")).toHaveClass("object-cover");
    const icon = screen.getByTestId("game-gog:icon");
    await vi.waitFor(() => expect(icon.querySelector("img")).toHaveClass("object-contain"));
    // No picture on this PC: initials, and nothing is asked for.
    expect(within(screen.getByTestId("game-epic:none")).getByText("NP")).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("library_cover", { id: "epic:none" });
    // An empty answer also falls back to initials.
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("library_cover", { id: "steam:empty" }));
    expect(within(screen.getByTestId("game-steam:empty")).getByText("EA")).toBeInTheDocument();
    expect(created).toHaveBeenCalledTimes(2);
  });

  it("reads cover bytes in every form the app bridge uses", () => {
    expect(coverBytes([1, 2, 3])).toEqual(new Uint8Array([1, 2, 3]));
    expect(coverBytes(new Uint8Array([1, 2]).buffer)).toEqual(new Uint8Array([1, 2]));
    expect(coverBytes(new Uint8Array([9, 1, 2, 9]).subarray(1, 3))).toEqual(new Uint8Array([1, 2]));
    expect(coverBytes([])).toBeNull();
    expect(coverBytes(new ArrayBuffer(0))).toBeNull();
    expect(coverBytes(null)).toBeNull();
    expect(coverBytes(["x"])).toBeNull();
  });

  it("adds a game picked with Browse, and does nothing when cancelled", async () => {
    let picked: string | null = null;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "library_list") return Promise.resolve([game({})]);
      if (cmd === "pick_exe") return Promise.resolve(picked);
      return Promise.resolve();
    });
    render(<LibraryView onPlayed={vi.fn()} onOpenAi={vi.fn()} onOpenTable={vi.fn()} />);
    const browse = await screen.findByRole("button", { name: "Browse for its .exe…" });
    fireEvent.click(browse);
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("pick_exe", { titleText: "Pick the game's .exe", start: null }));
    expect(invoke).not.toHaveBeenCalledWith("library_add", expect.anything());

    picked = "C:\\Games\\Mine\\mine.exe";
    fireEvent.click(browse);
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("library_add", { exe: "C:\\Games\\Mine\\mine.exe" }));
  });

  it("hides games and shows them again", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "library_list")
        return Promise.resolve([game({ id: "steam:1", name: "Shown Game" }), game({ id: "steam:2", name: "Secret Game", hidden: true })]);
      return Promise.resolve();
    });
    render(<LibraryView onPlayed={vi.fn()} onOpenAi={vi.fn()} onOpenTable={vi.fn()} />);
    await screen.findByTestId("game-steam:1");
    expect(screen.queryByTestId("game-steam:2")).not.toBeInTheDocument();

    // Hide the shown one: it leaves the list and the switch counts it.
    fireEvent.click(screen.getByRole("button", { name: "Hide Shown Game" }));
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("library_set_hidden", { id: "steam:1", hidden: true }));
    await vi.waitFor(() => expect(screen.queryByTestId("game-steam:1")).not.toBeInTheDocument());
    expect(screen.getByText(/All your games are hidden/)).toBeInTheDocument();

    const toggle = screen.getByLabelText("Show hidden (2)");
    fireEvent.click(toggle);
    const secret = screen.getByTestId("game-steam:2");
    expect(within(secret).getByText("Hidden")).toBeInTheDocument();
    fireEvent.click(within(secret).getByRole("button", { name: "Unhide Secret Game" }));
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("library_set_hidden", { id: "steam:2", hidden: false }));
    await vi.waitFor(() => expect(screen.getByLabelText("Show hidden (1)")).toBeInTheDocument());
  });
});
