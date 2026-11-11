/** The app's looks (v1.14.0). The colours live in themes.css; this is the list and the choice. */

export interface Theme {
  id: string;
  name: string;
  description: string;
  light?: boolean;
}

export const DEFAULT_THEME = "default";

export const THEMES: readonly Theme[] = [
  { id: DEFAULT_THEME, name: "JuiceBox Trainer", description: "The standard look: deep navy with a blue-violet accent." },
  { id: "arcade", name: "Retro Arcade", description: "Hot-pink neon on a purple-black arcade cabinet." },
  { id: "nextgen", name: "Next-Gen Blue", description: "A clean blue-black console look with bright blue highlights and round corners." },
  { id: "green", name: "Console Green", description: "Charcoal greys and a vivid green, like a console dashboard." },
  { id: "pocket", name: "Pocket LCD", description: "The four greens of an old handheld's screen, with blocky corners.", light: true },
  { id: "hud", name: "Visor HUD", description: "Sci-fi helmet read-outs: steel blue armour, cyan glow, sharp corners." },
  { id: "terminal", name: "Wasteland Terminal", description: "A green phosphor computer screen, with scanlines." },
  { id: "amber", name: "Amber Terminal", description: "An old amber monitor, with scanlines." },
  { id: "inferno", name: "Inferno", description: "Black and ember red." },
  { id: "daylight", name: "Daylight", description: "A plain light theme for bright rooms.", light: true },
];

export const THEME_KEY = "opentrainer.theme";

function known(id: string | null | undefined): string {
  return THEMES.some((t) => t.id === id) ? (id as string) : DEFAULT_THEME;
}

/** The saved theme, or the default when nothing (or something unknown) is saved. */
export function loadTheme(): string {
  try {
    return known(localStorage.getItem(THEME_KEY));
  } catch {
    return DEFAULT_THEME;
  }
}

/** Shows a theme on the whole window. */
export function applyTheme(id: string, root: HTMLElement = document.documentElement): void {
  root.dataset.theme = known(id);
}

/** Shows and remembers a theme (other JuiceBox Trainer windows follow; see watchTheme). */
export function saveTheme(id: string): void {
  const theme = known(id);
  applyTheme(theme);
  try {
    localStorage.setItem(THEME_KEY, theme);
  } catch {
    // Not saved, but still shown.
  }
}

/** Follows a theme picked in another window (the in-game overlay). Returns a stop function. */
export function watchTheme(onChange: (id: string) => void = applyTheme): () => void {
  const listener = (e: StorageEvent) => {
    if (e.key === THEME_KEY) onChange(known(e.newValue));
  };
  window.addEventListener("storage", listener);
  return () => window.removeEventListener("storage", listener);
}
