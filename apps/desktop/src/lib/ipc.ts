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

export type PackSource = "bundled" | "user" | "imported" | "community";

export const PACK_SOURCE_LABEL: Record<PackSource, string> = {
  bundled: "built-in example",
  user: "your packs",
  imported: "imported",
  community: "community",
};

export interface PackSummary {
  key: string;
  source: PackSource;
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
  /** Cheat id → the player's controller combo. */
  pads: Record<string, string>;
  settings: GameSettings;
  /** Preset cheats still waiting to turn on (e.g. until a level loads). */
  pending: string[];
}

// ----- presets, own keys (ot-packs settings) -----

export interface PresetCheat {
  id: string;
  value?: string;
}

export interface Preset {
  name: string;
  cheats: PresetCheat[];
}

export interface GameSettings {
  presets: Preset[];
  autoPreset?: string;
  favourites: string[];
  notes: Record<string, string>;
  /** Cheat id → own key; "" means no key. */
  keys: Record<string, string>;
  pads: Record<string, string>;
}

// ----- games library (src-tauri/src/library.rs) -----

export type StoreKind = "steam" | "epic" | "gog" | "xbox" | "own";

export interface LibraryGame {
  id: string;
  name: string;
  store: StoreKind;
  folder: string;
  exe: string | null;
  launch: { kind: "uri" | "exe"; target: string } | { kind: "manual" };
  pack: { key: string; name: string; ai: boolean; exe: string } | null;
  presets: string[];
  autoPreset: string | null;
  /** Why the anti-cheat guard blocks it. */
  blocked: string | null;
  /** A cover picture is on this PC (`libraryCover`): wide artwork, or an icon / logo. */
  cover: "art" | "icon" | null;
}

/** A running program that is one of the player's games (installed, or with a pack). */
export interface RunningGame {
  pid: number;
  name: string;
  libraryId: string | null;
}

export interface PlayResult {
  session: SessionInfo;
  trainer: TrainerView | null;
  preset: string | null;
}

/** Calls `cb` with each "Play with cheats" step. */
/** Subscribes to a backend event; returns the unsubscribe function. */
function on<T>(event: string, cb: (payload: T) => void): () => void {
  let unlisten: (() => void) | null = null;
  let stopped = false;
  try {
    listen<T>(event, (e) => cb(e.payload)).then(
      (u) => (stopped ? u() : (unlisten = u)),
      () => undefined,
    );
  } catch {
    // Not running inside Tauri (tests).
  }
  return () => {
    stopped = true;
    unlisten?.();
  };
}

// ----- sharing packs (mirror sharing.rs) -----

export interface CheatLine {
  name: string;
  ui: Ui;
  description: string | null;
  /** Changes game code (rather than values). */
  patch: boolean;
}

export interface PackPreview {
  id: string;
  game: string;
  exe: string;
  author: string;
  ai: boolean;
  version: string;
  notes: string | null;
  cheats: CheatLine[];
  warnings: Issue[];
  existing: { key: string; name: string; version: string; ai: boolean; same: boolean } | null;
  /** Cheat Engine table entries that didn't come over, and why. */
  skipped?: { name: string; why: string }[];
  /** A converted table that doesn't say which exe it's for: choose it before installing. */
  needsExe?: boolean;
}

/** GitHub sign-in for sharing packs with the community list. */
export interface GithubStatus {
  /** This version knows OpenTrainer's GitHub app. */
  configured: boolean;
  signedIn: boolean;
  login: string | null;
}

export interface DeviceCode {
  /** What the player types on GitHub's page, e.g. ABCD-1234. */
  userCode: string;
  expiresIn: number;
}

/** A pack sent to the community list (a pull request a maintainer checks). */
export interface SharedPack {
  url: string;
  number: number;
  /** The list already had a pack for this game. */
  update: boolean;
}

/** Author text of packs converted from a Cheat Engine table. */
export const TABLE_AUTHOR = "Cheat Engine table (imported)";

export interface ImportResult {
  key: string;
  installed: "new" | "same" | "replaced";
}

export interface CommunityPack {
  id: string;
  name: string;
  exe: string;
  arch: PackArch | null;
  engine: string | null;
  author: string;
  packVersion: string;
  cheats: number;
  notes: string | null;
  sha256: string;
  size: number;
  installed: "none" | "installed" | "update" | "other";
}

export interface CommunityList {
  url: string;
  contributeUrl: string;
  updated: string | null;
  packs: CommunityPack[];
  skipped: string[];
}

/** Files dropped on the window (their paths). */
/** Files OpenTrainer opens as packs: `.otpack`, or a plain pack `.json`. */
export function isPackFile(path: string): boolean {
  return /\.(otpack|json|ct)$/i.test(path.trim());
}

/** A Cheat Engine table (converted into a pack when opened). */
export function isTable(path: string): boolean {
  return /\.ct$/i.test(path.trim());
}

