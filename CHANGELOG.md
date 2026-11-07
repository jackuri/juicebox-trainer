# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [0.9.3] - 2026-11-07

### Added
- **Unreal object reader** (`ot_core::unreal`, AI tool `unreal_find`) for UE 4.23+ and UE5 games
  while they run. It reads the engine's name list and its classes, then lists the game's classes and
  properties by name (e.g. "Health") with their types and offsets, and reads the live objects of a
  class with their values and addresses. Object layout (where a class keeps its parent and
  properties) is worked out from well-known engine classes instead of fixed offsets.
- **Slower search**: when a game's object list can't be read directly, the reader finds classes and
  objects by searching the game's memory, like the value scanner does. It doesn't decode or get
  around anything. The first search can take several minutes on a big game; the result is kept for
  the rest of the session, so later searches are quick.
- Tested on a real single-player UE 5.7 game (played solo, read-only): 12,592 classes and structs,
  the player character's `Health` and `MaxHealth` found with the right offsets.

### Changed
- Unreal tips in `game_recon` point the AI to `unreal_find`.
- `ot-core` is built optimized in development builds and tests (memory scans walk gigabytes).

## [0.9.2] - 2026-11-07

### Added
- **Unity IL2CPP names** (`ot_core::il2cpp`, in `game_classes`): reads class and field names from
  `global-metadata.dat`. The layout differs between Unity versions, so the reader works out where
  each class keeps its field count and first field by checking which positions add up to the file's
  totals and cover every field exactly once, instead of trusting one fixed layout. Checked on a real
  format-39 (Unity 6) file: 37,181 classes and 164,280 fields. Types live in GameAssembly.dll and
  show as `?`. Protected or encrypted metadata is refused with a clear message, never decrypted.
- **Unreal engine version** from the game's exe (`++UE4+Release-4.27` style text), with a tip for
  UE5 (positions are 64-bit doubles). Checked on real UE 4.19 and 5.7 games.
- `game_classes` says when a game's code includes anti-cheat parts (so it's probably online).

### Changed
- Name search hides compiler-made helper classes, shows C# property storage
  (`<Health>k__BackingField`) as `Health`, and lists field-name matches before class-name matches.
- Checked the Unity Mono reader on a real Unity game (643 classes, types read correctly).
- The roadmap moves the Unreal object reader to 0.9.3 (it needs a running game to test).

## [0.9.1] - 2026-11-07

### Added
- **Unity (Mono) and .NET game names** (`ot_core::dotnet`, AI tool `game_classes`): reads the class
  and field names and types from the game's own assemblies (`Assembly-CSharp.dll`, or an
  XNA / FNA / MonoGame game's exe and DLLs) by parsing their metadata tables. No code is decompiled,
  loaded or run, and nothing is saved. The helper uses it to learn, for example, that health is a
  `float` before it scans.
- **Free AI on your own PC**: a third AI option, "A free AI running on this PC", for Ollama
  (`http://localhost:11434`) or LM Studio (`http://localhost:1234`) with the model you choose. It uses
  their Claude-compatible Messages API. Expect it to be slower and to find cheats less often.
- **Optional code-analysis tool** (Advanced, in the AI view): an MCP server you install yourself (for
  example REA) for reading game files. The helper may use its tools only to understand code it found;
  saves explanations never get it. OpenTrainer doesn't ship or require it.
- The AI view remembers its settings on this PC.

### Changed
- The roadmap moves the IL2CPP and Unreal readers to 0.9.2.

## [0.9.0] - 2026-11-07

