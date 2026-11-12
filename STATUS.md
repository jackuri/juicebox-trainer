# Status

_Last updated: 2026-11-12 — v1.29.0_

## Done
- CI green on GitHub (repo jackuri/juicebox-trainer, public since 2026-11-08) through v0.10.0; v1.0.0 pushed and tagged
  (its CI and Release runs started on GitHub).
- v0.1.0 skeleton: workspace, Tauri app boots, testbed game (x64/x86/shifted), CI, check + bump scripts.
- v0.2.0 core memory: process list, guarded attach/detach, typed read/write, module map, regions,
  anti-cheat guard with tests, desktop UI to attach and read/write.
- v0.3.0 scanner: multithreaded first/next scans, address list, 50 ms freeze loop, manual-tools UI.
- v0.4.0 packs: v1 schema, validator, `value` and `patch` ops, safe-patch checker, trainer runtime,
  build-hash check, hotkeys, pack store, `juicepack` CLI, example packs, Trainer tab.
- v0.5.0 stability: checked code writes, crash-safe journal with recovery, pointer scanner,
  signature maker, pointer paths in the address list, `Try JuiceBox Trainer.cmd` launcher (stops a
  leftover dev server, keeps errors on screen).
- v0.6.0 Saves I: save locator (Unity, Unreal, Steam Cloud, named folders), backups on game start /
  exit / attach / by hand with notes, content-deduplicated store with retention, compare (text lines
  and binary bytes), guarded restore (game closed, install-folder guard check, hash check, backup
  first), Steam Cloud warnings, Save backups view. Testbed `save` / `load`. All checks pass locally.
- v0.7.0 Saves II: save editor for JSON, INI, Unreal GVAS (up to UE 5.3) and Unity PlayerPrefs
  (registry, also backed up / compared / restored), gzip / zlib / Base64 wrappers and CRC32 / MD5 /
  SHA-1 / SHA-256 checksums recomputed on save, hex view with number inspector, quick codes from
  packs (validated placeholders), guarded saving (game closed, guard, backup first). All checks pass.
