import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ListEntry } from "../lib/ipc";
import { AddressList } from "./AddressList";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));

let entries: ListEntry[] = [];

beforeEach(() => {
  entries = [
    {
      id: 1,
      label: "Gold",
      address: 0x1f0a10,
      addressExpr: "0x1F0A10",
      ty: "i32",
      value: "250",
      error: null,
      frozen: false,
    },
  ];
  invoke.mockReset();
  invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "address_list":
        return Promise.resolve(entries.map((e) => ({ ...e })));
      case "address_set_frozen":
        entries[0]!.frozen = Boolean(args?.frozen);
        return Promise.resolve(entries[0]);
      case "address_write":
        entries[0]!.value = String(args?.value);
        return Promise.resolve(entries[0]);
      case "address_add":
        entries.push({
          id: 2,
          label: String(args?.label),
          address: 0x2000,
          addressExpr: String(args?.address),
          ty: args?.ty as ListEntry["ty"],
          value: "1",
          error: null,
          frozen: false,
        });
        return Promise.resolve(entries[1]);
      case "address_remove":
        entries = entries.filter((e) => e.id !== args?.id);
        return Promise.resolve();
      default:
        return Promise.reject({ kind: "unknown", message: cmd, verdict: null });
    }
  });
});

function Harness({ onLost = vi.fn() }: { onLost?: () => void }) {
  const [list, setList] = useState<ListEntry[]>([]);
  return <AddressList entries={list} onChange={setList} onSessionLost={onLost} />;
}

describe("AddressList", () => {
  it("shows live values, freezes, writes, adds and removes", async () => {
    render(<Harness />);
    const value = await screen.findByLabelText("Value of Gold");
    expect(value).toHaveValue("250");

    fireEvent.click(screen.getByLabelText("Freeze Gold"));
    await vi.waitFor(() => expect(screen.getByLabelText("Freeze Gold")).toBeChecked());
    expect(invoke).toHaveBeenCalledWith("address_set_frozen", { id: 1, frozen: true });

    fireEvent.focus(value);
    fireEvent.change(value, { target: { value: "9999" } });
    fireEvent.keyDown(value, { key: "Enter" });
    await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("address_write", { id: 1, value: "9999" }));
    fireEvent.blur(value);
    await vi.waitFor(() => expect(screen.getByLabelText("Value of Gold")).toHaveValue("9999"));

    fireEvent.change(screen.getByLabelText("New address"), { target: { value: "game.exe+0x10" } });
    fireEvent.change(screen.getByLabelText("New address type"), { target: { value: "f32" } });
    fireEvent.change(screen.getByLabelText("New address label"), { target: { value: "Speed" } });
    fireEvent.click(screen.getByRole("button", { name: "Add address" }));
    expect(await screen.findByLabelText("Value of Speed")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("address_add", {
      address: "game.exe+0x10",
      ty: "f32",
      label: "Speed",
    });

    fireEvent.click(screen.getByLabelText("Remove Speed"));
    await vi.waitFor(() => expect(screen.queryByLabelText("Value of Speed")).not.toBeInTheDocument());
  });

  it("reports a lost session from polling", async () => {
    const lost = vi.fn();
    invoke.mockImplementation(() =>
      Promise.reject({ kind: "processExited", message: "process 1 has exited", verdict: null }),
    );
    render(<Harness onLost={lost} />);
    await vi.waitFor(() => expect(lost).toHaveBeenCalled());
  });
});