### Added
- **Find what writes to an address** (`ot_core::writes`): OpenTrainer attaches as a debugger (visibly;
  it doesn't hide), sets a hardware write breakpoint on the value in every thread, and lists the
  instructions that wrote to it while the player played. Nothing in the game is changed; the
  breakpoints are cleared and the debugger leaves without closing the game. x64 and 32-bit games.
- **Code-patch cheats from the AI helper**: new tools `watch_writes_start` / `watch_writes_stop`
  (with a ready-made patch idea: the instruction's bytes replaced by NOPs). The helper now prefers
  this for "infinite" cheats, which needs no game restarts.
- **Pack repair after game updates** (`ot_packs::heal`, Trainer tab **Try to repair**, AI tool
  `pack_heal`): cheats whose signature stopped matching are found again by the closest match in the
  new code (only a single clear winner above 60% is accepted), get a fresh unique signature, and the
  pack is saved as your copy with a bumped version and the new build's hash (old file kept as
  `pack.json.bak`).
- **Engine detection** (`ot_core::engine`, AI tool `game_recon`): Unity (Mono / IL2CPP, with the
  Unity version), Unreal, Godot, GameMaker, Ren'Py, RPG Maker MV/MZ and XP/VX, XNA/FNA/MonoGame, with
  tips for finding cheats in each. Adds `um scan --json` output when the universal-modder `um`
  command is installed (optional).
- **Explain with AI** on a save comparison (backup vs now): the helper reads the diff with read-only
  save tools and labels each change in plain words ("player.gold went from 250 to 300: your gold").

- Tested for real on the testbed: asked for "Infinite health", the helper found the damage code with
  find-what-writes and saved a working code-patch cheat in one session, with no game restarts
  (about $0.19).

### Fixed
- Saving an AI pack for a game that already had one replaced the earlier cheats. `pack_write` now
  keeps them (new cheats with the same id replace old ones) and keeps the old file as
  `pack.json.bak`.

### Changed
- The roadmap splits AI II: engine fast paths (Mono / IL2CPP / Unreal readers), REA and local models
  move to 0.9.1.

## [0.8.0] - 2026-11-07

### Added
- **Make a trainer (AI)** view: pick your running game, tick the cheats you want, and an AI helper
  finds them with you. It asks for one small thing at a time ("Spend some gold, then press Done",
  "Does your gold show 12345?") with big Done / Yes / No buttons, shows what it is doing and the
  estimated cost, can be cancelled at any time, and saves a cheat pack marked as AI-made and tagged
  with your game's build. Load it on the Trainer tab as usual.
- **`ot-mcp`**: the guarded tool server the AI uses (MCP over stdio; also `opentrainer-desktop --mcp`).
  Tools: list processes, attach (anti-cheat guard first, saves backed up), game info, first/next
  scans, read, test writes (undone by `revert_all`, detach or exit), pointer scan and re-check, AOB
  maker and uniqueness check, pack validate / try / cheat on-off / write, save snapshot / diff /
  read. Every call is logged to `%LOCALAPPDATA%\OpenTrainer\ai-logs`.
- **AI helper sidecar** (`sidecars/ai-agent`, Node + the Claude Agent SDK). The agent gets no
  built-in tools (no shell, files or web): only the guarded OpenTrainer tools and "ask the player",
  everything else is denied, and no user settings or plugins are loaded. Default model Claude Opus
  5.5; uses your Claude subscription by default, or `ANTHROPIC_API_KEY` if you choose. A spending
  limit (default $5) stops long runs.
- **Sign in to Claude** from the helper: the app checks whether the helper's Claude engine is signed
  in (`auth status`) and, if not, offers a Sign in button that opens the engine's browser login.
  Claude settings inherited from another Claude program are not passed to the helper.
- Tested for real on the testbed: the helper made a working "Lots of gold" pack in one session
  (about $0.36). That run showed a value set by a cheat during the helper's tests stayed after
  "undo"; `cheat_set` now keeps the original value so `revert_all` and detach put it back.
- Decisions recorded (PLAN §8): keep the name OpenTrainer, Claude subscription as the default AI,
  a GitHub-repo pack index for sharing (later), no in-game overlay for exclusive fullscreen.
- New dependencies: `@anthropic-ai/claude-agent-sdk` (Anthropic's terms; its Windows engine is about
  330 MB) and `zod` (MIT), in the sidecar only.

### Changed
- `Try OpenTrainer.cmd` installs packages when the AI helper's are missing and builds the helper.
- The check script and CI build the AI helper and run its tests and typecheck.

### Not yet
- "Find what writes to this address" (hardware breakpoints) and local AI models are planned for a
  later version.

## [0.7.0] - 2026-11-07

### Added
- **Save editor** (`ot_saves::codec`, `ot_saves::editor`; **Edit saves** in the Save backups view).
  Open any file in a game's save folders and change its values:
  - **JSON** and **INI** saves: every value listed by path (`player.gold`, `items[2].count`,
    `Player.Gold`); an edit replaces only that value's text, so spacing and order stay as they were.
  - **Unreal saves** (`.sav`, GVAS, up to UE 5.3): the property tree, with numbers and true/false
    values editable in place. Text, engine structs, maps and sets are shown read-only.
  - **Unity PlayerPrefs** in the registry (`HKCU\Software\<company>\<product>`): found automatically
    for Unity games, backed up, compared and restored like a folder, and editable (each value keeps
    its registry type).
  - **Wrappers** detected and put back on saving: gzip, zlib, Base64, and checksums (CRC32, MD5,
    SHA-1, SHA-256 at the start or end of the file), which are recomputed after an edit.
  - **Hex view** for anything else (or any file, via "Show bytes"): pages of bytes, a number
    inspector (8/16/32/64-bit, float, double) and "Change bytes here".
  - Opening and saving with no changes gives back the same bytes. Files are parsed from their
    bytes; nothing is deserialised or run.
  - Saving is refused while the game runs or when the guard finds anti-cheat in the game's folder,
    and every save folder is backed up first ("Before an edit").
- **Quick codes**: a cheat pack's `saveCodes` now show as one-click buttons for that game (same
  checks and backup). `file` starts with a folder placeholder (`{documents}`, `{mygames}`,
  `{savedgames}`, `{appdata}`, `{localappdata}`, `{localappdatalow}`, `{playerprefs}`) and must be in
  one of the game's save folders. The validator now checks codecs, placeholders and values. The
  example packs have "999,999 gold" and "99 ammo" codes for the testbed's save file.
