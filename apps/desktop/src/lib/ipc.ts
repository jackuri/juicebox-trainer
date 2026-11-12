import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// Types mirror the Rust structs in jb-core and src-tauri/src/commands.rs.

export type Arch = "x86" | "x64" | "arm64" | "unknown";

export interface AppInfo {
  version: string;
  coreVersion: string;
  /** This start moved OpenTrainer's packs, saves and settings over (v1.23.0). */
  movedFromOpenTrainer?: boolean;
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
  /** Anti-cheat installed with the game but not running (allowed games only). */
  notes?: Finding[];
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

/** A memory region (jb_core::Region). */
export interface MemoryRegion {
  base: number;
  size: number;
  protect: number;
  readable: boolean;
  writable: boolean;
  executable: boolean;
  image: boolean;
}

/** Bytes for the memory viewer (commands.rs memory_view). */
export interface MemoryView {
  address: number;
  bytes: number[];
  readable: boolean[];
  region: MemoryRegion | null;
  module: string | null;
}

/** What a memory viewer edit did. */
export type ByteEdit = "unchanged" | "data" | "code";

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

// ----- find a value (mirror commands.rs finder_* / my_cheat_save) -----

export type FinderStep =
  | { kind: "number"; value: string }
  | { kind: "increased" }
  | { kind: "decreased" }
  | { kind: "unchanged" }
  | { kind: "changed" };

export interface FinderSummary {
  count: number;
  rounds: number;
}

export interface FinderHit {
  address: number;
  ty: ValueType;
  /** Value at the last search. */
  previous: string;
  /** Value now, if readable. */
  current: string | null;
}

/** A found value to save as a cheat (a module address, then pointer offsets). */
export interface NewCheat {
  name: string;
  ty: ValueType;
  value: string;
  /** Keep it at `value` (a switch) rather than set it once (a button). */
  keep: boolean;
  module: string;
  moduleOffset: number;
  offsets: number[];
}

export interface MyCheatSaved {
  key: string;
  packName: string;
  hotkey: string | null;
  /** Now loaded in the Trainer tab. */
  loaded: boolean;
}

// ----- packs + trainer (mirror jb-packs and commands.rs) -----

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

/** A game players asked cheats for (community "Most wanted", v1.25.0). */
export interface Wanted {
  game: string;
  exe: string | null;
  votes: number;
  number: number;
  asked: string | null;
}

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
  /** Who made the cheats and where they were published, if the pack says. */
  credit?: CheatSource | null;
  /** The credit names the maker and a page on a known table site: the link can be shared. */
  tableLink?: boolean;
}

/** The player's signed agreement (v1.18.0). */
export interface AgreementStatus {
  accepted: boolean;
  version: number;
  signed: { version: number; name: string; date: string } | null;
}

/** A game that runs in a web page (v1.18.0). */
export interface WebGame {
  id: string;
  name: string;
  url: string;
  added: string;
}

export type NumType = "u8" | "i16" | "i32" | "f32" | "f64";
/** Where a number lives in a web game: a page memory, or a JavaScript value by its path. */
export type WebTarget =
  | { k: "m"; mem: number; label: string; size: number; off: number; t: NumType }
  | { k: "j"; path: string[] };

export interface WebCheat {
  name: string;
  target: WebTarget;
  value?: number | null;
  /** A key that works while the game window is in front (v1.19.0). */
  key?: string | null;
  /** What the key does: lock / unlock (the default), or set the value once. */
  keyDoes?: "lock" | "set" | null;
}

/** A web game's cheats as a file or on the community list (v1.19.0). */
export interface WebPack {
  format: number;
  id: string;
  game: string;
  url: string;
  by: string;
  notes?: string | null;
  added?: string | null;
  cheats: WebCheat[];
}

export interface WebInstalled {
  game: WebGame;
  added: number;
  skipped: number;
}

/** What the helper in the game page has locked, and which list of keys it uses. */
export interface WebState {
  locks: string[];
  keysTag: string;
}

