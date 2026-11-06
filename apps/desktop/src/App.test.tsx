import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import App from "./App";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((cmd: string) => {
    if (cmd === "app_info") {
      return Promise.resolve({ version: "9.9.9", coreVersion: "9.9.9" });
    }
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  }),
}));

describe("App", () => {
  it("shows the app name and version from the backend", async () => {
    render(<App />);
    expect(screen.getByText("OpenTrainer")).toBeInTheDocument();
    expect(await screen.findByTestId("version")).toHaveTextContent("v9.9.9");
  });
});
