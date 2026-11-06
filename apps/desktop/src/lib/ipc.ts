import { invoke } from "@tauri-apps/api/core";

// Types mirror the Rust structs in ot-core and src-tauri/src/commands.rs.

export type Arch = "x86" | "x64" | "arm64" | "unknown";

export interface AppInfo {
  version: string;
  coreVersion: string;
}

export interface ProcessInfo {
  pid: number;
  parentPid: number;
  name: string;
  exePath: string | null;
  arch: Arch;
  threads: number;
}

export type FindingKind =
  | "process"
  | "service"
  | "driver"
  | "module"
  | "installMarker"
  | "onlineOnly"
  | "unverifiable";

export interface Finding {
  kind: FindingKind;
  product: string;
  detail: string;
}

export interface Verdict {
  pid: number;
  target: string;
  allowed: boolean;
  findings: Finding[];
  signaturesVersion: number;
}

export interface Module {
  name: string;
  path: string;
  base: number;
  size: number;
}

export interface SessionInfo {
  process: ProcessInfo;
  modules: Module[];
}

export const VALUE_TYPES = [
  "i8",
  "u8",
  "i16",
  "u16",
  "i32",
  "u32",
  "i64",
  "u64",
  "f32",
  "f64",
  "ptr",
] as const;
export type ValueType = (typeof VALUE_TYPES)[number];

export interface ReadResult {
  address: number;
  value: string;
  bytes: string;
}

export interface CommandError {
  kind: string;
  message: string;
  verdict: Verdict | null;
}

export function isCommandError(e: unknown): e is CommandError {
  return (
    typeof e === "object" &&
    e !== null &&
    "kind" in e &&
    "message" in e &&
    typeof (e as { message: unknown }).message === "string"
  );
}

/** Normalizes anything thrown by `invoke` into a CommandError. */
export function toCommandError(e: unknown): CommandError {
  if (isCommandError(e)) return { ...e, verdict: e.verdict ?? null };
  return { kind: "unknown", message: String(e), verdict: null };
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  listProcesses: () => invoke<ProcessInfo[]>("list_processes"),
  checkGuard: (pid: number) => invoke<Verdict>("check_guard", { pid }),
  attach: (pid: number) => invoke<SessionInfo>("attach", { pid }),
  detach: () => invoke<void>("detach"),
  currentSession: () => invoke<SessionInfo | null>("current_session"),
  refreshModules: () => invoke<Module[]>("refresh_modules"),
  readValue: (address: string, ty: ValueType) =>
    invoke<ReadResult>("read_value", { address, ty }),
  writeValue: (address: string, ty: ValueType, value: string) =>
    invoke<ReadResult>("write_value", { address, ty, value }),
};
