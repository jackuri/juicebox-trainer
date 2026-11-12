/** The AI helper's settings, kept on this PC (shared by Make a trainer and the live helper, v1.30.0). */

export type Backend = "subscription" | "apiKey" | "local";

export interface Settings {
  backend: Backend;
  budget: string;
  localUrl: string;
  model: string;
  analysisCommand: string;
  analysisArgs: string;
  /** The helper may look at the game window (v1.21.0). */
  pictures: boolean;
  /** Claude backends: the best model, or a cheaper and faster one. */
  quality: "best" | "cheaper";
}

/** The cheaper Claude model (the best one is the helper's default). */
export const CHEAPER_MODEL = "claude-sonnet-5-5";

const DEFAULTS: Settings = {
  backend: "subscription",
  budget: "5",
  localUrl: "http://localhost:11434",
  model: "",
  analysisCommand: "",
  analysisArgs: "",
  pictures: true,
  quality: "best",
};

const SETTINGS_KEY = "opentrainer.ai.settings";

export function loadSettings(): Settings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    return raw ? { ...DEFAULTS, ...(JSON.parse(raw) as Partial<Settings>) } : DEFAULTS;
  } catch {
    return DEFAULTS;
  }
}

export function saveSettings(s: Settings) {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(s));
  } catch {
    // Settings are a convenience; the helper works without them.
  }
}

/** The parts of a helper start request that come from the settings. */
export function fromSettings(s: Settings) {
  return {
    backend: s.backend,
    maxBudgetUsd: Number(s.budget) || 5,
    localUrl: s.backend === "local" ? s.localUrl : undefined,
    model: s.backend === "local" ? s.model : s.quality === "cheaper" ? CHEAPER_MODEL : undefined,
    pictures: s.pictures ? undefined : false,
    analysisCommand: s.analysisCommand.trim() || undefined,
    analysisArgs: s.analysisArgs.split(" ").filter(Boolean),
  };
}

/** The helper can start with these settings (signed in, a local model named). */
export function settingsReady(s: Settings, signedIn: boolean | null): boolean {
  if (s.backend === "subscription" && signedIn === false) return false;
  if (s.backend === "local" && !s.model.trim()) return false;
  return true;
}
