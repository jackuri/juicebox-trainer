import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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

// ----- packs + trainer (mirror ot-packs and commands.rs) -----

export type PackArch = "x86" | "x64";
export type Ui = "toggle" | "slider" | "number" | "button";

export interface Issue {
  severity: "error" | "warning";
  path: string;
  message: string;
}

export interface PackSummary {
  key: string;
  source: "bundled" | "user";
  path: string | null;
  id: string | null;
  name: string | null;
  exe: string | null;
  arch: PackArch | null;
  author: string | null;
  ai: boolean;
  packVersion: string | null;
  cheats: number;
  error: string | null;
  issues: Issue[];
}

export interface Cheat {
  id: string;
  name: string;
  category?: string;
  description?: string;
  ui: Ui;
  hotkey?: string;
  toggle?: { value?: number };
  slider?: { min: number; max: number; step: number; default?: number; hold?: boolean };
  number?: { min?: number; max?: number; hold?: boolean };
  button?: { add?: number; set?: number };
  impl: { kind: "value" | "patch" } & Record<string, unknown>;
}

export interface Pack {
  schema: number;
  game: {
    id: string;
    name: string;
    engine?: string;
    exe: string;
    arch?: PackArch;
    builds: { sha256: string; label?: string; verified?: boolean }[];
  };
  meta: { author: string; created?: string; packVersion: string; notes?: string };
  cheats: Cheat[];
}

export type BuildMatch =
  | { status: "verified" | "listed"; label: string | null }
  | { status: "unknown" | "untagged" | "unreadable" };

export interface CheatState {
  id: string;
  status: "ready" | "active" | "failed";
  active: boolean;
  message: string | null;
  value: string | null;
  address: number | null;
  hotkey: string | null;
  touches: string[];
}

export interface TrainerView {
  key: string;
  pack: Pack;
  build: BuildMatch;
  exeSha256: string | null;
  warnings: Issue[];
  cheats: CheatState[];
  hotkeyErrors: Record<string, string>;
}

export interface PointerScanOptions {
  maxDepth: number;
  maxOffset: number;
  maxResults: number;
}

export interface PointerPathView {
  /** Cheat Engine notation, e.g. `[[game.exe+0x1A2B0]+0x10]+0x18`. */
  expression: string;
  module: string;
  moduleOffset: number;
  offsets: number[];
  /** Where the path leads now, if attached and readable. */
  address: number | null;
}

export interface PointerResults {
  exe: string;
  target: number;
  rounds: number;
  count: number;
  paths: PointerPathView[];
}

export interface DecodedInstr {
  offset: number;
  len: number;
  text: string;
}

export interface Signature {
  pattern: string;
  module: string;
  moduleOffset: number;
  len: number;
  instructions: DecodedInstr[];
}

/** Code changes undone at startup after OpenTrainer closed unexpectedly. */
export interface Recovery {
  exe: string;
  pid: number;
  gameClosed: boolean;
  restored: number;
  problems: string[];
}

/** Packs for this process: same exe name, and same arch when the pack says. */
export function packsFor(packs: PackSummary[], p: ProcessInfo): PackSummary[] {
  return packs.filter(
    (k) =>
      k.exe !== null &&
      k.exe.toLowerCase() === p.name.toLowerCase() &&
      (k.arch === null || k.arch === p.arch),
  );
}

/**
 * Calls `cb` when a hotkey changed the trainer. Returns an unsubscribe
 * function. Outside Tauri (tests) this is a no-op.
 */
export function onTrainerChanged(cb: () => void): () => void {
  let unlisten: (() => void) | null = null;
  let stopped = false;
  try {
    listen("trainer-changed", () => cb()).then(
      (u) => (stopped ? u() : (unlisten = u)),
      () => undefined,
    );
  } catch {
    // Not running inside Tauri.
  }
  return () => {
    stopped = true;
    unlisten?.();
  };
}

// ----- save backups (ot-saves, src-tauri/src/saves.rs) -----

export interface SaveFolder {
  path: string;
  reason: string;
  userAdded: boolean;
  steamCloud: boolean;
}

export interface SaveGame {
  id: string;
  name: string;
  exe: string;
  steam: { appId: number; name: string | null } | null;
  cloud: { synced: boolean; reason: string | null };
  folders: SaveFolder[];
  addedMs: number;
  running: boolean;
  cloudSynced: boolean;
  backups: number;
  latestMs: number | null;
}

export interface SavesOverview {
  games: SaveGame[];
  steamRunning: boolean;
  root: string | null;
}

export type BackupReason = "launch" | "exit" | "manual" | "beforeRestore" | "beforeEdit";

export interface Backup {
  id: string;
  createdMs: number;
  reason: BackupReason;
  note: string;
  files: number;
  totalSize: number;
  skipped: { folder: number; path: string; error: string }[];
  folders: string[];
}

export interface BackupCreated {
  snapshot: Backup;
  new: boolean;
}

export interface RestoreReport {
  snapshot: string;
  safetySnapshot: string;
  written: number;
  unchanged: number;
  removed: string[];
  extra: string[];
  cloudWarning: string | null;
}

export type LineTag = "same" | "removed" | "added";

export interface TextHunk {
  oldLine: number;
  newLine: number;
  lines: { tag: LineTag; text: string }[];
}

export interface ByteRange {
  offset: number;
  len: number;
  old: string;
  new: string;
  oldI32: number | null;
  newI32: number | null;
}

export type DiffDetail =
  | { kind: "text"; hunks: TextHunk[]; more: number }
  | { kind: "binary"; ranges: ByteRange[]; more: number }
  | { kind: "tooLarge" };

