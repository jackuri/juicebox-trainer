import type { Finding, ProcessInfo } from "./ipc";

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
