import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AgreementGate } from "./AgreementGate";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));

const unsigned = { accepted: false, version: 1, signed: null };
const signed = { accepted: true, version: 1, signed: { version: 1, name: "Sam", date: "2026-11-09" } };

beforeEach(() => invoke.mockReset());

describe("AgreementGate", () => {
  it("shows the agreement until the player signs with their name", async () => {
    invoke.mockImplementation((cmd: string) => Promise.resolve(cmd === "agreement_sign" ? signed : unsigned));
    render(
      <AgreementGate>
        <p>The app</p>
      </AgreementGate>,
    );
    expect(await screen.findByRole("dialog", { name: "Before you use JuiceBox Trainer" })).toBeInTheDocument();
    expect(screen.getByText(/Never use cheats in games with strangers/)).toBeInTheDocument();
    expect(screen.getByText(/not responsible for anything that happens/)).toBeInTheDocument();
    expect(screen.queryByText("The app")).not.toBeInTheDocument();

    const sign = screen.getByRole("button", { name: "Sign and continue" });
    expect(sign).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Your name"), { target: { value: "Sam" } });
    expect(sign).toBeDisabled(); // must tick the box too
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(sign);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("agreement_sign", { name: "Sam" }));
    expect(await screen.findByText("The app")).toBeInTheDocument();
  });

  it("shows the app straight away once signed", async () => {
    invoke.mockResolvedValue(signed);
    render(
      <AgreementGate>
        <p>The app</p>
      </AgreementGate>,
    );
    expect(await screen.findByText("The app")).toBeInTheDocument();
  });

  it("asks again after the agreement changed", async () => {
    invoke.mockResolvedValue({ accepted: false, version: 2, signed: { version: 1, name: "Sam", date: "2026-11-09" } });
    render(
      <AgreementGate>
        <p>The app</p>
      </AgreementGate>,
    );
    expect(await screen.findByText(/The agreement changed since you signed it/)).toBeInTheDocument();
  });
});
