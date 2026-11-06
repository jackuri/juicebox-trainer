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

export type FirstScanKind = "exact" | "between" | "unknown";
export type NextScanKind =
  | "exact"
  | "between"
  | "changed"
  | "unchanged"
  | "increased"
  | "decreased";

export type ScanMode =
  | { kind: "exact"; value: string }
  | { kind: "between"; min: string; max: string }
  | { kind: "unknown" | "changed" | "unchanged" | "increased" | "decreased" };

export interface ScanOptions {
  writableOnly: boolean;
  aligned: boolean;
  floatTolerance: number | null;
}

export interface ScanSummary {
  count: number;
  ty: ValueType;
  rounds: number;
  memoryBytes: number;
}

export interface ScanProgress {
  running: boolean;
  done: number;
  total: number;
}

export interface ScanHit {
  address: number;
  previous: string;
  current: string | null;
}

export interface ScanResults {
  count: number;
  hits: ScanHit[];
}

export interface ListEntry {
  id: number;
  label: string;
  address: number;
  addressExpr: string;
  ty: ValueType;
  value: string | null;
  error: string | null;
  frozen: boolean;
}

/** Errors that mean the session is gone (the backend already dropped it). */
export function isSessionLost(e: CommandError): boolean {
  return e.kind === "blocked" || e.kind === "processExited" || e.kind === "notAttached";
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

  scanFirst: (ty: ValueType, mode: ScanMode, options: ScanOptions) =>
    invoke<ScanSummary>("scan_first", { ty, mode, options }),
  scanNext: (mode: ScanMode) => invoke<ScanSummary>("scan_next", { mode }),
  scanCancel: () => invoke<void>("scan_cancel"),
  scanProgress: () => invoke<ScanProgress>("scan_progress"),
  scanReset: () => invoke<void>("scan_reset"),
  scanResults: (offset: number, limit: number) =>
    invoke<ScanResults>("scan_results", { offset, limit }),

  addressList: () => invoke<ListEntry[]>("address_list"),
  addressAdd: (address: string, ty: ValueType, label: string) =>
    invoke<ListEntry>("address_add", { address, ty, label }),
  addressRemove: (id: number) => invoke<void>("address_remove", { id }),
  addressRename: (id: number, label: string) => invoke<void>("address_rename", { id, label }),
  addressSetFrozen: (id: number, frozen: boolean) =>
    invoke<ListEntry>("address_set_frozen", { id, frozen }),
  addressWrite: (id: number, value: string) =>
    invoke<ListEntry>("address_write", { id, value }),
};

/** Builds the scan mode the backend expects from form fields. */
export function scanMode(
  kind: FirstScanKind | NextScanKind,
  value: string,
  max: string,
): ScanMode {
  if (kind === "exact") return { kind, value };
  if (kind === "between") return { kind, min: value, max };
  return { kind };
}
