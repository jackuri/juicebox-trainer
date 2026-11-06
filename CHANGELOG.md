# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [0.3.0] - 2026-11-06

### Added
- **Scanner** (`ot_core::scan`): first scans (exact, between, unknown initial value) and next scans
  (exact, between, changed, unchanged, increased, decreased) for every value type. Runs on all cores
  in 16 MiB blocks. Each block is kept dense (snapshot + bitmap) or sparse (offsets + values),
  whichever is smaller. Options: writable-only, aligned/unaligned, float tolerance. Progress and
  cancel via `ScanControl`; a cancelled or failed next scan keeps the previous results. Unknown
  scans refuse above 4 GiB of memory. Scans read through `AttachedProcess`, so the guard applies.
- **Freeze loop** (`ot_core::Freezer`): rewrites frozen values every 50 ms on a background thread.
  Stops for good if the guard blocks or the game exits, and keeps the reason. Each freeze is logged
  once with its address and value; the repeated writes aren't logged one by one.
- Desktop: Scanner tab (type, scan kind, value/range, options, progress bar with cancel, first 200
  results with live and previous values, add to list); address list with live values (polled every
  500 ms), freeze toggles, inline value editing (a frozen value re-freezes at the new value), labels,
  manual add by `0x…` or `module.exe+0x…`; tabs for Scanner / Read & write / Modules.
- Tests: 9 scanner unit tests; 11 new integration tests on live testbeds (exact scan x64, x86 and
  unaligned; unknown → decreased/increased/unchanged/changed; float range; cancel keeps results;
  type mismatch; scans blocked after the guard trips; freezer holds/unfreezes; freezer stops on guard
  block and on game exit); 5 new frontend tests.

### Changed
- The session is now shared (`Arc<AttachedProcess>`) between the UI and the freeze thread.
- When several threads hit the 5-second guard recheck at once, only one of them runs it.
- Integration-test helpers moved to `crates/ot-core/tests/common/`.

### Fixed
- The app now notices a game that exits while attached even if nothing is reading its memory (it
  used to keep showing "Attached").

### Removed
- The unused `read_bytes` desktop command.

## [0.2.0] - 2026-11-06

### Added
- `ot-core` process list with image path and architecture (WOW64-aware via `IsWow64Process2`).
- `AttachedProcess`: guarded attach/detach, typed read/write (`i8`…`u64`, `f32`, `f64`, pointer-sized
  `ptr`), raw byte read/write, pointer reads, committed-region map (`VirtualQueryEx`), liveness check.
  Every write is logged with `tracing`.
- Module map via ToolHelp; 64-bit WOW64 layer modules are filtered out for x86 targets.
- Address expressions: `0x1234ABCD`, bare hex, `module.exe+0x1F0`.
- **Anti-cheat guard** (`ot_core::guard`) with no off switch: blocks on known anti-cheat processes,
  services, kernel drivers, modules in the target, install-folder markers (exe folder plus 3 parents),
  the built-in online-only list, or any check that fails to run. The signature list is compiled in.
  Attached sessions re-run the guard every 5 s and stay blocked once it trips.
- Desktop UI: process picker (search, hide Windows processes), guard verdict, attach/detach, read/write
  panel with activity log, module table.
- Tests: 34 `ot-core` unit tests (guard decisions, signature list shape, a source scan that fails if the
  guard gains an env var or feature switch), 11 integration tests against live x64 and x86 testbeds
  (read/write confirmed through the testbed's side channel, pointer chain, fake anti-cheat process,
  BattlEye/EAC folder markers, online-only exe name, anti-cheat appearing after attach), 8 frontend tests.

## [0.1.0] - 2026-11-06

### Added
- Monorepo skeleton: Rust workspace (`ot-core`, `ot-packs`, `ot-saves`, `ot-mcp`, `testbed-native`,
  desktop shell) and npm workspace for `apps/desktop`.
- Tauri 2 desktop app (React 19, TypeScript, Tailwind 4, Vite) that boots and shows its version via an
  `app_info` command.
- `testbed/native`: a fake console game with health, ammo, gold and speed behind a heap pointer chain
  (static root → World → Player → Stats), a JSON stdin/stdout side channel for tests, x86 builds, and a
  `shifted` feature that changes offsets.
- CI on `windows-latest`: rustfmt, clippy (`-D warnings`, plus x86 testbed), cargo tests, ESLint,
  TypeScript typecheck, Vitest.
- `scripts/check.ps1` (runs every CI check locally) and `scripts/bump-version.mjs` (bumps all manifests).
- MIT licence, README, STATUS.
