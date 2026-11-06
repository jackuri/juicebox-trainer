import { invoke } from "@tauri-apps/api/core";

/** Mirrors `commands::AppInfo` in src-tauri. */
export interface AppInfo {
  version: string;
  coreVersion: string;
}

export function appInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("app_info");
}
