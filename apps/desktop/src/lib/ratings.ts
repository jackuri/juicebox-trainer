/** Community pack ratings (v1.29.0): 1 star broken, 2 works with bugs, 3 works great. */
import type { Rating } from "./ipc";

export const STAR_WORDS: Record<1 | 2 | 3, string> = {
  1: "Broken",
  2: "Works, some bugs",
  3: "Works great",
};

/** What players say, from the average. */
export function ratingWords(stars: number): string {
  return stars >= 2.5 ? STAR_WORDS[3] : stars >= 1.5 ? STAR_WORDS[2] : STAR_WORDS[1];
}

/** "★★★" for 2.7, "★★☆" for 2.2: the average rounded to whole stars. */
export function starGlyphs(stars: number): string {
  const n = Math.min(3, Math.max(1, Math.round(stars)));
  return "★".repeat(n) + "☆".repeat(3 - n);
}

/** Best rated first: more stars, then more ratings; packs without ratings last, by name. */
export function bestFirst<T extends { id: string; name: string }>(list: T[], ratings: Record<string, Rating>): T[] {
  const score = (p: T) => {
    const r = ratings[p.id];
    return r && r.count > 0 ? r : null;
  };
  return [...list].sort((a, b) => {
    const ra = score(a);
    const rb = score(b);
    if (ra && rb) return rb.stars - ra.stars || rb.count - ra.count || a.name.localeCompare(b.name);
    if (ra || rb) return ra ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
}

const KEY = "juicebox.myRatings";

/** The stars this player gave a pack from this PC (the list itself updates within the hour). */
export function myRating(id: string): 1 | 2 | 3 | null {
  try {
    const all = JSON.parse(localStorage.getItem(KEY) ?? "{}") as Record<string, number>;
    const v = all[id];
    return v === 1 || v === 2 || v === 3 ? v : null;
  } catch {
    return null;
  }
}

export function rememberRating(id: string, stars: 1 | 2 | 3): void {
  try {
    const all = JSON.parse(localStorage.getItem(KEY) ?? "{}") as Record<string, number>;
    all[id] = stars;
    localStorage.setItem(KEY, JSON.stringify(all));
  } catch {
    // Only a reminder for this PC.
  }
}