export function onFileDrop(cb: (paths: string[]) => void): () => void {
  let unlisten: (() => void) | null = null;
  let stopped = false;
  try {
    void import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent((e) => {
          if (e.payload.type === "drop") cb(e.payload.paths);
        }),
      )
      .then(
        (u) => (stopped ? u() : (unlisten = u)),
        () => undefined,
      );
  } catch {
    // Not running inside Tauri (tests).
  }
  return () => {
    stopped = true;
    unlisten?.();
  };
}

/** An .otpack opened from Explorer while OpenTrainer runs. */
export function onOtpackOpen(cb: (path: string) => void): () => void {
  return on<string>("otpack-open", cb);
}

// ----- updates (mirror updates.rs) -----

export interface UpdateStatus {
  version: string;
  enabled: boolean;
  offReason: string | null;
  releasesUrl: string;
}

export interface UpdateInfo {
  version: string;
  notes: string | null;
  date: string | null;
}

export function onUpdateAvailable(cb: (info: UpdateInfo) => void): () => void {
  return on<UpdateInfo>("update-available", cb);
}

export function onUpdateProgress(cb: (got: number, total: number | null) => void): () => void {
  return on<[number, number | null]>("update-progress", ([got, total]) => cb(got, total));
}

export function onAiInstallProgress(cb: (message: string) => void): () => void {
  return on<string>("ai-install-progress", cb);
}

