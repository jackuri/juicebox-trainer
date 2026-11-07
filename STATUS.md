# Status

_Last updated: 2026-11-07 — v0.8.0_

## Done
- CI green on GitHub (private repo jackuri/opentrainer) through v0.8.0 (pushed and tagged).
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
- v0.7.0 Saves II: save editor for JSON, INI, Unreal GVAS (up to UE 5.3) and Unity PlayerPrefs
  (registry, also backed up / compared / restored), gzip / zlib / Base64 wrappers and CRC32 / MD5 /
  SHA-1 / SHA-256 checksums recomputed on save, hex view with number inspector, quick codes from
  packs (validated placeholders), guarded saving (game closed, guard, backup first). All checks pass.
- v0.8.0 AI I: guarded `ot-mcp` tool server (20 tools, logged, every change revertible, guard on
  attach, saves backed up), AI helper sidecar (Claude Agent SDK, no built-in tools, dontAsk
  permissions, isolated settings, budget cap), "Make a trainer (AI)" view with step-by-step
  Done / Yes / No prompts. Live test drives a whole agent session against the testbed. PLAN §8
  decisions recorded.
- Hand-tested by the user on v0.5.0: Trainer tab with the testbed pack (Infinite Health, +1000 gold)
  works.

## In progress
- Nothing. Stopped after v0.8.0 as planned.

## Next
- v0.9.0 AI II (PLAN §6): REA + `um` integration, engine fast paths (Unity Mono, IL2CPP, Unreal),
  self-healing packs, save diff labelling. Also: `find_what_writes` (hardware breakpoints) so the
  agent can make code patches, and local models (Ollama / LM Studio).
- Pack sharing via a GitHub-repo index (decided; build with `.otpack` import/export).
- SQLite saves: moved out of v0.7.0 (needs the large rusqlite library; the user chose to wait).
- Code caves: dropped from v0.5.0. They need a design review before they come back.
- Small follow-ups: hotkey rebinding, "Load pack from file…", auto-load a pack on attach, a
  "Find pointers" button on scanner results, "copy as pack JSON" for pointer paths, a folder picker
  dialog for save folders (needs the Tauri dialog plugin).

## Known issues
- Real AI runs (user approved): the first stopped at "Not logged in" (the Claude desktop app's
  sign-in isn't shared with the Agent SDK's engine), so the sign-in check and button were added.
  After the user signed in, a full session on the testbed (scripted player) found gold, used two
  restarts to pick a stable pointer path, tested and saved a working "Lots of gold" pack, for about
  $0.36. The pack works on a fresh testbed. The run showed value cheats weren't undone by
  revert_all; fixed (cheat_set keeps the original value).
- The AI helper needs Node.js and, for the subscription, a signed-in Claude Code on the PC. API keys
  are read from the `ANTHROPIC_API_KEY` environment variable; the app doesn't store keys.
- Without `find_what_writes`, AI cheats are value cheats via pointer paths (and patches only where
  the agent can find a signature another way); pointer scans can return many paths, so the helper
  may ask the player to restart the game once.
- The Agent SDK's Windows engine is about 330 MB in `node_modules` (and in CI installs).
- The Save backups view (incl. the editor), Pointers and Signatures tabs haven't been clicked
  through by hand yet (covered by frontend tests and live tests against the testbed).
- Save editor limits: Unreal saves from UE 5.4+ open as hex; Unreal text values, engine structs,
  maps and sets are read-only; XML / YAML saves open as hex; files over 64 MB aren't opened.
- PlayerPrefs floats are assumed to be REG_QWORD doubles (Unity's Windows format); a game that
  stores numbers differently shows them as raw whole numbers.
- Wrapper detection only handles a checksum over the whole rest of the file; seeded or partial
  checksums, and encryption, aren't detected (encrypted saves open as hex and are not decrypted).
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