/** One cheat's key, for the helper in the game page. */
export interface WebKey {
  id: string;
  name: string;
  key: string;
  does: "lock" | "set";
  target: WebTarget;
  value: number | null;
}

export interface WebHit {
  id: number;
  target: WebTarget;
  where: string;
  /** Missing when the value is gone right now (a swapped weapon, a new level). */
  value?: number;
}

export interface WebSearch {
  count: number;
  tooMany: boolean;
  results: WebHit[];
}

export interface WebInfo {
  url: string;
  title: string;
  memories: { index: number; label: string; bytes: number }[];
  locks: number;
}

/** A Cheat Engine table on its maker's page, from the community list (v1.16.0). */
export interface TableLink {
  id: string;
  game: string;
  exe: string | null;
  title: string;
  by: string;
  url: string;
  cheats: number | null;
  added: string | null;
  /** The table's terms (v1.27.0). */
  licence?: string | null;
  /** The table itself is on the list (its maker gave it an open licence or permission). */
  file?: string | null;
  sha256?: string | null;
}

/** Search places for cheat tables (v1.27.0: more than FearLess). */
export type TableSite = "fearless" | "cheatengine" | "opencheattables" | "github" | "reddit" | "web";

/** Terms under which a table itself may go on the community list (as `community::HOSTING_LICENCES`). */
export const HOSTING_LICENCES = [
  "CC-BY-4.0",
  "CC0-1.0",
  "CC-BY-SA-4.0",
  "MIT",
  "Apache-2.0",
  "GPL-3.0-or-later",
  "Shared with permission; all rights kept by the maker",
] as const;

/** A table from the community list, saved on this PC (v1.27.0). */
export interface GotTable {
  path: string;
  exe: string | null;
  by: string;
  url: string;
  licence: string | null;
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

// ----- presets, own keys (jb-packs settings) -----

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

/** Whether the attached game can get instant engine cheats (v1.26.0). */
export interface EngineSupport {
  engine: string | null;
  supported: boolean;
  why: string | null;
  /** Its instant cheats pack, if made before. */
  packKey: string | null;
}

export interface EngineCheatsMade {
  key: string;
  packName: string;
  cheats: string[];
  notFound: string[];
  loaded: boolean;
}

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
  /** Anti-cheat that comes with the game; works only while it isn't running. */
  antiCheat: string | null;
  /** Hidden from the Games tab by the player. */
  hidden: boolean;
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
  /** SHA-256 of the pack text shown; installing checks it is unchanged. */
  checksum?: string;
  /** Converted from a Cheat Engine table: the maker's credit can be added. */
  table?: boolean;
  /** Who made the cheats and where they were published, if the pack says. */
  source?: CheatSource | null;
}

/** Credit for cheats made by someone else (a Cheat Engine table's maker). */
export interface CheatSource {
  by?: string;
  url?: string;
  file?: string;
  /** The maker's terms (v1.17.0); none means MIT on the community list. */
  licence?: string;
  /** Where players can support the maker. */
  support?: string;
  /** The person who shared it says they made the cheats (checked by the list's maintainers). */
  fromMaker?: boolean;
}

/** Terms a maker can pick when sharing. */
export const LICENCE_MIT = "MIT";
export const LICENCE_PERMISSION = "Shared with permission; all rights kept by the maker";

/** What the player picks when sharing a pack (v1.17.0). */
export interface ShareTerms {
  fromMaker: boolean;
  licence?: string;
  support?: string;
}

/** The table maker's name and page, typed when converting a table. */
export interface Credit {
  by?: string;
  url?: string;
  /** The table's terms (set for a table from the community list). */
  licence?: string;
}

/** A Cheat Engine table in the player's Downloads folder (last 30 days). */
export interface RecentTable {
  path: string;
  file: string;
  /** Last changed, milliseconds since 1970. */
  modified: number;
  /** The game, from the table's file name. */
  game: string;
  /** The game's exe, when the table says. */
  exe: string | null;
  /** Cheats JuiceBox Trainer can use from it. */
  cheats: number;
  /** Why it can't be read, if it can't. */
  error: string | null;
}

