import { afterEach, describe, expect, it } from "vitest";
import { pictureFor, seasonalOn, seasonOf, setSeasonalOn } from "./mascot";

afterEach(() => localStorage.clear());

describe("holiday juice boxes", () => {
  it("dresses up by month", () => {
    expect(seasonOf(new Date(2026, 1, 14))).toBe("valentine");
    expect(seasonOf(new Date(2026, 9, 31))).toBe("halloween");
    expect(seasonOf(new Date(2026, 10, 1))).toBe("thanksgiving");
    expect(seasonOf(new Date(2026, 10, 30))).toBe("thanksgiving");
    expect(seasonOf(new Date(2026, 11, 25))).toBe("christmas");
    for (const m of [0, 2, 3, 4, 5, 6, 7, 8]) expect(seasonOf(new Date(2026, m, 10))).toBe("default");
  });

  it("is on unless turned off in Themes", () => {
    expect(seasonalOn()).toBe(true);
    setSeasonalOn(false);
    expect(seasonalOn()).toBe(false);
    setSeasonalOn(true);
    expect(seasonalOn()).toBe(true);
  });

  it("has a face for each moment", () => {
    expect(pictureFor(undefined, "christmas")).toBe("/juicebox/christmas.png");
    expect(pictureFor("wink", "christmas")).toBe("/juicebox/wink.png");
    expect(pictureFor("error")).toBe("/juicebox/error.png");
  });
});
