# Status

_Last updated: 2026-11-09 — v1.8.0_

## Done
- CI green on GitHub (repo jackuri/opentrainer, public since 2026-11-08) through v0.10.0; v1.0.0 pushed and tagged
  (its CI and Release runs started on GitHub).
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
- v0.9.0 AI II: find what writes (hardware breakpoints, x64 + WOW64, live-tested), AI code-patch
  cheats (watch tools + prompt), pack repair (heal, Trainer "Try to repair", `pack_heal`), engine
  detection (`game_recon`, optional `um scan`), "Explain with AI" for save comparisons.
- v0.9.1 AI III: .NET metadata reader for Unity Mono / XNA games (`game_classes`, tested on Windows'
  System.dll), local AI backend (Ollama / LM Studio via ANTHROPIC_BASE_URL), optional REA-style
  analysis MCP server, AI settings remembered.
- v0.9.2 AI IV: IL2CPP metadata names (self-calibrating; verified on a real format-39 file, read-only,
  game not started), Unreal version + tips (verified on real UE 4.19 / 5.7 exes), anti-cheat note in
  game_classes, better name search. Manual probes (`OT_IL2CPP_PROBE`, `OT_DOTNET_PROBE`, ignored
  tests) read a real game's files on this PC; no game data is committed.
- v0.9.3 AI V: Unreal object reader (`unreal_find`): classes, properties (type + offset) and live
  object values for UE 4.23+ / UE5 games, self-calibrating layout, memory-search fallback when the
  object list can't be read directly (no decoding). Tested read-only on Species: Unknown (UE 5.7,
  solo): 12,594 classes, the player's Health / MaxHealth read live (100 / 100).
