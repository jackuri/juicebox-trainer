import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ProcessInfo, RunningGame } from "../lib/ipc";
import { ProcessList } from "./ProcessList";
import { GamePicker } from "./GamePicker";

function proc(pid: number, name: string, exePath: string | null): ProcessInfo {
  return { pid, parentPid: 1, name, exePath, arch: "x64", threads: 1 };
}

const processes = [
  proc(10, "eldenring.exe", "D:\\Steam\\steamapps\\common\\ELDEN RING\\Game\\eldenring.exe"),
  proc(20, "ArmouryCrate.exe", "C:\\Program Files\\ASUS\\ArmouryCrate.exe"),
  proc(30, "svchost.exe", "C:\\Windows\\System32\\svchost.exe"),
];
const games: RunningGame[] = [{ pid: 10, name: "ELDEN RING", libraryId: "steam:1245620" }];

function renderList(over: Partial<Parameters<typeof ProcessList>[0]> = {}) {
  const props = {
    processes,
    games,
    selectedPid: null,
    loading: false,
    onSelect: vi.fn(),
    onRefresh: vi.fn(),
    onShowGames: vi.fn(),
    ...over,
  };
  render(<ProcessList {...props} />);
  return props;
}

describe("ProcessList", () => {
  it("shows running games by name and hides other programs", () => {
    const props = renderList();
    const row = screen.getByRole("option", { name: /ELDEN RING/ });
    expect(row).toHaveTextContent("eldenring.exe");
    expect(screen.queryByRole("option", { name: /ArmouryCrate/ })).not.toBeInTheDocument();
    fireEvent.click(row);
    expect(props.onSelect).toHaveBeenCalledWith(processes[0]);
  });

  it("lists other programs, never Windows ones, when asked", () => {
    renderList();
    fireEvent.click(screen.getByLabelText("Show all running programs"));
    expect(screen.getByRole("option", { name: /ArmouryCrate/ })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /svchost/ })).not.toBeInTheDocument();
  });

  it("searches games by name", () => {
    renderList();
    fireEvent.change(screen.getByLabelText("Search processes"), { target: { value: "elden" } });
    expect(screen.getByRole("option", { name: /ELDEN RING/ })).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Search processes"), { target: { value: "zzz" } });
    expect(screen.getByText("No running game matches.")).toBeInTheDocument();
  });

  it("says how to start a game when none is running", () => {
    const props = renderList({ games: [] });
    expect(screen.getByText(/None of your games is running/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Games" }));
    expect(props.onShowGames).toHaveBeenCalled();
  });

  it("keeps a chosen other program visible", () => {
    renderList({ selectedPid: 20 });
    expect(screen.getByRole("option", { name: /ArmouryCrate/ })).toHaveAttribute("aria-selected", "true");
  });
});

describe("GamePicker", () => {
  function renderPicker(g: RunningGame[], value = "") {
    const onChange = vi.fn();
    render(
      <GamePicker
        label="Game"
        processes={processes}
        games={g}
        value={value}
        onChange={onChange}
        onRefresh={vi.fn()}
      />,
    );
    return onChange;
  }
  const options = () =>
    Array.from((screen.getByLabelText("Game") as HTMLSelectElement).options).map((o) => o.textContent);

  it("offers running games by name, other programs only when asked", () => {
    const onChange = renderPicker(games);
    expect(options()).toEqual(["Pick your running game…", "ELDEN RING (eldenring.exe, 10)"]);
    fireEvent.change(screen.getByLabelText("Game"), { target: { value: "10" } });
    expect(onChange).toHaveBeenCalledWith("10");
    fireEvent.click(screen.getByLabelText("Show all running programs"));
    expect(options()).toEqual([
      "Pick your running game…",
      "ELDEN RING (eldenring.exe, 10)",
      "ArmouryCrate.exe (20)",
    ]);
  });

  it("says to start a game when none is running", () => {
    renderPicker([]);
    expect(options()).toEqual(["No game running: start one first"]);
    expect(screen.getByText(/Start the game/)).toBeInTheDocument();
  });

  it("keeps a chosen other program listed", () => {
    renderPicker(games, "20");
    expect(options()).toContain("ArmouryCrate.exe (20)");
  });
});