- v0.8.0 AI I: guarded `jb-mcp` tool server (20 tools, logged, every change revertible, guard on
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
  game_classes, better name search. Manual probes (`JB_IL2CPP_PROBE`, `JB_DOTNET_PROBE`, ignored
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
- Real-app test of the 1.15.0 release build (DevTools port, 14/14 passed): Find a cheat table on
  every playable game (25), its panel; a test table written to the real Downloads folder showed up
  within seconds with "2 cheats JuiceBox Trainer can use"; Turn into cheats → install dialog (script
  listed as not imported) → credit typed → installed pack has author + `meta.source`; not offered
  again; a `javascript:` page address refused. The test pack was removed (kept in packs-removed)
  and the test table deleted. The search buttons weren't clicked (they'd open the browser).
  Installer: `release-out/JuiceBoxTrainer_1.15.0_x64-setup.exe`.
- Real-app test of the 1.16.0 release build (DevTools port, local test copy of the community list
  via `JB_PACK_INDEX`, 14/14 passed): 2 links read; Baldur's Gate 3's Find a cheat table lists only
  its link with maker / site / cheats; unlisted and off-site pages refused; a table installed with
  a FearLess page offers Share the table's link and its dialog opens (not sent); other packs don't
  offer it. Test pack removed. Installer: `release-out/JuiceBoxTrainer_1.16.0_x64-setup.exe`.
- Real-app test of the 1.17.0 release build (DevTools port, local test copy of the community list,
  14/14 passed): credit / badge / terms / maker buttons on a community pack, MIT shown for a pack
  without terms, unknown link kinds and packs without credit refused, the share dialog asks who
  made the table, the three kinds of terms and a support page (not sent). It found a bug, fixed:
  a credited table pack (author = maker's name) was called "a file" in the share dialog; now a
  pack whose credit names a `.CT` file counts as a table. Test pack removed. Installer:
  `release-out/JuiceBoxTrainer_1.17.0_x64-setup.exe`.
- Real-app test of the 1.18.0 release build (DevTools port, testbed/web on a local server, 29/29
  passed): agreement first, web_open refused before signing, sign needs the box + a name, saved
  with the date; Web games tab + play-fair note; add (plain http elsewhere refused), open, helper
  in the page; the game page's IPC is refused ("not allowed by ACL"); health in WebAssembly
  memory found / set / locked / unlocked, gold as game.player.gold found / set, ammo as f32 found;
  cheats saved, window closed, reopened and cheats work again; unknown calls refused. It found a
  bug, fixed: the memory count was read only once, before the game made its memory; it's now
  read again every few seconds. Older real-app tests (1.13 to 1.17) re-run on 1.18 with a
  temporary test signature: 101/101. The user's own agreement / web-games files were backed up and
  restored (none existed, so the user still signs it themselves). Installer:
  `release-out/JuiceBoxTrainer_1.18.0_x64-setup.exe`.
- v1.18.0 Agreement + web games I (user request: web games like outbreakweb.com, which the user
  says is a fan-made co-op site where you invite friends; a warning and a waiver every player
  signs). `src-tauri/src/{agreement.rs, web.rs, web_agent.js}`, `components/{AgreementGate,
  WebGamesView, WebCheatsPanel}.tsx`, `lib/agreement.ts`, `testbed/web`. The agreement text is
  plain-language and not legal advice; the user should have it checked. Not tested on
  outbreakweb.com or any real site (only on testbed/web).
- Before releasing 1.17.0 (the user asked to run and check everything first): all real-app tests
  from 1.13 to 1.17 re-run on the 1.17 build, 101/101 passed (memory viewer + XML / YAML / SQLite
  saves 25, themes 34, Downloads tables 14, table links 14, credit and terms 14). Review fixes:
  MIT is no longer written into shared packs (it's the default; older versions can still read
  them, only packs with other terms or the maker badge need 1.17); the Downloads check pauses
  while JuiceBox Trainer is hidden.
- v1.17.0 Creator invitations (cheat-library plan step 3; the user chose "let makers choose"
  their terms): `schema::Source` `licence` / `support` / `from_maker`, `SUPPORT_SITES`,
  `is_support_url`; `github::{Terms, with_terms}` and terms lines in the PR text; community `Entry`
  `source`; `sharing::community_open_credit`; ShareDialog who-made-it + terms + support page;
  community rows with credit / badge / buttons; `docs/CREATORS.md` (invitation message for the
  user to send). Community repo commit 79b222e (local): index carries `source`, README terms and
  badge rule. Sending invitations is up to the user.
- v1.16.0 Table link directory (cheat-library plan step 2): `community::{TableLink, TABLE_SITES,
  table_site, table_problem, table_id}`, index `tables` (default empty, so older lists load);
  `community_open_table` (only listed pages), `github::{send_file, table_link, table_pr_text,
  community_share_table}`; PackSummary `credit` / `tableLink`; FindTable lists links
  (`lib/tables.ts` `linksFor`, `siteOf`); ShareDialog `mode="link"`. Community repo commit
  6c30cda (local, not pushed yet): `tables/<id>.json` allowed in PRs, index builder checks links
  (same rules) and writes `tables`. The list starts with no links: they come from players
  sharing them (nothing is copied from FearLess).
- v1.15.0 Get cheats from tables (cheat-library plan, step 1 of 5; the user picked the recommended
  plan on 2026-11-09 after hearing why bulk-copying FearLess tables isn't an option: permission,
  sign-in, scripts, game updates). `src-tauri/src/tables.rs` (`tables_search` fearless / web,
  `tables_recent` Downloads scan + cache), `components/CheatTables.tsx` (FindTable, DownloadedTables),
  `lib/tables.ts` (matchGame, last search, Not now list), `schema::Source` + checks, `packs_import`
  `credit`. FearLess shows a bot check to automated browsers, so its search URL (standard phpBB
  `search.php?keywords=…&sf=titleonly&sr=topics`) was not opened here; a real browser passes it.
  Plan, next: 1.16 link directory (community list lists where tables are, links only), 1.17
  creator invitations (credit badge, licence field, invitation text; the community repo's checks
  must accept `meta.source`), then tested AI-made packs for popular games, then code caves.
- v1.14.0 Themes (user request 2026-11-09: game-themed looks, current one as default): Themes tab
  (`ThemeView`, live preview cards each with their own `data-theme`), `lib/theme.ts` (list,
  localStorage `opentrainer.theme`, storage-event sync for the overlay), `themes.css` (10 blocks:
  zinc + emerald scales and extras; the scales were made from a few anchor colours by OKLab
  mixing). Names are generic on purpose (no Halo / Fallout / PlayStation / Xbox / Game Boy names
  or logos). `text-white` in UI chrome is now `text-bright` (dark in light themes).
- Real-app test of the 1.14.0 release build (DevTools port, 34/34 passed): Themes tab, 10 live
  previews, each theme applied + saved + right colour scheme, remembered after a reload, scanlines
  in the terminal theme; screenshots of Themes / Games / Save backups in every theme looked right.
  The test put the saved theme back to the default. Installer:
  `release-out/JuiceBoxTrainer_1.14.0_x64-setup.exe`.
- v1.13.0 SQLite saves: `codec::sqlite` with `rusqlite` 0.41.1 (`bundled`, `serialize`, no default
  features; the user asked for it on 2026-11-09). Workspace `rust-version` is now 1.89 (rusqlite's).
  Fields `Table[rowid].Column` (names quoted like JSON keys when odd), one-cell UPDATE with
  triggers/views off, WAL header bytes kept, `-wal` with data blocks edits (editor note). Save code
  codec `sqlite`. Tested with databases made in the tests only.
- Real-app test of the 1.13.0 release build (DevTools port + testbed, 25/25 passed): memory viewer
  (module label, live refresh + highlight, inspector, write seen by the game, read-only header
  refused), XML / YAML / SQLite saves opened and edited through the real IPC, SQLite file still
  valid afterwards, editor shows the SQLite save, account view answers. Installer:
  `release-out/JuiceBoxTrainer_1.13.0_x64-setup.exe`.
- v1.12.0 Memory viewer: `AttachedProcess::{read_partial, region_at, edit_bytes}` + `ByteEdit`,
  commands `memory_view` (max 4096 bytes) / `memory_edit`, `MemoryViewer` tab ("Memory") and a ⌗
  button in the address list. Live test against the testbed (x64 + x86): partial read across a
  region gap, data edit seen by the game, header refused, code edit restored on detach.
- v1.11.0 Account copy: `jb_saves::accounts` (accounts, view, plan, copy, rewrite_ids),
  `Reason::BeforeAccountCopy`, `Library::add_folder_as`, commands `saves_accounts`,
  `saves_account_plan`, `saves_account_copy`, `AccountCopyPanel` in Save backups (hidden unless the
  game has per-account saves and the PC knows 2+ accounts). Tested with a fake Steam folder only;
  not tried with a real game or real second account.
- v1.10.0 Save formats: `codec::xml` (roxmltree, already a dependency of jb-packs; DTDs refused,
  element text + attributes, CDATA, repeated names `Item[2]`), `codec::yaml` (own block-style
  scanner: maps, lists, quotes, comments, `---` documents as `doc1.…`; flow / alias / block text
  read-only; anything else = hex), `codec::lzstring` (bit-identical to the JS library, test vectors
  from node_modules/lz-string) as `Layer::LzString` (RPG Maker MV), INI: quoted values (GameMaker),
  `/` in keys, loose section names and Godot multi-line `{`/`[` values (read-only), `codec::ess`
  (Skyrim LE/SE + Fallout 4 header summary, read-only), "GDEC" Godot encrypted note. Save codes
  accept `xml` / `yaml`. Tested with files made in the tests only (no real game saves).
- v1.9.0 Find a value: `jb_core::finder::ValueFinder` (i32 + f32 + f64 at once; typed numbers with
  `,` separators; decimals match within one unit of the last digit; unknown start = one i32 snapshot
  split into i32 + f32 by `Scan::next_as`). Commands `finder_start/next/results/reset` and
  `my_cheat_save` (writes `packs\mine-<exe>\pack.json`, next free F-key, build sha, checks the path
  leads to the found address, loads it if no other pack is loaded). UI: Cheats → "Find a value" tab
  (tries each place with a test number and restores the real one; Keep/Set; Use it now = address
  list + freeze; Save = static module address, or pointer scan → restart → find again → filter).
  Pending save kept in localStorage; pointer results only live while JuiceBox Trainer stays open.
  Scanner values refresh every 1 s while visible. Real-app test (release build + testbed via
  DevTools) passed: found gold, kept 9999, saved after restart as `testbed_native.exe+0x49200`
  `[0x10, 0x10, 0xC]`, the Trainer toggle held it.
- v1.8.1 One entry per running game: `running_games` keeps one process per game (pack exe, else
  most memory via `jb_core::memory_in_use`, else biggest exe, else lowest PID); helpers stay under
  "Show all running programs". The Play button's `pick()` still uses pack exe / biggest exe (memory
  is too low 4 s after launch to judge); a Unity game's tiny stub exe may lose to its launcher there.
- v1.8.0 Hide games: `library_set_hidden(id, hidden)` keeps hidden ids in
  `%APPDATA%\JuiceBox Trainer\hidden-games.json` (broken file = nothing hidden); `LibraryGame.hidden`;
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
- v1.6.0 Hooked pointers: `jb_core::capture` reads a register at a code address with a hardware
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
  without updates open releases/latest). The user chose to make `jackuri/juicebox-trainer` **public**
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
  PR jackuri/juicebox-packs#1 had only the pack file and the right text, its check passed; closed.
  The fork path (other players) is tested against a fake GitHub only. `github::tests::live_share`
  (ignored) repeats the real test.
- v1.3.0 Cheat Engine tables: the user asked for a ready library of cheats (importing Wand/WeMod's
  was declined: their cheats are proprietary and locked). `jb_packs::ct` converts `.CT` tables'
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
  `juicepack validate`). All checks pass.
