import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SignaturePanel } from "./SignaturePanel";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args),
}));

beforeEach(() => {
  invoke.mockReset();
});

describe("SignaturePanel", () => {
  it("makes a signature and copies it", async () => {
    invoke.mockResolvedValue({
      pattern: "8B 01 29 D0 89 01 48 83 C4 ??",
      module: "testbed_native.exe",
      moduleOffset: 0x2224,
      len: 10,
      instructions: [
        { offset: 0, len: 2, text: "mov eax,[rcx]" },
        { offset: 2, len: 2, text: "sub eax,edx" },
      ],
    });
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });

    render(<SignaturePanel onSessionLost={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("Code address"), { target: { value: "testbed_native.exe+0x2224" } });
    fireEvent.click(screen.getByRole("button", { name: "Make signature" }));
    expect(await screen.findByTestId("signature")).toHaveTextContent("8B 01 29 D0 89 01 48 83 C4 ??");
    expect(invoke).toHaveBeenCalledWith("signature_make", { address: "testbed_native.exe+0x2224" });
    expect(screen.getByText(/10 bytes, unique in testbed_native.exe/)).toHaveTextContent(
      "starts at testbed_native.exe+0x2224",
    );
    expect(screen.getByText("sub eax,edx", { exact: false })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    expect(await screen.findByRole("button", { name: "Copied" })).toBeInTheDocument();
    expect(writeText).toHaveBeenCalledWith("8B 01 29 D0 89 01 48 83 C4 ??");
  });

  it("explains failures", async () => {
    invoke.mockRejectedValue({
      kind: "signatureNotUnique",
      message: "no unique signature found at 0x10 within 64 bytes; try a nearby address",
      verdict: null,
    });
    render(<SignaturePanel onSessionLost={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("Code address"), { target: { value: "0x10" } });
    fireEvent.keyDown(screen.getByLabelText("Code address"), { key: "Enter" });
    expect(await screen.findByRole("alert")).toHaveTextContent("try a nearby address");
  });
});
