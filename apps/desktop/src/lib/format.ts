import type { BackupReason, Finding, ProcessInfo } from "./ipc";

/** `0x`-prefixed uppercase hex. Addresses stay below 2^53, so numbers are exact. */
export function hex(n: number): string {
  return `0x${n.toString(16).toUpperCase()}`;
}

/** Processes living in the Windows folder or with no readable path. */
export function isSystemProcess(p: ProcessInfo): boolean {
  if (!p.exePath) return true;
  return /^[a-z]:\\windows\\/i.test(p.exePath);
}

export function filterProcesses(
  list: ProcessInfo[],
  query: string,
  hideSystem: boolean,
): ProcessInfo[] {
  const q = query.trim().toLowerCase();
  return list.filter((p) => {
    if (hideSystem && isSystemProcess(p)) return false;
    if (!q) return true;
    return p.name.toLowerCase().includes(q) || String(p.pid) === q;
  });
}

const KIND_TEXT: Record<Finding["kind"], string> = {
  process: "process running",
  service: "service running",
  driver: "driver loaded",
  module: "module loaded in the game",
  installMarker: "found in the game folder",
  onlineOnly: "online-only game",
  unverifiable: "check could not run",
};

export function describeFinding(f: Finding): string {
  return `${f.product}: ${KIND_TEXT[f.kind]} (${f.detail})`;
}

/** "512 B", "12.3 KB", "4.0 MB". */
export function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(1)} ${units[i]}`;
}

/** "just now", "5 min ago", "3 h ago", "2 days ago". */
export function timeAgo(ms: number, now: number = Date.now()): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return "just now";
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 48) return `${h} h ago`;
  return `${Math.round(h / 24)} days ago`;
}

const REASON_TEXT: Record<BackupReason, string> = {
  launch: "Game started",
  exit: "Game closed",
  manual: "Made by you",
  beforeRestore: "Before a restore",
  beforeEdit: "Before an edit",
};

export function backupReason(r: BackupReason): string {
  return REASON_TEXT[r];
}

/** Little-endian numbers at the start of `bytes`, for the hex inspector. */
export function inspect(bytes: number[]): { label: string; value: string }[] {
  const buf = new Uint8Array(8);
  bytes.slice(0, 8).forEach((b, i) => (buf[i] = b));
  const v = new DataView(buf.buffer);
  const n = bytes.length;
  const out: { label: string; value: string }[] = [];
  if (n >= 1) out.push({ label: "8-bit", value: `${v.getUint8(0)} / ${v.getInt8(0)}` });
  if (n >= 2) out.push({ label: "16-bit", value: `${v.getUint16(0, true)} / ${v.getInt16(0, true)}` });
  if (n >= 4) {
    out.push({ label: "32-bit", value: `${v.getUint32(0, true)} / ${v.getInt32(0, true)}` });
    out.push({ label: "float", value: String(v.getFloat32(0, true)) });
  }
  if (n >= 8) {
    out.push({ label: "64-bit", value: v.getBigInt64(0, true).toString() });
    out.push({ label: "double", value: String(v.getFloat64(0, true)) });
  }
  return out;
}

/** The bytes as Tauri hands them over: an ArrayBuffer, a typed array or (often) a plain number list. */
export function coverBytes(data: unknown): Uint8Array<ArrayBuffer> | null {
  let bytes: Uint8Array<ArrayBuffer> | null = null;
  if (data instanceof ArrayBuffer) bytes = new Uint8Array(data);
  else if (ArrayBuffer.isView(data)) bytes = new Uint8Array(new Uint8Array(data.buffer, data.byteOffset, data.byteLength));
  else if (Array.isArray(data) && data.every((n) => typeof n === "number")) bytes = Uint8Array.from(data as number[]);
  return bytes && bytes.byteLength > 0 ? bytes : null;
}