- Hand-tested by the user on v0.10.0: reported working.
- Hand-tested by the user on v0.5.0: Trainer tab with the testbed pack (Infinite Health, +1000 gold)
  works.

- v1.19.0 Web cheat packs: keys for web cheats (work while the game window is in front, note in
  the game), share as an `.otweb` file or with everyone (pull request `web/<id>.json`), open a
  friend's file, community web cheats in the Web games tab. All checks pass.

- v1.20.0 AI helper for web games: the app's local link to its game window for the AI's tool
  server (token, whitelisted calls, the helper's own locks only, undone at the end), ten `web_`
  tools in jb-mcp, the sidecar's web task and prompt, Find cheats with AI in Web games. All
  checks pass. Real-app test of the release build (testbed/web, a scripted stand-in for the
  helper that starts the real tool server through the real sidecar settings): 47/47, including
  the link refusing a wrong token, the player's keys call and "unlock everything", the helper's
  tests undone and its cheats shown at once and working with their key. One real AI run (the
  user's Claude login, $2 limit) on testbed/web with a simulated player: all three cheats (gold
  by name, health and ammo by search) found, checked, saved and still right after a reload, in
  67 s for about $0.23, with 7 questions to the player. Security review before release (user's
  request): no exploitable issue found in the new code; `npm audit` of the shipped packages 0
  issues (10 moderate in eslint's minimatch, dev only); `cargo audit` 0 vulnerabilities (5
  warnings for Linux-only or build-time crates). Fixed from the review: the app now undoes what a
  stopped helper left in the page (the link keeps each place's value from before; the tool server
  says "reverted" when it undid everything), lock ids must be `ai:<number>`, the link refuses a
  page that went to another website, each connection has its own thread, the token is never
  printed, page text over 300 characters is cut, and both prompts say text from the game is data,
  never instructions.

- v1.21.0 Smarter AI helpers (all five ideas of PLAN §5.6): game pictures, the values next to a
  find, known cheats and notes, only tried cheats saved, smaller tool sets, Opus or Sonnet, and
  the benchmark `scripts/ai-bench.mjs`. Real-app test of the release
  build (scripted helper on testbed/web): 61/61 (picture of the game window, web_near, known
  cheats and notes, untried cheat refused, hard stop undone). 1.20 re-tested once the user closed
  their app: 52/52, 1.19 36/36. Benchmark (one run each, so noisy; testbed x64, a console game, so
  no pictures): 1.20 3/3 with 16 questions, $0.57; 1.21 best model first 2/3 (its gold pointer
  path was luck), then after a prompt fix (a one-step path must survive a second restart) 3/3
  with 15 questions, $0.77; 1.21 cheaper model 3/3 with 7 questions, $0.22.

- v1.22.0 Clear problem messages: why a game closed (exit code, Windows crash record, cheats on,
  last change), web game pages that crash or freeze (Reload game), JuiceBox Trainer's screen restarts
  itself, Help page with recent problems and common questions, plain error messages, web links
  through `ShellExecuteW` (FearLess search opened Documents), known table page as the main button.

- v1.23.0 Rename to JuiceBox Trainer: names, folders moved on first start, installer removes
  OpenTrainer, `.juice` / `.juiceweb` (old endings still open), new icon, community repo text.
  GitHub repositories renamed 2026-11-11 (old addresses forward; checked).

- v1.24.0 Juice box faces: holiday looks by month (app corner + taskbar, Themes switch), wink on
  success, crossed-out eyes on problems, the smiling juice box as the everyday icon.

- v1.24.1 The taskbar / Alt+Tab icon follows the season too (Tauri's set_icon only set the small
  title-bar icon; the big one is set with WM_SETICON ICON_BIG). Checked with WM_GETICON on the built app.

- v1.25.0 Community growth (idea 2 of 3): Share with everyone after the AI helper succeeds; Ask the
  community for cheats on games without any (GitHub request form) and Most wanted in Packs
  (community repo builds wanted.json from "request" issues hourly). Pushed and tagged 2026-11-11;
  "request" label created on juicebox-packs, Most wanted workflow ran OK.

- v1.26.0 Instant engine cheats for Unreal (idea 1 of 3, Unreal part): Cheats → Make instant
  cheats reads the engine's names (GEngine → player → movement / world settings; player values by
  usual names on things the player owns) and saves an `engine-<exe>` pack of pointer paths. Frozen
  pointer paths now pause while they lead nowhere (level loads, respawns). Tested on a fake game in
  unit tests and on the testbed; not yet on a real Unreal game. Pushed and tagged 2026-11-12 at the
  user's request; the real-game test (Species: Unknown, CPU kept under 85 °C) waits until they are home.

