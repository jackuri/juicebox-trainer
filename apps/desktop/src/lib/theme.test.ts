import { afterEach, describe, expect, it } from "vitest";
import { applyTheme, DEFAULT_THEME, loadTheme, saveTheme, THEME_KEY, THEMES, watchTheme } from "./theme";

describe("themes", () => {
  afterEach(() => {
    localStorage.clear();
    delete document.documentElement.dataset.theme;
  });

  it("lists the default first and has unique ids", () => {
    expect(THEMES[0]?.id).toBe(DEFAULT_THEME);
    expect(new Set(THEMES.map((t) => t.id)).size).toBe(THEMES.length);
  });

  it("loads the default when nothing or something unknown is saved", () => {
    expect(loadTheme()).toBe(DEFAULT_THEME);
    localStorage.setItem(THEME_KEY, "no-such-theme");
    expect(loadTheme()).toBe(DEFAULT_THEME);
  });

  it("saves, loads and shows a theme", () => {
    saveTheme("terminal");
    expect(localStorage.getItem(THEME_KEY)).toBe("terminal");
    expect(loadTheme()).toBe("terminal");
    expect(document.documentElement.dataset.theme).toBe("terminal");
  });

  it("never shows an unknown theme", () => {
    applyTheme("nope");
    expect(document.documentElement.dataset.theme).toBe(DEFAULT_THEME);
  });

  it("follows a theme picked in another window", () => {
    const stop = watchTheme();
    window.dispatchEvent(new StorageEvent("storage", { key: THEME_KEY, newValue: "pocket" }));
    expect(document.documentElement.dataset.theme).toBe("pocket");
    window.dispatchEvent(new StorageEvent("storage", { key: "other", newValue: "amber" }));
    expect(document.documentElement.dataset.theme).toBe("pocket");
    stop();
    window.dispatchEvent(new StorageEvent("storage", { key: THEME_KEY, newValue: "amber" }));
    expect(document.documentElement.dataset.theme).toBe("pocket");
  });
});
