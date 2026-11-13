// The phone app's calls into its Rust side (src-tauri/src/lib.rs).
import { invoke } from "@tauri-apps/api/core";

/** A paired PC. */
export interface Pc {
  id: string;
  name: string;
  address: string;
}

/** What went wrong (from src-tauri/src/pcs.rs). */
export interface Problem {
  kind: "unreachable" | "unpaired" | "refused" | "badLink" | "notFound" | "storage";
  message: string;
}

/** One cheat, as the PC shows it (JuiceBox Trainer's remote_view). */
export interface Cheat {
  id: string;
  name: string;
  category: string | null;
  ui: "toggle" | "slider" | "number" | "button";
  active: boolean;
  status: "ready" | "active" | "failed";
  message: string | null;
  value: string | null;
  min: number | null;
  max: number | null;
  step: number | null;
  label: string | null;
}

/** What the PC shows now. */
export interface View {
  pack: { game: string } | null;
  /** The game that closed last (when no pack is loaded). */
  closed?: string | null;
  cheats?: Cheat[];
  presets?: string[];
  /** Preset cheats still waiting for the game. */
  pending?: string[];
}

export interface Action {
  id: string;
  op: "toggle" | "press" | "set" | "release";
  on?: boolean;
  value?: string;
}

export function isProblem(e: unknown): e is Problem {
  return typeof e === "object" && e !== null && "kind" in e && "message" in e;
}

export function explain(e: unknown): string {
  if (isProblem(e)) return e.message.charAt(0).toUpperCase() + e.message.slice(1);
  return "Something went wrong. Try again.";
}

export const api = {
  pcs: () => invoke<Pc[]>("pcs_list"),
  pair: (link: string, phone: string) => invoke<Pc>("pc_pair", { link, phone }),
  forget: (id: string) => invoke<void>("pc_forget", { id }),
  state: (id: string) => invoke<View>("pc_state", { id }),
  act: (id: string, action: Action) => invoke<View>("pc_action", { id, action }),
  preset: (id: string, name: string) => invoke<View>("pc_preset", { id, name }),
};

/**
 * Scans a QR code with the camera. `null` when the phone has no camera, the player said no to
 * it, or this isn't a phone (the PC build, for development).
 */
export async function scanQr(): Promise<string | null> {
  try {
    const scanner = await import("@tauri-apps/plugin-barcode-scanner");
    let allowed = await scanner.checkPermissions();
    if (allowed !== "granted") allowed = await scanner.requestPermissions();
    if (allowed !== "granted") return null;
    const result = await scanner.scan({ windowed: false, formats: [scanner.Format.QRCode] });
    return result.content || null;
  } catch {
    return null;
  }
}

/** Remembers small things on this phone (the last PC opened, this phone's name). */
export const prefs = {
  get(key: string): string | null {
    try {
      return localStorage.getItem(`juicebox.remote.${key}`);
    } catch {
      return null;
    }
  },
  set(key: string, value: string | null) {
    try {
      if (value === null) localStorage.removeItem(`juicebox.remote.${key}`);
      else localStorage.setItem(`juicebox.remote.${key}`, value);
    } catch {
      // Not saved: fine, it's only a convenience.
    }
  },
};