- v1.27.0 More cheat table sources (idea 3 of 3): Find a cheat table also searches the Cheat Engine
  forum, OpenCheatTables, GitHub and Reddit r/cheatengine; tables with an open licence (or the
  maker's permission) can be on the community list as `tables/<id>.CT` next to their link, with
  "Turn into cheats now"; sharing a table's link can include the table itself. Community repo
  builder / check accept them. No real table is on the list yet. Scope widened at the user's request:
  online games without anti-cheat are allowed (CLAUDE.md, agreement, About, AI prompt, docs,
  community rules); games are still refused while their anti-cheat runs. Agreement version not
  raised (the change only allows more). Pushed and tagged 2026-11-12 (both repos).

- v1.27.1 Fix: the window went blank while finding a value in a web game once a result's value was
  gone (user's report on Outbreak; reproduced with the testbed web game's weapon swap). Gone values
  show "?"; every view is wrapped in an error boundary with Try again. Pushed and tagged 2026-11-12.

- v1.28.0 Cheat book: see and edit a pack's cheats without the game (names, groups, values, ranges,
  keys, notes, favourites, removal; never addresses or patches), from Packs, Games and Cheats; "ask
  the AI to fix it" opens the AI helper with the request filled in. Pushed and tagged 2026-11-12.

- v1.29.0 Pack ratings: 1–3 stars per community pack (rating issues + reactions on the community
  repo, counted hourly into ratings.json); Packs shows stars, Best rated first, and Rate it on
  installed community packs (GitHub sign-in as for sharing).

