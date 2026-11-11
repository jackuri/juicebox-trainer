import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { THEME_KEY, THEMES } from "../lib/theme";
import { ThemeView } from "./ThemeView";

describe("ThemeView", () => {
  afterEach(() => {
    localStorage.clear();
    delete document.documentElement.dataset.theme;
  });

  it("shows every theme with a live preview and marks the default as in use", () => {
    const { container } = render(<ThemeView />);
    for (const t of THEMES) expect(screen.getByRole("button", { name: new RegExp(t.name) })).toBeInTheDocument();
    expect(container.querySelectorAll("[data-theme]")).toHaveLength(THEMES.length);
    expect(screen.getByRole("button", { name: /JuiceBox Trainer/ })).toHaveAttribute("aria-pressed", "true");
  });

  it("switches and remembers the theme when one is clicked", () => {
    render(<ThemeView />);
    const terminal = screen.getByRole("button", { name: /Wasteland Terminal/ });
    fireEvent.click(terminal);
    expect(terminal).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: /JuiceBox Trainer/ })).toHaveAttribute("aria-pressed", "false");
    expect(document.documentElement.dataset.theme).toBe("terminal");
    expect(localStorage.getItem(THEME_KEY)).toBe("terminal");
  });

  it("turns holiday juice boxes off and on (v1.24.0)", () => {
    render(<ThemeView />);
    const box = screen.getByRole("checkbox", { name: /Holiday juice boxes/ });
    expect(box).toBeChecked();
    fireEvent.click(box);
    expect(box).not.toBeChecked();
    expect(localStorage.getItem("juicebox.seasonal")).toBe("off");
  });

  it("opens on the saved theme", () => {
    localStorage.setItem(THEME_KEY, "pocket");
    render(<ThemeView />);
    expect(screen.getByRole("button", { name: /Pocket LCD/ })).toHaveAttribute("aria-pressed", "true");
  });
});