- New dependencies (small, MIT/Apache): `flate2`, `base64`, `crc32fast`, `md-5`, `sha1`.

### Changed
- SQLite saves are not in this version (they need a large library); the plan moves them later.

## [0.6.0] - 2026-11-07

### Added
- **Save backups** (`ot-saves`, desktop **Save backups** view). OpenTrainer finds where a game keeps
  its saves and backs them up when the game starts, when it closes, when you attach to it (before
  any cheat), and whenever you click **Back up now** (with an optional note).
  - Save locator: Unity (`<exe>_Data\app.info` → `AppData\LocalLow\<company>\<product>`), Unreal
    (`Saved\SaveGames`), Steam Cloud folders (`Steam\userdata\<id>\<appid>\remote`), and folders
    named like the game in `Documents\My Games`, `Documents`, `Saved Games`, `%APPDATA%`,
    `%LOCALAPPDATA%` and `LocalLow`. The game's install folder is never picked. You can add or
    remove folders yourself.
  - Backups live in `%LOCALAPPDATA%\OpenTrainer\saves`. Each file's contents are stored once, so
    repeated backups of unchanged saves cost no space. Automatic backups identical to the newest one
    are skipped, and only the newest 30 automatic backups are kept; your own backups stay until you
    delete them. Folders over 20,000 files or 2 GB are refused.
  - **Compare**: any backup with the saves now, or with the backup before it. Text saves show the
    changed lines (long one-line JSON is cut to the part that changed); other files show changed bytes
    with the 32-bit number at that spot.
  - **Restore**: only while the game is closed, after the anti-cheat guard checks the game's folder
    (no restores for online games). Every stored file is checked first, and your current saves are
    backed up ("Before a restore") so a restore can be undone. Files not in the backup are left alone
    unless you tick the box to delete them.
  - **Steam Cloud warnings** when Steam syncs a game's saves (and whether Steam is running).
- Guard: `ot_core::guard::check_install(exe)` checks a game that isn't running (install-folder markers
  and the online-only list), failing closed.
- Testbed: `save` writes `AppData\LocalLow\TestCo\Testbed\save1.json`, `load` reads it back, and the
  testbed writes a Unity-style `testbed_native_Data\app.info` so the locator finds it.
- New dependency: `similar` (Apache-2.0) for line diffs.

## [0.5.0] - 2026-11-06

### Added
- **Crash-safe restore** (`ot_core::journal`): every code change (patch) is recorded with its original
  bytes before it is written. The desktop app also writes that record to
  `%LOCALAPPDATA%\OpenTrainer\restore\<pid>-<start time>.json`. If OpenTrainer crashes or is killed,
  the next start puts the original code back (and says so in a notice), and a crash handler does the
  same when the app itself fails. Recovery attaches through the anti-cheat guard like any attach, only
  ever writes the recorded original bytes over the recorded patched bytes, and skips anything else.
  Journals of games that have closed are deleted.
- **Pointer scanner** (`ot_core::pointer`, desktop **Pointers** tab): finds paths from the exe or a
  DLL to a value, Cheat Engine style (`[[game.exe+0x3A000]+0x10]+0x18`), shortest first, up to 7
  levels. After restarting the game, enter the value's new address to keep only the paths that still
  work. Results survive detaching and re-attaching. Paths can be added to the address list.
- **Signature maker** (`ot_core::signature`, desktop **Signatures** tab): turns a code address into
  the shortest byte pattern that matches once in its module. Jump and call targets, RIP-relative and
  absolute addresses are wildcarded; struct offsets and constants are kept.
- **Pointer paths in the address list** and every address field: `[[module.exe+0x10]+0x18]+0x40`.
  Pointer entries follow the value when it moves.
- **`Try OpenTrainer.cmd`**: double-click to open the fake test game and the app together. It stops a
  leftover OpenTrainer web server that would block the app, says so if the app is already open, and
  keeps its window open when something fails.
- Testbed: gold pickups (`earn`) now run fixed-byte code too (marker 4).