## In progress
- Pushed through v1.17.0; v1.17.0 is tagged and Latest (in-app updates on). Community repo pushed
  through 98efc39 (web cheats). v1.18.0 – v1.24.1 are pushed and tagged (from 1.23.0 the repositories are jackuri/juicebox-trainer and jackuri/juicebox-packs) (CI and Release green,
  installers published). v1.10.0 – v1.12.0 were
  released together inside 1.13.0 (no separate tags). The community list has its first real shared pack (Species Unknown, merged by the user).
- The community packs repository is live and public: https://github.com/jackuri/juicebox-packs
  (one demo pack for the testbed; its "Check packs" workflow is green). The live `index.json` and
  pack were downloaded, the SHA-256 matched and `juicepack validate` passed.

## Next
- Next (user's order 2026-11-11): try 1.26's instant cheats on a real Unreal game (Species: Unknown,
  with the user's OK, CPU kept under 85 °C), then instant cheats for Unity (ComeCloser is a Unity
  Mono game on this PC). Both wait until the user is home.
- AI helpers: live Unity Mono objects by class (like `unreal_find`) need a real Unity game to test
  on. The benchmark could get web and harder test games (values ×10, scrambled).
- The user's choices, when they want them (docs/RELEASING.md): a code-signing certificate
  (SignPath works now that the repo is public; Microsoft Trusted Signing about $10/month). The
  updater key still has to be added as the `TAURI_SIGNING_PRIVATE_KEY` secret by the user.
- Code caves: dropped from v0.5.0. They need a design review before they come back.
- The roadmap (PLAN §6) is done through 1.13.0. Left for the user to choose: code caves, moving
  saves between PCs, Bethesda save editing (beyond the summary), Ren'Py / .NET BinaryFormatter
  structural readers (PLAN §2.3), LZ4 wrappers.

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
  close or complain; JuiceBox Trainer doesn't hide from them (by design). Only one debugger can attach,
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
- The memory viewer hasn't been tried in the real app or on a real game yet (unit + testbed
  tests only). It shows 256 bytes; a pointer is shown as a JavaScript number (fine below 2^53).
