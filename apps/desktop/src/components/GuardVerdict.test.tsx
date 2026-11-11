import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { Verdict } from "../lib/ipc";
import { GuardVerdict } from "./GuardVerdict";

const base: Verdict = { pid: 7, target: "game.exe", allowed: true, findings: [], signaturesVersion: 1 };
const files = { kind: "installMarker", product: "Easy Anti-Cheat", detail: "EasyAntiCheat" } as const;

describe("GuardVerdict", () => {
  it("says anti-cheat that comes with the game isn't running", () => {
    render(<GuardVerdict verdict={{ ...base, notes: [files] }} />);
    expect(screen.getByRole("status")).toHaveTextContent("No anti-cheat running. You can attach.");
    expect(screen.getByRole("status")).toHaveTextContent(/Easy Anti-Cheat comes with this game but isn't running/);
  });

  it("explains a running anti-cheat that came with the game", () => {
    render(
      <GuardVerdict
        verdict={{
          ...base,
          allowed: false,
          findings: [{ kind: "service", product: "Easy Anti-Cheat", detail: "EasyAntiCheat_EOS" }, files],
        }}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent(/anti-cheat is running\. JuiceBox Trainer only works while it isn't/);
    expect(screen.getByRole("alert")).toHaveTextContent(/can't be turned off/);
  });

  it("has no such hint for online-only games", () => {
    render(
      <GuardVerdict
        verdict={{ ...base, allowed: false, findings: [{ kind: "onlineOnly", product: "Counter-Strike 2", detail: "cs2.exe" }] }}
      />,
    );
    expect(screen.getByRole("alert")).not.toHaveTextContent(/only works while it isn't/);
  });
});