/** GitHub sign-in for sharing packs with the community list. */
export interface GithubStatus {
  /** This version knows JuiceBox Trainer's GitHub app. */
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
  /** Who made the cheats, their terms and support page (v1.17.0). */
  source?: CheatSource | null;
  sha256: string;
  size: number;
  installed: "none" | "installed" | "update" | "other";
}

export interface CommunityList {
  url: string;
  contributeUrl: string;
  updated: string | null;
  packs: CommunityPack[];
  /** Links to Cheat Engine tables on their makers' pages (empty from older lists). */
  tables?: TableLink[];
  /** Cheats for web games (v1.19.0; empty from older lists). */
  web?: WebPack[];
  skipped: string[];
}

/** Files dropped on the window (their paths). */
/** Files JuiceBox Trainer opens as packs: `.juice` (`.otpack` from OpenTrainer), a plain pack `.json`, or a table. */
export function isPackFile(path: string): boolean {
  return /\.(juice|otpack|json|ct)$/i.test(path.trim());
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

/** A .juice pack opened from Explorer while JuiceBox Trainer runs. */
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

/** Code changes undone at startup after JuiceBox Trainer closed unexpectedly. */
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

// ----- save backups (jb-saves, src-tauri/src/saves.rs) -----

/** A Steam account on this PC (jb_saves::accounts::SteamAccount). */
export interface SteamAccount {
  /** SteamID64 as text (too big for a JS number). */
  id64: string;
  id32: number;
  name: string | null;
  login: string | null;
  onThisPc: boolean;
}

export interface AccountFolder {
  path: string;
  listed: string;
}

export interface AccountSaves {
  account: SteamAccount;
  folders: AccountFolder[];
  files: number;
  bytes: number;
}

export interface AccountsView {
  withSaves: AccountSaves[];
  accounts: SteamAccount[];
}

export interface PlannedCopy {
  from: string;
  to: string;
  size: number;
  replaces: boolean;
  ids: number;
}

export interface CopyPlan {
  from: SteamAccount;
  to: SteamAccount;
  files: PlannedCopy[];
  newFolders: string[];
  cloudWarning: string | null;
}

export interface CopyReport {
  backup: string;
  copied: number;
  replaced: number;
  ids: number;
  addFolders: string[];
  cloudWarning: string | null;
}

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

export type BackupReason =
  | "launch"
  | "exit"
  | "manual"
  | "beforeRestore"
  | "beforeEdit"
  | "beforeAccountCopy";

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
  /** "trainer" (default), "explainSave" (needs exe and backup) or "webTrainer" (the open web game). */
  task?: "trainer" | "explainSave" | "webTrainer";
  /** For "webTrainer": the web game's id (it must be the one open in the game window). */
  webGame?: string;
  /** false: the helper may not look at the game window (v1.21.0). */
  pictures?: boolean;
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

/** Calls `cb` with a web game's id when the AI helper saved cheats for it. */
export function onWebCheatsChanged(cb: (id: string) => void): () => void {
  let unlisten: (() => void) | null = null;
  let stopped = false;
  try {
    listen<string>("web-cheats-changed", (e) => cb(e.payload)).then(
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
  /** When the game closed: what happened and why (v1.22.0). */
  closed?: Problem | null;
}

/** A problem worth telling the player about (src-tauri/src/problems.rs, v1.22.0). */
export interface Problem {
  id: number;
  /** `gameClosed`, `webPage`, `appScreen` or `appCrash`. */
  kind: string;
  game: string | null;
  title: string;
  why: string;
  advice: string[];
  details: string[];
  crashed: boolean;
  seen: boolean;
}

/** A problem was recorded or updated (a game closed, a page crashed…). */
export function onProblem(cb: (p: Problem) => void): () => void {
  return on<Problem>("problem", cb);
}

/** The web game page crashed or froze. */
export function onWebGameProblem(cb: (p: Problem) => void): () => void {
  return on<Problem>("web-game-problem", cb);
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
  memoryView: (address: string, len: number) => invoke<MemoryView>("memory_view", { address, len }),
  memoryEdit: (address: string, hex: string) => invoke<ByteEdit>("memory_edit", { address, hex }),

  finderStart: (number: string | null) => invoke<FinderSummary>("finder_start", { number }),
  finderNext: (step: FinderStep) => invoke<FinderSummary>("finder_next", { step }),
  finderResults: (limit: number) => invoke<FinderHit[]>("finder_results", { limit }),
  finderReset: () => invoke<void>("finder_reset"),
  myCheatSave: (gameName: string, cheat: NewCheat, expect: number) =>
    invoke<MyCheatSaved>("my_cheat_save", { gameName, cheat, expect }),
  /** Can the attached game get instant engine cheats? */
  engineCheatsSupport: () => invoke<EngineSupport>("engine_cheats_support"),
  /** Reads the game's engine and saves (and loads) its instant cheats. Can take minutes. */
  engineCheatsMake: (gameName: string) => invoke<EngineCheatsMade>("engine_cheats_make", { gameName }),

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
  packsImport: (path: string, replace: boolean, exe?: string, checksum?: string, credit?: Credit) =>
    invoke<ImportResult>("packs_import", { path, replace, exe: exe ?? null, checksum: checksum ?? null, credit: credit ?? null }),
  agreementStatus: () => invoke<AgreementStatus>("agreement_status"),
  agreementSign: (name: string) => invoke<AgreementStatus>("agreement_sign", { name }),
  webGames: () => invoke<WebGame[]>("web_games"),
  webGameAdd: (name: string, url: string) => invoke<WebGame>("web_game_add", { name, url }),
  webGameRemove: (id: string) => invoke<void>("web_game_remove", { id }),
  webCheats: (id: string) => invoke<WebCheat[]>("web_cheats", { id }),
  webCheatsSave: (id: string, cheats: WebCheat[]) => invoke<void>("web_cheats_save", { id, cheats }),
  webOpen: (id: string) => invoke<WebGame>("web_open", { id }),
  webCurrent: () => invoke<WebGame | null>("web_current"),
  webClose: () => invoke<void>("web_close"),
  /** Reloads the web game page (after it crashed or froze). */
  webReload: () => invoke<void>("web_reload"),
  /** Sets the taskbar / window icon to a built-in juice box (v1.24.0). */
  appIconSet: (name: string) => invoke<void>("app_icon_set", { name }),
  /** Recent problems, newest first. */
  problemsList: () => invoke<Problem[]>("problems_list"),
  /** Marks every problem as looked at. */
  problemsSeen: () => invoke<void>("problems_seen"),
  /** Forgets every problem. */
  problemsClear: () => invoke<void>("problems_clear"),
  webInfo: () => invoke<WebInfo>("web_call", { method: "info", args: null }),
  webFind: (value: number, kind: "any" | "whole" | "decimal") => invoke<WebSearch>("web_call", { method: "find", args: { value, kind } }),
  webNext: (how: "equals" | "changed" | "unchanged" | "increased" | "decreased", value?: number) =>
    invoke<WebSearch>("web_call", { method: "next", args: { how, value: value ?? null } }),
  webResults: () => invoke<WebSearch>("web_call", { method: "results", args: null }),
  webRead: (targets: WebTarget[]) => invoke<{ value?: number; error?: string }[]>("web_call", { method: "read", args: { targets } }),
  webSet: (target: WebTarget, value: number) => invoke<{ value: number }>("web_call", { method: "set", args: { target, value } }),
  webLock: (id: string, target: WebTarget, value: number) => invoke<unknown>("web_call", { method: "lock", args: { id, target, value } }),
  webUnlock: (id?: string) => invoke<unknown>("web_call", { method: "unlock", args: { id: id ?? null } }),
  webKeys: (list: WebKey[], tag: string) => invoke<unknown>("web_call", { method: "keys", args: { list, tag } }),
  webState: () => invoke<WebState>("web_call", { method: "state", args: null }),
  webExport: (id: string, by: string, notes: string | null) => invoke<string | null>("web_export", { id, by, notes }),
  webPickFile: () => invoke<string | null>("web_pick_file"),
  webReadFile: (path: string) => invoke<WebPack>("web_read_file", { path }),
  webInstall: (pack: WebPack) => invoke<WebInstalled>("web_install", { pack }),
  communityShareWeb: (id: string, by: string, notes: string | null) => invoke<SharedPack>("community_share_web", { id, by, notes }),
  /** Opens a search for the game's Cheat Engine tables in the browser. */
  tablesSearch: (site: TableSite, game: string) => invoke<void>("tables_search", { site, game }),
  /** Cheat Engine tables downloaded in the last 30 days, newest first. */
  tablesRecent: () => invoke<RecentTable[]>("tables_recent"),
  packsRemove: (key: string) => invoke<string>("packs_remove", { key }),
  githubStatus: () => invoke<GithubStatus>("github_status"),
  githubSignIn: () => invoke<DeviceCode>("github_sign_in"),
  /** Resolves with the account name once the player approved on GitHub. */
  githubSignInWait: () => invoke<string>("github_sign_in_wait"),
  githubSignInCancel: () => invoke<void>("github_sign_in_cancel"),
  githubSignOut: () => invoke<void>("github_sign_out"),
  githubOpenDevicePage: () => invoke<void>("github_open_device_page"),
  githubOpenPr: (url: string) => invoke<void>("github_open_pr", { url }),
  /** `permission`: for a pack from someone else, the player made it or its maker allows sharing it. */
  communityShare: (key: string, note: string, permission: boolean, terms?: ShareTerms) =>
    invoke<SharedPack>("community_share", { key, note, permission, terms: terms ?? null }),
  /** Opens a community pack maker's page or support page (only what the list gives). */
  communityOpenCredit: (id: string, which: "page" | "support") => invoke<void>("community_open_credit", { id, which }),
  communityList: (refresh: boolean) => invoke<CommunityList>("community_list", { refresh }),
  /** Games players asked cheats for, most votes first (v1.25.0). */
  communityWanted: (refresh: boolean) => invoke<Wanted[]>("community_wanted", { refresh }),
  /** Opens the filled-in request form for a game in the browser. */
  communityAsk: (game: string, exe: string | null) => invoke<void>("community_ask", { game, exe }),
  /** Opens a request's page (to vote). */
  communityOpenRequest: (number: number) => invoke<void>("community_open_request", { number }),
  /** Adds the link to the table a pack came from to the community list (a pull request). */
  /** Shares a table's link; with `file` and `licence`, the table itself too (v1.27.0). */
  communityShareTable: (key: string, file?: string, licence?: string) =>
    invoke<SharedPack>("community_share_table", { key, file: file ?? null, licence: licence ?? null }),
  /** Downloads a table the community list carries, to turn it into cheats. */
  communityGetTable: (id: string) => invoke<GotTable>("community_get_table", { id }),
  /** Lets the player pick a .CT file. */
  pickTable: () => invoke<string | null>("pick_table"),
  /** Opens a table's page from the community list (only pages the list has). */
  communityOpenTable: (url: string) => invoke<void>("community_open_table", { url }),
  communityPreview: (id: string) => invoke<PackPreview>("community_preview", { id }),
  communityInstall: (id: string, replace: boolean) =>
    invoke<ImportResult>("community_install", { id, replace }),
  updateStatus: () => invoke<UpdateStatus>("update_status"),
  updateCheck: () => invoke<UpdateInfo | null>("update_check"),
  updateInstall: () => invoke<void>("update_install"),
  /** Opens one of JuiceBox Trainer's own pages in the browser. */
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
  librarySetHidden: (id: string, hidden: boolean) => invoke<void>("library_set_hidden", { id, hidden }),
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
  savesAccounts: (id: string) => invoke<AccountsView>("saves_accounts", { id }),
  savesAccountPlan: (id: string, from: string, to: string) =>
    invoke<CopyPlan>("saves_account_plan", { id, from, to }),
  savesAccountCopy: (id: string, from: string, to: string) =>
    invoke<CopyReport>("saves_account_copy", { id, from, to }),
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
