# Status

_Last updated: 2026-11-07 — v0.6.0_

## Done
- CI green on GitHub (private repo jackuri/opentrainer) through v0.6.0 (pushed and tagged).
- v0.1.0 skeleton: workspace, Tauri app boots, testbed game (x64/x86/shifted), CI, check + bump scripts.
- v0.2.0 core memory: process list, guarded attach/detach, typed read/write, module map, regions,
  anti-cheat guard with tests, desktop UI to attach and read/write.
- v0.3.0 scanner: multithreaded first/next scans, address list, 50 ms freeze loop, manual-tools UI.
- v0.4.0 packs: v1 schema, validator, `value` and `patch` ops, safe-patch checker, trainer runtime,
  build-hash check, hotkeys, pack store, `otpack` CLI, example packs, Trainer tab.
- v0.5.0 stability: checked code writes, crash-safe journal with recovery, pointer scanner,
  signature maker, pointer paths in the address list, `Try OpenTrainer.cmd` launcher (stops a
  leftover dev server, keeps errors on screen).
- v0.6.0 Saves I: save locator (Unity, Unreal, Steam Cloud, named folders), backups on game start /
  exit / attach / by hand with notes, content-deduplicated store with retention, compare (text lines
  and binary bytes), guarded restore (game closed, install-folder guard check, hash check, backup
  first), Steam Cloud warnings, Save backups view. Testbed `save` / `load`. All checks pass locally.
- Hand-tested by the user on v0.5.0: Trainer tab with the testbed pack (Infinite Health, +1000 gold)
  works.

## In progress
- Nothing. Stopped after v0.6.0 as planned.

## Next
- v0.7.0 Saves II (PLAN §6): codecs (JSON / zlib / GVAS / PlayerPrefs / SQLite), structured editor,
  hex fallback, checksum fixers, quick codes.
- Code caves: dropped from v0.5.0. They need a design review before they come back.
- Small follow-ups: hotkey rebinding, "Load pack from file…", auto-load a pack on attach, a
  "Find pointers" button on scanner results, "copy as pack JSON" for pointer paths, a folder picker
  dialog for save folders (needs the Tauri dialog plugin).

## Known issues
- The Save backups view, Pointers and Signatures tabs haven't been clicked through by hand yet
  (covered by frontend tests and live tests against the testbed).
- Unity PlayerPrefs saves live in the registry (`HKCU\Software\<company>\<product>`) and are not
  backed up yet (v0.7.0 codecs).
- Folders are added by pasting a path (no folder picker yet).
- Every attached program is added to Save backups (it can be removed there). Its saves are backed
  up at attach only if a save folder was found.
- Steam Cloud: OpenTrainer can't stop Steam from syncing; it warns and tells you to keep the local
  copy if Steam asks.
- Backups of files the game has locked are listed as "couldn't be read" and the rest is backed up.
- The user isn't a developer: test instructions should be double-click-first (see
  `Try OpenTrainer.cmd`), not terminal-first.
- Crash recovery covers code patches. Frozen values need no recovery (they just stop).
- If recovery is blocked by the guard (anti-cheat started meanwhile), the journal is kept and tried
  again next start; it is deleted once the game closes.
- Pointer scans hold a map of every pointer in the game (16 bytes each, limit 48 million entries,
  about 770 MB). Very large games may hit the limit; lower the levels or close other programs.
- A registered hotkey is taken from every other app while a pack is loaded (`RegisterHotKey`).
- The hotkey test needs an interactive desktop, so it is `#[ignore]`d and not run in CI.
- Example packs list no build hashes (the testbed's hash changes with the compiler).
- Guard false positive: a game within three folders of a folder holding an anti-cheat directory is
  blocked (e.g. under `%LOCALAPPDATA%` on a PC with `%LOCALAPPDATA%\BattlEye`). Intended to err on the
  side of blocking. The same applies to restoring saves.
- Scan results live in memory; unknown-value scans refuse above 4 GiB; the scanner shows 200 results.
- The guard blocks every attach while a kernel anti-cheat (Vanguard, FACEIT, ACE…) is running. Intended.
- Some guard signature names need confirming on real installs (a wrong name can only miss, never
  falsely block). Driver file-name detection is best effort without admin rights.
- App icon is a placeholder.
