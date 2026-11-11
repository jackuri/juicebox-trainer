import { useEffect, useState } from "react";

/**
 * The juice box's faces (v1.24.0): the everyday smile, a holiday look by month, a wink when something
 * worked and crossed-out eyes on errors. Pictures are in `public/juicebox/`.
 */
export type Season = "default" | "halloween" | "thanksgiving" | "christmas" | "valentine";
export type Mood = "wink" | "error";

const KEY = "juicebox.seasonal";
const EVENT = "juicebox-seasonal";

/** The holiday look for a date (months are 0-based). */
export function seasonOf(date: Date): Season {
  switch (date.getMonth()) {
    case 1:
      return "valentine";
    case 9:
      return "halloween";
    case 10:
      return "thanksgiving";
    case 11:
      return "christmas";
    default:
      return "default";
  }
}

/** Whether holiday juice boxes are on (default: on). */
export function seasonalOn(): boolean {
  try {
    return localStorage.getItem(KEY) !== "off";
  } catch {
    return true;
  }
}

export function setSeasonalOn(on: boolean): void {
  try {
    localStorage.setItem(KEY, on ? "on" : "off");
  } catch {
    /* kept for this session only */
  }
  window.dispatchEvent(new Event(EVENT));
}

/** The picture for a mood, or for the season. */
export function pictureFor(mood?: Mood, season: Season = "default"): string {
  return `/juicebox/${mood ?? season}.png`;
}

/** The look to show now: follows the month and the Themes switch (checked hourly). */
export function useSeason(): Season {
  const pick = () => (seasonalOn() ? seasonOf(new Date()) : "default");
  const [season, setSeason] = useState<Season>(pick);
  useEffect(() => {
    const update = () => setSeason(pick());
    window.addEventListener(EVENT, update);
    const timer = window.setInterval(update, 60 * 60 * 1000);
    return () => {
      window.removeEventListener(EVENT, update);
      window.clearInterval(timer);
    };
  }, []);
  return season;
}