- v0.10.0 Extras: Games tab (Steam / Epic / GOG / Xbox / own exe, pack + guard status, "Play with
  cheats" with a start preset), presets that keep trying until values exist, favourites, notes, own
  keys, controller combos (XInput), overlay window (click-through, no injection), phone remote (off
  by default, one-time QR pairing). All checks pass. Live-tested: presets on the testbed; the
  remote's pairing over real sockets; the library lists this PC's games (30 Steam, 1 GOG).
- v1.0.0 Release: Windows installer (NSIS, per-user, 4.6 MB, `.otpack` file association, single
  instance), new icon, Packs tab (`.otpack` export / import with a "what's inside" check, remove
  keeps a copy, community list), About tab (updates, guide, "never does"), Tauri updater signed with
  our own key (off until the repo is public), AI engine downloaded on first use, release workflow
  (signing-ready), user / developer / release docs. All checks pass. Live-tested: a release
  installer and its update signature built locally; latest.json made; the AI engine download
  (same files and npm command as the app) in a temp folder, and the helper loads with it.
- v1.8.0 Hide games: `library_set_hidden(id, hidden)` keeps hidden ids in
  `%APPDATA%\OpenTrainer\hidden-games.json` (broken file = nothing hidden); `LibraryGame.hidden`;
  Games tab Hide / Unhide buttons, "Show hidden (N)" switch, "Hidden" badge, all-hidden message.
  Only the Games list changes (the Cheats game picker still lists running hidden games).
- v1.7.0 Official no-anti-cheat modes: `guard::check` (running game) treats install markers alone as
  `Verdict.notes` (allowed); anything live (process/service/driver/module), online-only or a failed
  check still blocks, listing the markers too. `check_install` (saves, Games list) unchanged; the
  Games list shows marker-only games as "Has X: works only when the game runs without it" and lets
  Play try (the attach decides). `Error::Blocked` / `CommandError.verdict` are boxed (clippy
  large-err). On this PC only Halo MCC has an official no-EAC launch option (`-no-eac`, Steam
  option 2); DBXV2 (START.exe), Elden Ring (start_protected_game.exe) and Dune Awakening (BattlEye)
  always run their anti-cheat and stay blocked.
  All checks pass (live tests run with no anti-cheat running; while any game's anti-cheat runs,
  every attach on the PC is refused, tests included).
- v1.6.0 Hooked pointers: `ot_core::capture` reads a register at a code address with a hardware
  execute breakpoint (attach as debugger, arm DR0 on all threads, resume flag on hits, optional
  `when` register = value, disarm, detach); live-tested on x64 and x86 testbeds. Pack schema
  `address.capture {register, add?, when?}` (aob only, no operand, needs arch). Trainer: captured
  starts cached per (site, register, when); user actions wait up to 6 s, held values refresh every
  5 s (0.4 s wait, keep last), states show "not found yet" (status ready). `ct_script` recognises
  pointer-grab hooks (mov tmp,var / mov [tmp],reg, mov [var],reg, test/cmp + jne guard, lea no-op;
  globalalloc or label vars). DBXV2: 137 cheats (Current Health = rdi+0xF8, matching the table's
  own health script). All checks pass.
- v1.5.0 Smarter tables (user's DBXV2.CT from FearLessRevolution gave 0 cheats): `ct_script.rs`
  reads Auto Assembler / Lua as text and recognises symbol patterns (Lua RIP idiom, aobscanmodule +
  registersymbol, define, label-at-number), byte-swap scripts (→ patch, x64 only, changed bytes
  wildcarded), and `ct.rs` evaluates offset math. DBXV2 now gives 98 cheats (92 values, 6 patches).
  A script-style table for the live x64 testbed (Lua RIP pointer, slot math, byte swap) passes.
  Next (1.6.0): values under pointers that injection scripts grab (battle_ptr / player_ptr, 39 in
  DBXV2) via a hardware-breakpoint "capture" address; maybe Binary (bit) values.
- v1.4.2 Hardening: re-test (all checks; 29/29 real-app e2e incl. .CT drop → install → use,
  share dialog, Update button), dependency scan (npm: 0 in shipped code, 10 moderate in eslint's
  minimatch, dev-only, fix needs a major eslint can't take yet; Rust via OSV: zip 4.6.1 removed by
  turning off the updater's zip feature; glib/zbus are Linux-only; proc-macro-error is build-time),
  and an independent security review (0 high, 3 medium, 6 low, all fixed). Community repo PR check
  also drops persisted credentials (pushed).
- v1.4.1 Update button: About has one **Update** button (check → install → restart; builds
  without updates open releases/latest). The user chose to make `jackuri/opentrainer` **public**
  (done 2026-11-08 after a scan for secrets / binaries found none). The user added the
  `TAURI_SIGNING_PRIVATE_KEY` secret (2026-11-08); the v1.4.1 release was built with in-app updates
  on and has `latest.json` + `.sig` (fetched publicly without a sign-in). 1.4.1 is the first
  self-updating version; older copies need one manual install. A real update runs with 1.4.2.
- v1.4.0 Share with everyone: `github.rs` (device flow, token in Credential Manager via
  CredWriteW/CredReadW, fork → merge-upstream → branch from upstream main → contents PUT → PR;
  owner shares from a branch), ShareDialog in Packs. Tested against a fake GitHub on localhost.
  Community repo commit a295e09 (local, not pushed): PR check = pack files only, index rebuilt on
  main. OAuth app registered under jackuri (client ID in `github.rs`, device flow on, tokens
  don't expire). Real share tested 2026-11-08 as jackuri (owner path: branch in the list repo):
  PR jackuri/opentrainer-packs#1 had only the pack file and the right text, its check passed; closed.
  The fork path (other players) is tested against a fake GitHub only. `github::tests::live_share`
  (ignored) repeats the real test.
- v1.3.0 Cheat Engine tables: the user asked for a ready library of cheats (importing Wand/WeMod's
  was declined: their cheats are proprietary and locked). `ot_packs::ct` converts `.CT` tables'
  value entries into packs (roxmltree, DTDs off, 16 MiB / 20k entries / depth 32 / 500 cheats);
  scripts, fixed addresses, script symbols, text/AoB values are listed as skipped. Opened through the
  normal pack preview (`share::open_path(path, exe)`), with "Choose the game's exe" when the table
  names only DLLs. Integration test writes a CE-style table for the live testbed (x64 + x86) and
  checks set / hold / release on its real values. Also in 1.3.0 (user's request): typed answers to
  every AI question and messages while the helper works (`ai_note` → sidecar `note` → NoteBox,
  delivered via PostToolUse / Stop hook `additionalContext`). All checks pass.
- v1.2.3 Game lists: the user found the program dropdowns confusing (every running program,
  no games). New `running_games` command matches running exes to installed games' folders
  (deepest folder wins, no drive roots, no crash reporters / Windows programs) or to a pack's exe
  name; Cheats list, AI and Save backups pickers show those by game name, others behind "Show all
  running programs". All checks pass.
- v1.2.2 Fixes, found by an end-to-end test of the real built app (driven through WebView2's
  DevTools port on this PC: covers, community list, auto-load and Unload on the x86 testbed,
  Infinite Health checked against the game's real health, scanner → Find pointers → pointer scan →
  Copy as pack cheat on the x64 testbed; 16 of 16 pass) and a code review: covers didn't show in
  the installed app (CSP had no `blob:`, and Tauri hands raw bytes over as a number list), auto-load
  came back after Unload + tab switch, Xbox logo paths could leave the game folder, cover cache
  didn't refresh. All checks pass.
- v1.2.1 Covers: game cards show cover art from files already on this PC (Steam's library cache,
  GOG's icon, an Xbox game's logo, else the exe's icon via Windows; nothing downloaded, copied or
  saved). On this PC 29 of 30 games have a store picture (Steam headers, GOG icon); the 30th (a
  Steam test server) gets its exe icon. Checked in a browser with real Steam covers. All checks pass.
- v1.2.0 Look: new look like modern trainer apps (user's request): sidebar with icons, navy +
  blue-violet theme (one place: `apps/desktop/src/index.css`), cheat cards with big ON / OFF
  switches and key caps, game cards with covers made from the game's name, new icon colours, phone
  remote recoloured. Checked by eye in a browser with sample data (Cheats, Games, Packs, Save
  backups, AI). All checks pass.
- v1.1.0 Polish: "Browse…" pickers (game exe in Games and Save backups, save folders), plain
  `pack.json` import (picker and drop), the only pack for a game loads by itself on attach (not
  again after Unload), "Get a pack…" from an empty Trainer tab, "Find pointers" on scanner results,
  "Copy as pack cheat" on pointer paths (checked: a pack with the copied cheat passes
  `otpack validate`). All checks pass.
- Hand-tested by the user on v0.10.0: reported working.
- Hand-tested by the user on v0.5.0: Trainer tab with the testbed pack (Infinite Health, +1000 gold)
  works.

## In progress
- v1.1.0 – v1.7.0 are pushed and tagged (v1.7.0 is Latest, in-app updates on). v1.8.0 is committed locally. The community list has its first real shared pack (Species Unknown, merged by the user).
- The community packs repository is live and public: https://github.com/jackuri/opentrainer-packs
  (one demo pack for the testbed; its "Check packs" workflow is green). The live `index.json` and
  pack were downloaded, the SHA-256 matched and `otpack validate` passed.

## Next
- The user's choices, when they want them (docs/RELEASING.md): a code-signing certificate
  (SignPath works now that the repo is public; Microsoft Trusted Signing about $10/month). The
  updater key still has to be added as the `TAURI_SIGNING_PRIVATE_KEY` secret by the user.
- SQLite saves: moved out of v0.7.0 (needs the large rusqlite library; the user chose to wait).
- Code caves: dropped from v0.5.0. They need a design review before they come back.
- The roadmap (PLAN §6) is done through 1.1.0. Left for the user to choose: SQLite saves, code
  caves, more save formats (XML / YAML editing, Bethesda, RPG Maker, GameMaker, Godot), moving saves
  between your own Steam accounts (PLAN §2.3), a memory viewer (PLAN §2.2).

## Known issues
- v1.0.0 – v1.2.1 haven't been installed and clicked through by hand yet (the user saw v1.2.0's
  Games tab and asked for real covers): the installer,
  double-clicking an `.otpack`, dropping a pack on the window, the Packs and About tabs, the AI
  engine download button, the Browse pickers and auto-load.
- In-app updates haven't run for real yet: the first chance is updating 1.4.1 to 1.4.2. The private
  updater key is at `%USERPROFILE%\.opentrainer-release-keys\updater.key` (outside the repo, no
  password); losing it means installed copies can't update.
- The installer isn't code-signed, so Windows shows "Windows protected your PC" (More info → Run
  anyway).
- The AI engine download pins the SDK and zod to the tested versions, but not their own
  dependencies (about 105 packages from npm, no lockfile).
- v0.10.0: the user reported it works; not confirmed one by one: the phone remote on a real phone,
  controller combos with a real controller and "Play with cheats" with a store game.
- The overlay doesn't show over exclusive-fullscreen games (by design, PLAN §8); switch the game to
  borderless or windowed.
- The phone remote needs the PC and phone on the same network; Windows asks once to allow it
  through the firewall. It uses plain HTTP on the local network (pairing keeps other devices out,
  but someone on the same Wi-Fi could watch the traffic). Port 47110, or a free one if taken.
- "Play with cheats" picks the game's process as the pack's exe or the biggest exe running from the
  install folder; a game that starts from outside its folder isn't found (attach from Cheats).
- Library: Epic and GOG are read from their own listing files and registry keys; Xbox games are
  found in `X:\XboxGames\*\Content` only. `um scan --list` isn't used.
- Unreal: games whose object list can't be read directly (e.g. Species: Unknown) use the slower
  memory search; the first `unreal_find` there took about 6 minutes (the game's memory is paged in
  from disk). Results are kept for the session. UE 4.22 and older (a different name list) aren't
  read; Captain Hardcore (UE 4.19) wasn't tried. No real AI run of `unreal_find` yet.
- Local AI (Ollama / LM Studio) and the analysis server are wired and unit-tested but not run for
  real here (neither is installed on this PC). Ollama's Claude-compatible API doesn't support
  tool_choice or prompt caching; the helper doesn't need them.
- `game_classes` gives names (and Mono types), not field offsets: Mono and IL2CPP lay out objects at
  run time / in GameAssembly.dll. IL2CPP field types aren't read.
- The IL2CPP reader is verified on one real file (format 39); older formats are tested with files
  from our own writer. Games that protect their metadata are refused.
- Real AI run of the v0.9.0 code-cheat path (user approved): "Infinite health" on the testbed, found
  with find-what-writes, working patch saved in one session, no restarts, about $0.19; the pack
  works on a fresh testbed. Repair and "explain a save" have no real Claude run yet. The run also
  showed AI packs for the same game overwrote each other; fixed (merged).
- The AI helper trusts the player's answers; a wrong "Done" or "Yes" (e.g. healing instead of taking
  damage) can make it patch the wrong code. It asks the player to confirm in game before saving.
- Find what writes attaches as a debugger. Single-player games with DRM that refuses debuggers may
  close or complain; OpenTrainer doesn't hide from them (by design). Only one debugger can attach,
  so it fails if another (e.g. Visual Studio) is attached.
- Pack repair needs the cheat's code to still exist and look mostly the same (60% of the old
  signature's fixed bytes); static-address value cheats need a new pointer scan instead.
- `um scan` is only used when `um` is a Windows command (`um.exe`/`um.cmd`/`um.bat`); the plugin's
  bash launcher isn't called.
- Real AI runs (user approved): the first stopped at "Not logged in" (the Claude desktop app's
  sign-in isn't shared with the Agent SDK's engine), so the sign-in check and button were added.
  After the user signed in, a full session on the testbed (scripted player) found gold, used two
  restarts to pick a stable pointer path, tested and saved a working "Lots of gold" pack, for about
  $0.36. The pack works on a fresh testbed. The run showed value cheats weren't undone by
  revert_all; fixed (cheat_set keeps the original value).
- The AI helper needs Node.js and, for the subscription, a signed-in Claude Code on the PC. API keys
  are read from the `ANTHROPIC_API_KEY` environment variable; the app doesn't store keys.
- The Agent SDK's Windows engine is about 330 MB in `node_modules` (and in CI installs).
- The Save backups view (incl. the editor), Pointers and Signatures tabs haven't been clicked
  through by hand yet (covered by frontend tests and live tests against the testbed).
- Save editor limits: Unreal saves from UE 5.4+ open as hex; Unreal text values, engine structs,
  maps and sets are read-only; XML / YAML saves open as hex; files over 64 MB aren't opened.
- PlayerPrefs floats are assumed to be REG_QWORD doubles (Unity's Windows format); a game that
  stores numbers differently shows them as raw whole numbers.
- Wrapper detection only handles a checksum over the whole rest of the file; seeded or partial
  checksums, and encryption, aren't detected (encrypted saves open as hex and are not decrypted).
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