export function onLibraryProgress(cb: (message: string) => void): () => void {
  let unlisten: (() => void) | null = null;
  let stopped = false;
  try {
    listen<string>("library-progress", (e) => cb(e.payload)).then(
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

// ----- phone remote (src-tauri/src/remote.rs) -----

export interface RemoteStatus {
  pairUrl: string;
  qrSvg: string;
  address: string;
  devices: number;
}

export interface RepairFix {
  cheat: string;
  oldAob: string;
  newAob: string;
  at: string;
  matched: number;
  of: number;
}

export interface RepairResult {
  fixed: RepairFix[];
  broken: [string, string][];
  saved: string | null;
  view: TrainerView;
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

// ----- AI helper (sidecars/ai-agent, src-tauri/src/ai.rs) -----

export type AskKind = "done" | "yesno" | "text";

export type AiEvent =
  | { type: "ready"; version: string }
  | { type: "say"; text: string }
  | { type: "ask"; id: string; text: string; kind: AskKind }
  | { type: "answered"; id: string; text: string }
  /** A message the player typed while the helper worked. */
  | { type: "note"; text: string }
  | { type: "activity"; tool: string; summary: string }
  | { type: "cost"; usd: number }
  | { type: "finished"; ok: boolean; summary: string }
  | { type: "error"; message: string }
  | { type: "exit" };

export interface AiStatus {
  available: boolean;
  problem: string | null;
  running: boolean;
  transcript: AiEvent[];
  /** Signed in to a Claude account; null if it couldn't be checked. */
  signedIn: boolean | null;
  /** The AI engine: downloaded on first use in the installed app. */
  download: "notNeeded" | "missing" | "outdated" | "ready";
  installing: boolean;
}

export interface AiStartRequest {
  pid: number;
  /** "trainer" (default) or "explainSave" (needs exe and backup). */
  task?: "trainer" | "explainSave";
  exe?: string;
  backup?: string;
  wishes: string[];
  notes: string;
  backend: "subscription" | "apiKey" | "local";
  maxBudgetUsd: number;
  /** For "local": Ollama (http://localhost:11434) or LM Studio (http://localhost:1234). */
  localUrl?: string;
  model?: string;
  /** Optional static-analysis tool server (e.g. REA). */
  analysisCommand?: string;
  analysisArgs?: string[];
}

/** Calls `cb` for each message from the AI helper. Returns an unsubscribe function. */
export function onAiEvent(cb: (e: AiEvent) => void): () => void {
  let unlisten: (() => void) | null = null;
  let stopped = false;
  try {
    listen<AiEvent>("ai-event", (e) => cb(e.payload)).then(
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
  runningGames: () => invoke<RunningGame[]>("running_games"),
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
  packsPendingOpen: () => invoke<string | null>("packs_pending_open"),
  /** Asks where to save; null if the player cancelled. */
  packsExport: (key: string) => invoke<string | null>("packs_export", { key }),
  packsShowFile: (path: string) => invoke<void>("packs_show_file", { path }),
  packsPickOtpack: () => invoke<string | null>("packs_pick_otpack"),
  /** Windows folder picker; `null` if cancelled. `start`: a folder or file to start in. */
  pickFolder: (titleText: string, start?: string) =>
    invoke<string | null>("pick_folder", { titleText, start: start ?? null }),
  /** Windows file picker for a program (.exe); `null` if cancelled. */
  pickExe: (titleText: string, start?: string) =>
    invoke<string | null>("pick_exe", { titleText, start: start ?? null }),
  /** `exe`: the game's exe file name, for a Cheat Engine table that doesn't say. */
  packsPreview: (path: string, exe?: string) => invoke<PackPreview>("packs_preview", { path, exe: exe ?? null }),
  packsImport: (path: string, replace: boolean, exe?: string) =>
    invoke<ImportResult>("packs_import", { path, replace, exe: exe ?? null }),
  packsRemove: (key: string) => invoke<string>("packs_remove", { key }),
  githubStatus: () => invoke<GithubStatus>("github_status"),
  githubSignIn: () => invoke<DeviceCode>("github_sign_in"),
  /** Resolves with the account name once the player approved on GitHub. */
  githubSignInWait: () => invoke<string>("github_sign_in_wait"),
  githubSignInCancel: () => invoke<void>("github_sign_in_cancel"),
  githubSignOut: () => invoke<void>("github_sign_out"),
  githubOpenDevicePage: () => invoke<void>("github_open_device_page"),
  githubOpenPr: (url: string) => invoke<void>("github_open_pr", { url }),
  communityShare: (key: string, note: string, tableOk: boolean) =>
    invoke<SharedPack>("community_share", { key, note, tableOk }),
  communityList: (refresh: boolean) => invoke<CommunityList>("community_list", { refresh }),
  communityPreview: (id: string) => invoke<PackPreview>("community_preview", { id }),
  communityInstall: (id: string, replace: boolean) =>
    invoke<ImportResult>("community_install", { id, replace }),
  updateStatus: () => invoke<UpdateStatus>("update_status"),
  updateCheck: () => invoke<UpdateInfo | null>("update_check"),
  updateInstall: () => invoke<void>("update_install"),
  /** Opens one of OpenTrainer's own pages in the browser. */
  openLink: (which: "releases" | "latest" | "guide" | "contribute" | "node") => invoke<void>("open_link", { which }),
  trainerLoad: (key: string) => invoke<TrainerView>("trainer_load", { key }),
  trainerState: () => invoke<TrainerView | null>("trainer_state"),
  trainerUnload: () => invoke<void>("trainer_unload"),
  trainerToggle: (id: string, on: boolean) => invoke<TrainerView>("trainer_toggle", { id, on }),
  trainerSet: (id: string, value: string) => invoke<TrainerView>("trainer_set", { id, value }),
  trainerRelease: (id: string) => invoke<TrainerView>("trainer_release", { id }),
  trainerPress: (id: string) => invoke<TrainerView>("trainer_press", { id }),
  trainerRepair: () => invoke<RepairResult>("trainer_repair"),
  presetApply: (name: string) => invoke<TrainerView>("preset_apply", { name }),
  presetSave: (name: string) => invoke<TrainerView>("preset_save", { name }),
  presetDelete: (name: string) => invoke<TrainerView>("preset_delete", { name }),
  presetAuto: (name: string | null) => invoke<TrainerView>("preset_auto", { name }),
  cheatNote: (id: string, note: string) => invoke<TrainerView>("cheat_note", { id, note }),
  cheatFavourite: (id: string, on: boolean) => invoke<TrainerView>("cheat_favourite", { id, on }),
  /** `null` goes back to the pack's key, "" means none. */
  cheatKey: (id: string, key: string | null) => invoke<TrainerView>("cheat_key", { id, key }),
  cheatPad: (id: string, chord: string | null) => invoke<TrainerView>("cheat_pad", { id, chord }),

  libraryList: () => invoke<LibraryGame[]>("library_list"),
  /** The cover picture's bytes (empty if there is none). */
  libraryCover: (id: string) => invoke<unknown>("library_cover", { id }),
  libraryAdd: (exe: string) => invoke<void>("library_add", { exe }),
  libraryRemove: (id: string) => invoke<void>("library_remove", { id }),
  libraryOpenFolder: (id: string) => invoke<void>("library_open_folder", { id }),
  libraryCancel: () => invoke<void>("library_cancel"),
  libraryPlay: (id: string) => invoke<PlayResult>("library_play", { id }),

  overlaySet: (show: boolean) => invoke<boolean>("overlay_set", { show }),
  overlayVisible: () => invoke<boolean>("overlay_visible"),

  remoteStart: () => invoke<RemoteStatus>("remote_start"),
  remoteStop: () => invoke<void>("remote_stop"),
  remoteStatus: () => invoke<RemoteStatus | null>("remote_status"),
  remoteForget: () => invoke<RemoteStatus | null>("remote_forget"),

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

  aiStatus: () => invoke<AiStatus>("ai_status"),
  aiStart: (request: AiStartRequest) => invoke<void>("ai_start", { request }),
  aiAnswer: (id: string, text: string) => invoke<void>("ai_answer", { id, text }),
  /** A message for the helper while it works; it reads it after its current step. */
  aiNote: (text: string) => invoke<void>("ai_note", { text }),
  aiCancel: () => invoke<void>("ai_cancel"),
  aiSignIn: () => invoke<void>("ai_sign_in"),
  aiInstall: () => invoke<void>("ai_install"),
  aiUninstall: () => invoke<void>("ai_uninstall"),
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
