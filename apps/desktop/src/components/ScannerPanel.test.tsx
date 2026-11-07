import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ScannerPanel } from "./ScannerPanel";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));

let round = 0;
beforeEach(() => {
  round = 0;
  invoke.mockReset();
  invoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "scan_first":
        round = 1;
        return Promise.resolve({ count: 3, ty: "i32", rounds: 1, memoryBytes: 24 });
      case "scan_next":
        round = 2;
        return Promise.resolve({ count: 1, ty: "i32", rounds: 2, memoryBytes: 8 });
      case "scan_results":
        return Promise.resolve(
          round === 1
            ? {
                count: 3,
                hits: [
                  { address: 0x1000, previous: "250", current: "250" },
                  { address: 0x2000, previous: "250", current: "267" },
                  { address: 0x3000, previous: "250", current: null },
                ],
              }
            : { count: 1, hits: [{ address: 0x2000, previous: "267", current: "267" }] },
        );
      case "scan_progress":
        return Promise.resolve({ running: true, done: 1, total: 2 });
      case "scan_reset":
        return Promise.resolve();
      default:
        return Promise.reject({ kind: "unknown", message: cmd, verdict: null });
    }
  });
});

describe("ScannerPanel", () => {
  it("runs a first exact scan, then narrows with a next scan", async () => {
    const onAdd = vi.fn();
    render(<ScannerPanel onAddToList={onAdd} onSessionLost={vi.fn()} />);

    const first = screen.getByRole("button", { name: "First scan" });
    expect(first).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Scan value"), { target: { value: "250" } });
    fireEvent.click(first);

    expect(await screen.findByTestId("scan-count")).toHaveTextContent("3 results");
    expect(invoke).toHaveBeenCalledWith("scan_first", {
      ty: "i32",
      mode: { kind: "exact", value: "250" },
      options: { writableOnly: true, aligned: true, floatTolerance: null },
    });
    const table = screen.getByRole("table", { name: "Scan results" });
    expect(within(table).getByText("0x2000")).toBeInTheDocument();
    expect(within(table).getByText("??")).toBeInTheDocument();
    // Type is fixed once a scan exists.
    expect(screen.getByLabelText("Scan value type")).toBeDisabled();

    fireEvent.change(screen.getByLabelText("Scan kind"), { target: { value: "increased" } });
    expect(screen.queryByLabelText("Scan value")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Next scan" }));
    expect(await screen.findByText("1 result")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("scan_next", { mode: { kind: "increased" } });

    fireEvent.click(screen.getByRole("button", { name: "Add 0x2000 to address list" }));
    expect(onAdd).toHaveBeenCalledWith(0x2000, "i32");

    fireEvent.click(screen.getByRole("button", { name: "New scan" }));
    expect(await screen.findByRole("button", { name: "First scan" })).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("scan_reset", undefined);
  });

  it("sends a result to the pointer scanner", async () => {
    const find = vi.fn();
    render(<ScannerPanel onAddToList={vi.fn()} onFindPointers={find} onSessionLost={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("Scan value"), { target: { value: "250" } });
    fireEvent.click(screen.getByRole("button", { name: "First scan" }));
    fireEvent.click(await screen.findByRole("button", { name: "Find pointers to 0x2000" }));
    expect(find).toHaveBeenCalledWith(0x2000, "i32");
  });

  it("sends a range scan and float tolerance", async () => {
    render(<ScannerPanel onAddToList={vi.fn()} onSessionLost={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("Scan value type"), { target: { value: "f32" } });
    fireEvent.change(screen.getByLabelText("Scan kind"), { target: { value: "between" } });
    fireEvent.change(screen.getByLabelText("Minimum"), { target: { value: "0.5" } });
    fireEvent.change(screen.getByLabelText("Maximum"), { target: { value: "1.5" } });
    fireEvent.change(screen.getByLabelText("Float tolerance"), { target: { value: "0.01" } });
    fireEvent.click(screen.getByRole("button", { name: "First scan" }));
    await screen.findByTestId("scan-count");
    expect(invoke).toHaveBeenCalledWith("scan_first", {
      ty: "f32",
      mode: { kind: "between", min: "0.5", max: "1.5" },
      options: { writableOnly: true, aligned: true, floatTolerance: 0.01 },
    });
  });

  it("hands guard blocks to the session handler", async () => {
    const lost = vi.fn();
    invoke.mockImplementationOnce(() =>
      Promise.reject({ kind: "blocked", message: "blocked", verdict: null }),
    );
    render(<ScannerPanel onAddToList={vi.fn()} onSessionLost={lost} />);
    fireEvent.change(screen.getByLabelText("Scan kind"), { target: { value: "unknown" } });
    fireEvent.click(screen.getByRole("button", { name: "First scan" }));
    await vi.waitFor(() => expect(lost).toHaveBeenCalled());
    expect(lost.mock.calls[0]![0].kind).toBe("blocked");
  });
});