export interface FileChange {
  folder: string;
  path: string;
  change: "added" | "removed" | "changed";
  oldSize: number | null;
  newSize: number | null;
  detail: DiffDetail | null;
}

export interface SaveDiff {
  files: FileChange[];
  unchanged: number;
}

export interface SaveFile {
  folder: string;
  /** Path inside the folder; empty for a registry key. */
  path: string;
  size: number;
  modifiedMs: number | null;
  registry: boolean;
}

export type FieldKind = "int" | "float" | "bool" | "text" | "null" | "other";

export interface SaveField {
  path: string;
  kind: FieldKind;
  value: string;
  editable: boolean;
  note: string | null;
}

export interface FileView {
  folder: string;
  path: string;
  layers: string[];
  format: string;
  codec: string;
  fields: SaveField[];
  truncated: boolean;
  bodyLen: number;
  note: string | null;
  registry: boolean;
}

export type SaveEdit =
  | { kind: "set"; path: string; value: string }
  | { kind: "patch"; offset: number; hex: string };

export interface EditReport {
  backup: string;
  changed: boolean;
  cloudWarning: string | null;
}

export interface QuickCode {
  packKey: string;
  packName: string;
  id: string;
  name: string;
  codec: string;
  file: string;
  path: string;
  value: number | string | boolean;
  problem: string | null;
}

/**
 * Calls `cb` when the save watcher backed something up or a game started or
 * closed. Returns an unsubscribe function. Outside Tauri this is a no-op.
 */
export function onSavesChanged(cb: () => void): () => void {
  let unlisten: (() => void) | null = null;
  let stopped = false;
  try {
    listen("saves-changed", () => cb()).then(
      (u) => (stopped ? u() : (unlisten = u)),
      () => undefined,
    );
  } catch {
    // Not running inside Tauri.
  }
  return () => {
    stopped = true;
    unlisten?.();
  };
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

  packsList: () => invoke<PackSummary[]>("packs_list"),
  packsFolder: () => invoke<string>("packs_folder"),
  openPacksFolder: () => invoke<void>("open_packs_folder"),
  trainerLoad: (key: string) => invoke<TrainerView>("trainer_load", { key }),
  trainerState: () => invoke<TrainerView | null>("trainer_state"),
  trainerUnload: () => invoke<void>("trainer_unload"),
  trainerToggle: (id: string, on: boolean) => invoke<TrainerView>("trainer_toggle", { id, on }),
  trainerSet: (id: string, value: string) => invoke<TrainerView>("trainer_set", { id, value }),
  trainerRelease: (id: string) => invoke<TrainerView>("trainer_release", { id }),
  trainerPress: (id: string) => invoke<TrainerView>("trainer_press", { id }),

  pointerScan: (target: string, options: PointerScanOptions) =>
    invoke<PointerResults>("pointer_scan", { target, options }),
  pointerFilter: (target: string) => invoke<PointerResults>("pointer_filter", { target }),
  pointerResults: () => invoke<PointerResults | null>("pointer_results"),
  pointerClear: () => invoke<void>("pointer_clear"),
  signatureMake: (address: string) => invoke<Signature>("signature_make", { address }),
  recoveryReport: () => invoke<Recovery[]>("recovery_report"),

  savesOverview: () => invoke<SavesOverview>("saves_overview"),
  savesAddProcess: (pid: number) => invoke<SaveGame>("saves_add_process", { pid }),
  savesAddExe: (path: string) => invoke<SaveGame>("saves_add_exe", { path }),
  savesRemoveGame: (id: string, deleteBackups: boolean) =>
    invoke<void>("saves_remove_game", { id, deleteBackups }),
  savesRelocate: (id: string) => invoke<SaveGame>("saves_relocate", { id }),
  savesAddFolder: (id: string, path: string) => invoke<SaveGame>("saves_add_folder", { id, path }),
  savesRemoveFolder: (id: string, path: string) =>
    invoke<SaveGame>("saves_remove_folder", { id, path }),
  savesBackups: (id: string) => invoke<Backup[]>("saves_backups", { id }),
  savesBackupNow: (id: string, note: string) =>
    invoke<BackupCreated>("saves_backup_now", { id, note }),
  savesDeleteBackup: (id: string, backup: string) =>
    invoke<void>("saves_delete_backup", { id, backup }),
  savesSetNote: (id: string, backup: string, note: string) =>
    invoke<Backup>("saves_set_note", { id, backup, note }),
  /** `null` means the saves as they are now. */
  savesDiff: (id: string, from: string | null, to: string | null) =>
    invoke<SaveDiff>("saves_diff", { id, from, to }),
  savesRestore: (id: string, backup: string, removeExtra: boolean) =>
    invoke<RestoreReport>("saves_restore", { id, backup, removeExtra }),
  savesOpenFolder: (id: string, path: string) =>
    invoke<void>("saves_open_folder", { id, path }),
  savesFiles: (id: string) => invoke<SaveFile[]>("saves_files", { id }),
  savesOpenFile: (id: string, folder: string, path: string) =>
    invoke<FileView>("saves_open_file", { id, folder, path }),
  savesHex: (id: string, folder: string, path: string, offset: number, len: number) =>
    invoke<number[]>("saves_hex", { id, folder, path, offset, len }),
  savesEdit: (id: string, folder: string, path: string, edits: SaveEdit[]) =>
    invoke<EditReport>("saves_edit", { id, folder, path, edits }),
  savesQuickCodes: (id: string) => invoke<QuickCode[]>("saves_quick_codes", { id }),
  savesApplyQuickCode: (id: string, packKey: string, codeId: string) =>
    invoke<EditReport>("saves_apply_quick_code", { id, packKey, codeId }),
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