- The Save backups view (incl. the editor), Pointers and Signatures tabs haven't been clicked
  through by hand yet (covered by frontend tests and live tests against the testbed).
- SQLite: WITHOUT ROWID and virtual tables aren't listed; blobs can't be edited; encrypted
  databases (SQLCipher) open as hex; databases over 64 MB aren't opened.
- Save editor limits: Unreal saves from UE 5.4+ open as hex; Unreal text values, engine structs,
  maps and sets are read-only; files over 64 MB aren't opened. XML in UTF-16 opens as hex. YAML
  with flow collections over several lines, or plain text over several lines, opens as hex.
  Bethesda saves: only the summary (no edits); Fallout 3 / New Vegas / Oblivion saves open as hex.
  Godot `store_var` binary saves, GameMaker `ds_map` hex strings and RPG Maker XP-VX Ace (Ruby
  Marshal) saves open as hex. None of the 1.10.0 formats was tried on a real game's save yet.
- PlayerPrefs floats are assumed to be REG_QWORD doubles (Unity's Windows format); a game that
  stores numbers differently shows them as raw whole numbers.
- Wrapper detection only handles a checksum over the whole rest of the file; seeded or partial
  checksums, and encryption, aren't detected (encrypted saves open as hex and are not decrypted).
- Every attached program is added to Save backups (it can be removed there). Its saves are backed
  up at attach only if a save folder was found.
- Steam Cloud: JuiceBox Trainer can't stop Steam from syncing; it warns and tells you to keep the local
  copy if Steam asks.
- Account copy: only finds saves whose folder path names the account; a game that keeps one save
  file per account in a shared folder (`save_<id>.dat` next to each other) isn't offered. Short
  binary account ids (4 bytes) and ids in encrypted saves aren't changed. Steam Cloud may not
  upload copied files until the game saves once with that account. Copying between PCs isn't done.
- Backups of files the game has locked are listed as "couldn't be read" and the rest is backed up.
- The user isn't a developer: test instructions should be double-click-first (see
  `Try JuiceBox Trainer.cmd`), not terminal-first.
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