### Changed
- All code writes go through `AttachedProcess::apply_code` / `revert_code`, which check the bytes
  first, refuse a change that overlaps another one, and record it. Raw code writes are no longer
  public. Detaching restores every recorded change, even one the trainer never turned off.
- The Trainer tab shows each patch as on or off from the bytes really in the game, refreshed on every
  poll.
- Code caves (`"kind": "cave"`) are not part of this version; the plan moves them to a later one.
  Packs that use them still get a clear message.

## [0.4.0] - 2026-11-06

### Added
- **Cheat packs** (`ot-packs`): the v1 pack format from `docs/PLAN.md` §4 as strict Rust types
  (unknown fields rejected) plus a JSON Schema at `crates/ot-packs/schema/pack.v1.json`.
- **Validator** with JSON paths in every message: unique ids, hotkey syntax and conflicts, settings that
  match the cheat's UI, numbers that fit their value type, slider ranges, address forms, AOB and byte
  syntax, patch length and safe instructions, SemVer and build hashes. Clear messages for an unknown
  schema version and for `cave` (arrives in v0.5).
- **`value` op**: a typed value at a static base (`module.exe+0x…`) or an AOB signature (raw, `abs32`
  for x86 code, `rip32` for x64 code), then a Cheat Engine-style pointer chain. Toggles freeze (at a
  fixed value or the current one), sliders and number inputs set (optionally held), buttons add or set.
  Frozen pointer chains are re-resolved on every refresh, so a moved object is followed.
- **`patch` op**: replaces a few bytes at an AOB-located site. The signature must match exactly once,
  the original bytes are checked before every write and on restore, and a site left patched (e.g. by a
  crash) is recognised. Replacement bytes are decoded with iced-x86 and only plain instructions are
  allowed: no calls, indirect jumps, system calls, interrupts, privileged or I/O instructions, and
  near jumps must land inside the game module. Limit 32 bytes.
- **Trainer runtime** (`ot_packs::Trainer`): checks the exe name, architecture and SHA-256 build
  against the pack (verified / listed / unknown / untagged), restores every patch on toggle off,
  unload, detach, drop and app exit. Shows what each cheat touches (module offset, bytes and
  disassembly for patches, address chain for values).
- **Hotkeys**: `F1`, `Ctrl+Num1`, `Alt+Shift+F5`, `Num+` … parsed and normalised; letters, digits and
  Space need Ctrl or Alt. The desktop app registers them with Win32 `RegisterHotKey` (no keyboard hooks)
  on its own thread while a pack is loaded, reports keys another app already holds, and unregisters
  them with the pack.
- **Pack store**: example packs built into the app and user packs in
  `%APPDATA%\OpenTrainer\packs\<game-id>\pack.json` (4 MiB limit).
- **`otpack` CLI**: `otpack validate <pack.json>...` and `otpack schema`.
- **Example packs** for the x64 and x86 testbed (`packs/examples/`): Infinite Health and Infinite Ammo
  (patches), Health Lock at 999 (freeze), +1000 Gold (button), Set Gold (number), Move Speed (slider).
- Desktop: **Trainer** tab (now the first tab): packs for the attached exe and arch, load/unload,
  cheats grouped by category with switches, buttons, number inputs and sliders, hotkey badges and
  conflicts, held values with Release, failed cheats with the reason, build badge and warning,
  AI-made badge, "What it changes" details, pack warnings. Hotkey presses refresh the UI at once.
- `ot-core`: AOB patterns and module scanning, guarded code writes (`write_code`: temporary
  protection change, restore, instruction-cache flush, logged), pointer-chain resolution, exe SHA-256,
  iced-x86 patch checker and disassembler.
- Testbed: the damage, fire and root-pointer code are inline assembly behind fixed marker bytes, so
  AOB signatures are identical on every build, compiler version and architecture.
- Tests: 25 `ot-packs` unit tests; 7 trainer integration tests on live x64 and x86 testbeds (every
  example cheat, hotkeys, restore on drop, static pointer chain, drifted/missing/ambiguous signatures,
  null pointer, build hash match, wrong exe/arch refused, leftover patch restored); 8 new `ot-core`
  unit tests (AOB, patch checker, hashing); desktop command tests; a local-only hotkey test that
  presses a real key (`cargo test -p opentrainer-desktop -- --ignored hotkeys`); 7 new frontend tests.

### Changed
- Testbed test helpers moved to a `testbed-harness` crate shared by `ot-core` and `ot-packs`. Their
  scratch folders now live under `target/tmp` instead of `%TEMP%` (the guard looks three folders up,
  and `%LOCALAPPDATA%` can hold a real anti-cheat folder on a developer PC).
- The app restores patches and stops freezes on exit.

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
