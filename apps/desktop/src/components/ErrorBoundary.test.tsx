import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ErrorBoundary } from "./ErrorBoundary";

let broken = true;
function Screen() {
  if (broken) throw new Error("Cannot read properties of null (reading 'toFixed')");
  return <p>The screen is back</p>;
}

describe("ErrorBoundary", () => {
  it("keeps a screen's problem inside it, and Try again shows it again", () => {
    const quiet = vi.spyOn(console, "error").mockImplementation(() => undefined);
    render(
      <div>
        <nav>Tabs still here</nav>
        <ErrorBoundary label="The Web games screen">
          <Screen />
        </ErrorBoundary>
      </div>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("The Web games screen ran into a problem");
    expect(screen.getByText("Tabs still here")).toBeInTheDocument();
    broken = false;
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(screen.getByText("The screen is back")).toBeInTheDocument();
    quiet.mockRestore();
  });
});
