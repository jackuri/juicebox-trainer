# JuiceBox Trainer — Product & Technical Plan (v0 draft)

> Working name. One free, open-source Windows app that combines a **WeMod/Wand-style game trainer** with a
> **Save Wizard-style save editor** for **PC games without anti-cheat** (single-player, co-op on a
> private server or LAN, and online games without anti-cheat, the owner's decision 2026-11-12), plus a built-in **AI agent that creates
> new trainers and save cheats for games that don't have them yet**. No paywall, no queues, no time limits,
> works offline once a game's cheat pack exists.

---

## 1. Goals

1. **Trainer (WeMod/Wand side):** detect installed games, one-click "Play with cheats", toggles / sliders /
   buttons, global hotkeys, controller hotkeys, a phone remote, presets, auto-apply on launch.
2. **Save editor (Save Wizard side):** find any game's saves, back them up automatically, edit them in a
   structured view (not just hex), apply "quick codes" (e.g. max money), compare two saves, restore any
   snapshot.
3. **AI creator:** for a game with no cheat pack, an agent works with the user through plain-English steps
   ("Note your ammo. Now fire one shot.") and produces a stable, version-tagged cheat pack. When a game
   updates and a cheat breaks, the agent repairs it ("self-healing").
4. **Better than both:** every feature free and unlimited; transparent (every cheat shows exactly what it
   changes); offline-first; packs are plain files the user owns and can share.


`universal-modder`'s `skills/mod-any-game/references/safety.md`, which the AI agent must load.

---

## 2. Feature list (by area)

### 2.1 Library & launch
- Auto-detect Steam / Epic / Xbox / GOG / folder installs (`um scan --list`, plus manual "Add folder").
- Per-game card: engine, version, exe SHA-256, anti-cheat status, save folders, pack status
  (✅ verified pack / 🧪 AI-made pack / ➕ "Create with AI").
- "Play with cheats": launch via store URI or exe, wait for process, attach, auto-apply chosen preset.
- Attach to an already-running game (process picker with icons).

### 2.2 Trainer
- Cheat types: **toggle**, **slider** (min/max/step), **number input**, **button** (one-shot, e.g. "+1000 gold").
- Global hotkeys (default `F1…F12`, `Ctrl+Num` combos), fully rebindable, conflict detection.
- Controller hotkeys (XInput: hold-button chords like `LB+RB+DPadUp`).
- **Overlay:** a separate always-on-top, click-through transparent window (no injection into the game). Works
  in windowed/borderless; documented limitation for exclusive fullscreen.
- **Phone remote:** local LAN web UI served by the app, paired by QR code + one-time token, off by default.
- Presets per game ("Story mode", "Speedrun practice"), favourites, notes per cheat.
- Status line per cheat: active / failed (signature not found) / needs repair → "Fix with AI".
- Built-in manual tools for power users: value scanner (exact, unknown, changed/unchanged, increased/decreased,
  float tolerance), address list, freeze, pointer scanner, AOB signature maker, memory viewer.

### 2.3 Save manager & editor
- **Locator:** `um scan` save hints + known patterns (`Documents/My Games`, `%APPDATA%`, `%LOCALAPPDATA%`,
  `AppData/LocalLow/<Company>/<Game>` for Unity, `Saved/SaveGames` for Unreal, `Saved Games/`,
  `Steam/userdata/<id>/<appid>/remote`), Unity PlayerPrefs in the registry, user override.
- **Backups:** automatic snapshot on game launch and on game exit, manual snapshots with notes, restore,
  retention policy. Reuse `um backup` semantics (create / diff / restore) or port them to Rust.
- **Steam Cloud awareness:** detect cloud-synced saves; only edit while the game is closed; warn that cloud
  can overwrite edits; offer "edit then launch immediately" flow.
- **Codecs (format plugins), safe parsing only:**
  - Plain JSON / XML / INI / YAML / CSV
  - gzip / zlib / deflate / LZ4 auto-detect wrappers, Base64 wrappers
  - Unreal **GVAS** `.sav` (property tree view)
  - Unity **PlayerPrefs** (registry) and common Unity JSON saves
  - SQLite
  - .NET BinaryFormatter and Python pickle (Ren'Py): **parse the byte structure without ever deserializing /
    unpickling** (both formats execute code when deserialized)
  - Fallback: hex editor with typed-value inspector
  - Later: Bethesda `.ess`, RPG Maker, GameMaker ini/json, Godot `.save`
- **Integrity handling:** detect simple checksums/hashes (CRC32, MD5, SHA-1 trailer) and recompute them after
  edits. This is fixing your own file, not breaking protection; if a save is encrypted with a key we'd have to
  extract from a protected binary, stop.
- **Save diff assistant:** snapshot → do one thing in-game ("buy something") → snapshot → highlight changed
  fields → AI labels them ("this is gold, this is the potion count").
- **Quick codes:** named save patches against a codec path (`player.money = 9999999`) bundled in cheat packs.
- **Account transfer:** move your own saves between your own Steam accounts/PCs, re-writing an embedded
  SteamID where a game stores one.

### 2.4 AI creator ("Make a trainer")
See §5. Entry points: "Create with AI" on a game card, "Fix with AI" on a broken cheat, "Explain this save".

### 2.5 Packs
- Plain-file cheat packs (§4), stored in `%APPDATA%/JuiceBox Trainer/packs/<game-id>/`.
- Import/export `.juice` (zip). Optional community index later as a GitHub repo of packs (no game data in
  packs — only signatures, offsets, codec paths, metadata).
- Badges: verified (tested on this exact exe hash) / AI-made / community / broken-on-this-version.

---

## 3. Architecture

```
juicebox-trainer/
├─ apps/desktop/                 Tauri 2 app (React + TypeScript + Tailwind UI)
│  └─ src-tauri/                 Rust shell: commands, window mgmt, overlay window, tray, hotkeys
├─ crates/
│  ├─ jb-core/                   process enum/attach, memory read/write/query, scanner, pointer scanner,
│  │                             AOB search, freeze loop, patch engine, x86/x64 assembler (iced-x86),
│  │                             module map, exe hashing, ANTI-CHEAT GUARD
│  ├─ jb-packs/                  pack schema (serde + JSON Schema), loader, validator, resolver
│  ├─ jb-saves/                  locator, snapshots, codecs (plugin trait), checksum fixers, diff
│  └─ jb-mcp/                    MCP server (stdio) exposing jb-core + jb-saves as GUARDED tools for the AI
├─ sidecars/
│  └─ ai-agent/                  Node/TS: Claude Agent SDK; connects jb-mcp + REA MCP; shells out to `um`
├─ testbed/                      fake games for CI (see §7)
├─ packs/examples/               example packs targeting testbed games
└─ docs/
```

**Why Tauri + Rust:** small install, native speed for scanning gigabytes of memory, Rust's `windows` crate for
`OpenProcess`/`ReadProcessMemory`/`WriteProcessMemory`/`VirtualQueryEx`/`VirtualAllocEx`, and a web UI that's
fast to iterate. REA is Node and `um` is Python, so they run as sidecars rather than being linked in.

**Key rule:** the AI never gets raw process access. Everything it does to a game goes through `jb-mcp`, which
calls the same `jb-core` functions as the UI, through the same anti-cheat guard and the same logging. AI output
is **declarative pack data**, never arbitrary native code executed by the app.

### 3.1 Anti-cheat guard (jb-core)
Before any attach/write, block if any of these are true:
- A known anti-cheat process/service/driver is running or loaded in the target (EasyAntiCheat, BEService /
  BattlEye, vgc / vgk, EA Javelin, Ricochet, ACE, nProtect GameGuard, XIGNCODE, mhyprot, etc.).
- A check could not run (fail closed).
- For changing files of a game that isn't running (save restore/edit): anti-cheat files in the install.
Anti-cheat files in the install of a *running* game are a note, not a block (from 1.7.0): games with an
official no-anti-cheat mode (e.g. Halo MCC "Anti-Cheat Disabled") run with none of it active. The
session re-checks every 5 s and blocks for good if it starts. 
When blocked: show why, offer save backups only (no save edits for online-progression games). There is no
setting, flag or env var that turns this off.

### 3.2 Patch engine (declarative ops only)
- `value`: resolve address (pointer chain or AOB+offset or managed path) → read/write/freeze typed value.
- `patch`: AOB-located byte replacement (e.g. NOP a decrement); original bytes stored; toggle off restores.
- `cave`: allocate code cave near module, assemble a short snippet with iced-x86, write a jump; validated
  (bounded length, only whitelisted instruction classes, no syscalls/calls outside the game module).
- Every op verifies the original bytes before writing and refuses on mismatch (version drift → "needs repair").
- Restore all patches on detach / app exit / crash handler.

### 3.3 Engine fast paths (Phase 5+)
Driven by `um scan` engine detection and universal-modder's engine playbooks:
- **Unity Mono:** resolve class/field by name through the Mono runtime's exported functions → stable across
  updates.
- **Unity IL2CPP:** REA decodes `global-metadata.dat` (v24–v39) for field offsets.
- **Unreal 4/5:** reflection (GObjects/GNames) → property paths like `PlayerCharacter.Health`.
- **.NET / XNA / FNA:** static metadata via REA.
- **Native:** classic scan → what-writes → AOB → code patch, with REA/Ghidra for static confirmation.

---

## 4. Cheat pack format (v1)

`packs/<game-id>/pack.json` (JSON Schema lives in `crates/jb-packs/schema/pack.v1.json`):

```json
{
  "schema": 1,
  "game": { "id": "testbed-native", "name": "Testbed Native", "engine": "native",
            "exe": "testbed_native.exe",
            "builds": [{ "sha256": "…", "label": "1.0", "verified": true }] },
  "meta": { "author": "ai", "created": "2026-11-06", "packVersion": "1.0.0", "notes": "" },
  "cheats": [
    { "id": "inf_health", "name": "Infinite Health", "category": "Player", "ui": "toggle",
      "hotkey": "F1",
      "impl": { "kind": "patch", "module": "testbed_native.exe",
                "aob": "29 47 10 8B 47 10", "offset": 0,
                "original": "29 47 10", "replace": "90 90 90" } },
    { "id": "gold", "name": "Set Gold", "category": "Inventory", "ui": "number",
      "impl": { "kind": "value", "type": "i32",
                "address": { "base": "testbed_native.exe+0x2A1F0", "offsets": [ "0x18", "0x40" ] } } },
    { "id": "speed", "name": "Move Speed", "category": "Player", "ui": "slider",
      "slider": { "min": 0.5, "max": 5, "step": 0.1, "default": 1 },
      "impl": { "kind": "value", "type": "f32",
                "address": { "aob": "F3 0F 10 86 ?? ?? ?? ?? F3 0F 59", "deref": "rip+4" } } }
  ],
  "saveCodes": [
    { "id": "max_money", "name": "Max money", "codec": "json",
      "file": "{localappdatalow}/TestCo/Testbed/save1.json",
      "set": { "path": "player.money", "value": 9999999 } }
  ]
}
```

Rules: packs contain no game bytes beyond short signatures; every cheat tagged to build hashes it was verified
on; unknown fields rejected by the validator.

---

## 5. AI creator design

### 5.1 Backend options (user picks in Settings)
1. **Claude subscription via the Claude Agent SDK** (signed-in Claude Code on the PC). Note: since mid-2026,
   Agent SDK / `claude -p` usage on subscription plans draws from a separate monthly Agent SDK credit.
2. **Anthropic API key** (pay-as-you-go).
3. **Local model** (Ollama / LM Studio, OpenAI-compatible endpoint) — free, lower success rate; uses the same
   tools.
The app itself has no paywall; AI costs are whatever the user's chosen backend costs (zero for local).

### 5.2 Tools the agent gets
- `jb-mcp` (guarded): `list_processes`, `attach`, `scan_first`, `scan_next`, `read`, `write_test`,
  `find_what_writes` (hardware breakpoint, single-player only), `make_aob`, `pointer_scan`, `verify_aob_unique`,
  `apply_cheat`, `revert_all`, `save_snapshot`, `save_diff`, `codec_read`, `pack_write`, `pack_validate`.
- **REA MCP** (static analysis: Ghidra/PE/IL2CPP/.NET) — optional; app must work without it.
- **`um` CLI**: `um scan`, `um kb search`, `um backup`.
- **universal-modder skills** as reference docs: `game-recon`, `mod-any-game` (+ `references/safety.md`,
  engine playbooks), `reverse-engineering`.

### 5.3 "Make a trainer" flow
1. **Recon:** `um scan --json` → engine, version, exe hash, anti-cheat, saves. Guard decides go / no-go.
2. **Wishlist:** user ticks common cheats (infinite health/ammo/stamina, money, XP, god mode, speed, no
   reload, freeze timer) or types their own in plain English.
3. **Guided discovery:** UI shows one plain step at a time with a big "Done" button; the agent drives scans
   between steps. Unknown-value fallback ("Did it go up or down?").
4. **Stabilize:** `find_what_writes` → instruction → AOB signature (verified unique) and/or pointer chain
   (verified across a game restart). Optional REA static check of the surrounding function.
5. **Build cheat:** prefer engine fast path > code patch at the write site > pointer value > raw address.
6. **Test:** apply, ask user to confirm in-game, auto-check value behaviour, revert, re-apply after restart.
7. **Save:** write pack, tag build hash, mark "AI-made", show diff of what each cheat touches.

### 5.4 Self-healing
On game update (exe hash changed): re-run each AOB; for misses, agent searches for the nearest similar
signature / re-derives via REA diff of old vs new function, re-tests, bumps pack version.

### 5.5 Agent guardrails
- System prompt includes `safety.md`; anti-cheat guard enforced in code anyway.
- Agent may only write to game memory through `jb-mcp`; every write logged and revertible.
- Back up saves before the first cheat is ever applied to a game.
- Show token/cost estimate before long runs; cancel button always visible.

### 5.6 Making the helpers better at finding cheats (ideas, 2026-11-10; not scheduled)
Proposed to the user after 1.20, in the order recommended:
1. **Measure first:** a benchmark of testbed games (native x64 / x86, Unity-like floats, doubles,
   pointer chains, values stored ×10 or XOR-ed, web JS and WebAssembly) with a simulated player
   that answers the helper from the testbed's own state. Score cheats found, questions asked,
   cost and time, so every prompt or tool change is measured, not guessed.
2. **Let the helper see the game:** a read-only `game_screenshot` tool (the game's window only),
   so it reads numbers and checks "did it work?" itself instead of asking the player each time.
3. **More from each find:** `read_struct` (the values next to one it found, typed: max health,
   stamina and ammo often sit together), and finding live objects by class for Unity Mono like
   `unreal_find` does for Unreal (names → values without scanning).
4. **Reuse what's known:** a `known_cheats` tool over the community list, the player's packs and
   field notes (engine, value types, what worked), and a short note saved after each session.
5. **Checks in code:** `pack_write` / `web_cheats_save` refuse cheats never tried in the session;
   a cheaper model for the mechanical narrowing steps; a smaller tool set for local models.

---

## 6. Roadmap (every update bumps the version)

| Version | Phase | Delivers |
|---|---|---|
| 0.1.0 | Skeleton | Monorepo, Tauri app boots, Rust workspace, CI (fmt, clippy, tests), testbed games build |
| 0.2.0 | Core memory | Process list, attach/detach, read/write typed values, module map, anti-cheat guard + tests |
| 0.3.0 | Scanner | Exact/unknown/changed scans (multithreaded), address list, freeze loop, manual-tools UI |
| 0.4.0 | Packs | Pack schema + validator, value/patch ops, hotkeys, trainer UI, example packs vs testbed |
| 0.5.0 | Stability | Pointer scanner, AOB maker, original-byte verification, restore-on-exit (crash-safe journal). Code caves moved to a later version |
| 0.6.0 | Saves I | Locator, snapshots on launch/exit, restore, diff, Steam Cloud warnings |
| 0.7.0 | Saves II | Codecs (JSON/INI/gzip/zlib/Base64/GVAS/PlayerPrefs), structured editor, hex fallback, checksum fixers, quick codes. SQLite moved to a later version (large dependency; user chose to wait; done in 1.13.0) |
| 0.8.0 | AI I | ai-agent sidecar, ot-mcp, guided "Make a trainer" for native games on testbed. `find_what_writes` and local models moved to 0.9.0 |
| 0.9.0 | AI II | `find_what_writes` (hardware breakpoints) and code-patch cheats, self-healing packs, engine recon (+ optional `um scan`), AI save diff labelling |
| 0.9.1 | AI III | Unity Mono / .NET class and field names (`game_classes`), optional REA analysis server, local models (Ollama / LM Studio) |
| 0.9.2 | AI IV | Unity IL2CPP metadata names (self-calibrating, formats 24–39), Unreal engine version and tips, anti-cheat note |
| 0.9.3 | AI V | Unreal object/property reader (GNames / GObjects) in a running game, with a memory-search fallback; tested on a real single-player UE 5.7 game with the user's OK |
| 0.10.0 | Extras | Overlay window, phone remote, controller hotkeys, presets (+ favourites, notes, own keys), library auto-detect (Steam / Epic / GOG / Xbox / own) + "Play with cheats". `um scan --list` not used yet |
| 1.0.0 | Release | Installer (NSIS, per-user, `.otpack` file association), signing-ready release workflow (unsigned until a certificate exists), auto-update (off until the repo is public), user / developer / release docs, `.otpack` import/export, community pack list (GitHub repo index), AI engine downloaded on first use. The repo stays private for now (user's choice) |
| 1.1.0 | Polish | "Browse…" pickers for games and save folders, plain `pack.json` import, auto-load the only pack on attach, "Find pointers" from scanner results, "Copy as pack cheat" for pointer paths. Added after 1.0.0 from STATUS follow-ups (no new dependencies) |
| 1.2.0 | Look | New look in the style of modern trainer apps (user's request: "like Wand / WeMod"): sidebar, navy + blue-violet theme, cheat cards with big switches and key caps, game cards with generated covers, new icon colours. Own design; no other app's branding or art |
| 1.30.0 | Live AI fixes | User's idea 2026-11-12, after testing instant cheats on Species: Unknown (ammo only for the starting gun; a flashlight battery with no number couldn't be found by hand). `ai_start` `live` (pack key, cheat id, problem): the app keeps the game (no `shutdown`), turns only that cheat off, copies a bundled pack to the user dir, and sends the sidecar `live` (pack file, cheats on, fixing). Sidecar `LIVE_NOTE` + live kickoff; `jb-mcp` `LiveFix` (`JB_LIVE_PACK`, `JB_LIVE_ON`): `pack_try` refuses cheats that are on, `pack_write` replaces/adds cheats in the player's pack (author kept, `.bak`), `known_cheats.playingNow`. The helper keeps its own journal (`<pid>-<start>-helper.json`, `enable_journal_as`; recovery uses the file it finds). After exit `live_fix::finish` waits for that journal to go, reloads the pack if it changed and turns the cheats back on (`liveDone` event). UI `LiveHelper` (Cheats: Ask the AI, Fix with AI per cheat; cheat book routes there). Finder: unknown snapshots also read as f64 at 8-aligned slots (`Scan::next_as` widening), unknown-scan limit follows free memory (4–24 GiB) |
| 1.29.0 | Pack ratings | User's idea 2026-11-12 (1 broken, 2 works with bugs, 3 works great). Community repo: `scripts/build-ratings.mjs` + workflow `ratings.yml` (hourly, push of index.json, dispatch; issues: write) make a "Rate: <pack id>" issue (label `rating`) for every pack and count reactions (+1 = 3, confused = 2, -1 = 1; newest per account, bots ignored) into `ratings.json` (issue, count, stars, votes). `jb_packs::community` `Rating`, `ratings_url`, `parse_ratings` (lenient: sums must match, stars 1–3); app `community_ratings` (cached 10 min), `community_rate` (`github::rate`: only an open "Rate: " issue labelled `rating`; deletes the player's other rating reactions, adds the new one; GitHub sign-in as for sharing); Packs: stars on community rows, Best rated first, Rate it on installed community packs |
| 1.28.0 | Cheat book | User's idea 2026-11-12. App `cheat_book.rs`: `book_open` (pack by key + the game's notes / favourites / keys from settings, `loaded`), `book_save` (`apply_edit`: kept cheats are the originals with only name, category, description and their control's config taken from the edit; `impl` and `ui` never change, no new cheats; validated; written in place with `pack.json.bak`, a bundled pack as `<user dir>/<game.id>` copy; `edited_settings` via `set_note` / `set_favourite` / `set_key`, presets and pads drop removed cheats; a loaded pack is loaded again). UI `CheatBook` (Packs row, Games card, Cheats header), `lib/cheatBook` (`toEdit`, `describeCheat`, `fixRequest`); AiView `fix` prefills the wish, the note and the running game |
| 1.27.1 | Web finder blank-screen fix | User's report 2026-11-12 (Outbreak): a search result whose value stopped being a number came back as NaN → JSON null → `toFixed` on null crashed React and blanked the window. Page helper `summary` leaves `value` out when it isn't a finite number (and skips a hit it can't describe); `WebCheatsPanel` shows only real numbers and ignores malformed refreshes; `ErrorBoundary` around every view (Try again, keyed by view). Testbed web game: rockets on a weapon object and a Swap button that empties them (reproduced in the real 1.27.0 app, fixed in 1.27.1) |
| 1.27.0 | More cheat table sources | User's choice 2026-11-11 (idea 3 of 3: link with credit; open-licence tables included with credit; never copy WeMod / FLiNG / Cheat Happens; never re-host without permission). `tables_search` sites `cheatengine` / `opencheattables` (DuckDuckGo `site:`), `github` (repository search), `reddit` (r/cheatengine); `TableLink` `licence` / `file` (`tables/<id>.CT`) / `sha256`, `HOSTING_LICENCES` (CC-BY-4.0, CC0-1.0, CC-BY-SA-4.0, MIT, Apache-2.0, GPL-3.0-or-later, the maker's permission), `MAX_TABLE_FILE` 2 MB, `table_file_url`, `check_table_file`; app `community_get_table` (downloaded to `%LOCALAPPDATA%\JuiceBox Trainer\tables`, hash-checked, then the usual Turn into cheats with by / url / licence credit), `community_share_table` `file` + `licence` (`with_table_file`: open licence, readable table with cheats, same game; `send_files` puts both files in one pull request), `pick_table`; community repo: builder and check accept `tables/<id>.CT` next to its link |
| 1.26.0 | Instant engine cheats: Unreal | User's choice 2026-11-11 (idea 1 of 3, Unreal first; Unity next). `jb_core::engine_cheats` (`Graph` trait, implemented by `unreal::Reader`; `find_unreal`): the engine object's global in the exe, then by property name GameInstance → LocalPlayers[0] → PlayerController → Pawn → CharacterMovement (MaxWalkSpeed, JumpZVelocity, GravityScale), Character.JumpMaxCount, GameViewport → World → PersistentLevel → WorldSettings (property or found in the level) → TimeDilation; player values by usual names (health, stamina, mana, ammo with their max; money set once) on the pawn, objects inside it or owned by it, and the player state, never through pointers to other actors. `jb_packs::engine` builds an ordinary pack (`engine-<exe>`, pointer paths from `exe+global`, F-keys for toggles, held sliders). App `engine_cheats_support` / `engine_cheats_make` (blocking read off the session lock), Cheats tab card. Trainer: a frozen pointer path that fails to resolve pauses its freeze until it resolves again (testbed `despawn` / `respawn`) |
| 1.25.0 | Community growth: share nudge and Most wanted | User's choice 2026-11-11 (idea 2 of 3: then engine-wide cheats, then more table sources). AI view: after `finished ok`, the user pack whose exe matches the target (or the web game) → ShareDialog / WebShareDialog. Community repo: issue form `cheat-request.yml` (label `request`), `scripts/build-wanted.mjs` + workflow `wanted.yml` (issues events + hourly) write `wanted.json` (game, exe, votes = 1 + 👍, number; text cleaned, links from the number). `jb_packs::community` `parse_wanted` (lenient, cleaned, sorted, ≤ 200), `request_url` (prefilled form), `request_page`; app `community_wanted` (cached 10 min), `community_ask`, `community_open_request` (through `browser::open_url`); Games: Ask the community / add your vote; Packs: Most wanted with "make cheats with AI" for owned games |
| 1.24.1 | Taskbar icon fix | User's report: the holiday look showed in the app but not on the taskbar. Tauri `set_icon` sends only `WM_SETICON ICON_SMALL`; `mascot.rs` also sends `ICON_BIG` (PNG via `CreateIconFromResourceEx`, kept until replaced); moods embedded at 64×64 |
| 1.24.0 | Juice box faces | User's pictures 2026-11-11 (cut out of their backgrounds, floating details kept). `lib/mascot.ts`: `seasonOf` (Feb valentine, Oct halloween, Nov thanksgiving, Dec christmas), `juicebox.seasonal` switch in Themes, `useSeason` (event + hourly); `Logo` follows the season, `Mascot` moods `wink` (AI finished ok, pack message, save restored) and `error` (ProblemCard); `mascot.rs` `app_icon_set` (whitelisted embedded PNGs, tauri `image-png`) sets the window/taskbar icon; everyday app icon = the smile |
| 1.23.0 | Rename to JuiceBox Trainer | User's choice 2026-11-11 (JUICE: Jackuri's Ultimate In-game Cheat Engine; JUCE_BOY was considered: JUCE is an existing framework's trademark and "Boy" leans on Nintendo's). `jb_core::names` (name, folders, id; `move_legacy_folders` on GUI start: `%APPDATA%\OpenTrainer` renamed or merged, `%LOCALAPPDATA%\OpenTrainer` entry by entry minus the old program files, `org.opentrainer.desktop` folders; never overwrites or deletes); GitHub sign-in moved in Credential Manager; NSIS `installerHooks` run OpenTrainer's uninstaller silently (it deletes only its program files); `.juice` / `.juiceweb` written, `.otpack` / `.otweb` read and `.otpack` still associated; crates `jb-*`, `juicebox-trainer.exe`, `juicepack`, `JB_*`, MCP server `jb`; icon from the user's picture (cut out of its background); release file `JuiceBoxTrainer_<v>_x64-setup.exe`. Kept on purpose: localStorage keys `opentrainer.*`, the pack schema id, the `otpack.json` manifest inside packs, `.opentrainer-release-keys` |
| 1.22.0 | Clear problem messages | User's pick 2026-11-11 (idea 5 of the simplicity list). `problems.rs`: `%LOCALAPPDATA%\OpenTrainer\problems.json` (last 30, `problem` event, Help page); game closed: `AttachedProcess::exit_code` / `exit_time`, exit codes in plain words, Windows' "Application Error" 1000 record via `EvtQuery` (module, code, offset) a few seconds later, cheats on (`Trainer::active_cheats`, frozen list entries), the session's last change (toggles, sets, keys, phone); `settle` attaches the report to `processExited` errors and a 2 s watcher notices exits on any screen; `page_watch.rs`: WebView2 `ProcessFailed` (webview2-com 0.39.1, already in the tree) for the web game window (report + `web_reload`) and the main window (report + reload); panics recorded; `lib/explain.ts` plain errors in player screens; `browser.rs` `ShellExecuteW` for https links (Explorer opened Documents for FearLess searches); known table page as the main button. A FearLess topic finder was looked at: FearLess search sits behind a bot check and search engines refuse honest automated clients, so not done |
| 1.21.0 | Smarter AI helpers | User's go-ahead 2026-11-10 (all five ideas of §5.6). `ot_core::screen` (biggest visible window of the pid, `PrintWindow` with full content, screen copy when black, box-scaled to ≤ 1280 px, RGB PNG); ot-mcp `game_screenshot` (MCP image content), `read_struct` (rows of i32 / f32 / f64 / pointer with hints), `known_cheats` (installed packs for the exe, the cached community list, notes), `note_save` (≤ 600 characters, 10 per key, `exe:` / `engine:` / `site:` keys); web `web_screenshot` (link `screenshot`: the game window's HWND), `web_near` (page helper v4 `near`); tried checks in `pack_write` and `web_cheats_save`; `OT_MCP_TOOLS` limits listed tools (web, local models with `LOCAL_PROMPT`); the app keeps the downloaded community list in `%LOCALAPPDATA%` for `known_cheats`; AI screen: pictures on/off, Opus or Sonnet; `scripts/ai-bench.mjs` (simulated player on testbed_native, checks on a fresh game). Live Unity Mono objects (like `unreal_find`) left out: no Unity game to test on |
| 1.20.0 | AI helper for web games | User's go-ahead 2026-11-10. App `web_link.rs`: while a `webTrainer` session runs, a local link (127.0.0.1, new port, 128-bit token from `remote::token`, one request per connection, 1 MB cap) passes on only the page helper's `info` / `names` / `find` / `next` / `results` / `read` / `set` / `lock` / `unlock` (locks only with `ai:` ids) plus `game` and `saveCheats` (checked `WebCheat`s merged by place, `web-cheats-changed` event), and only while the session's game is the open one; it stops when the helper exits. `ot-mcp` `web` module: `web_game`, `web_names`, `web_find`, `web_next`, `web_results`, `web_read`, `web_set`, `web_lock`, `web_unlock`, `web_cheats_save` (listed only with a link; `OT_WEB_LINK` / `OT_WEB_TOKEN`), every set / lock recorded and undone by `revert_all` or exit. Sidecar: task `webTrainer` with `WEB_PROMPT` (names first, then guided search; play-fair scope), only the web tools, link env given to the tool server only. Page helper v3: `names`. UI: Find cheats with AI in a web game's cheats, "What kind of game?" in the AI helper |
| 1.19.0 | Web cheat packs | User's go-ahead 2026-11-10. `ot_packs::web` (WebPack format 1 / `.otweb`, WebCheat with `key` + `keyDoes`, `check_pack` local vs community rules: https and not this PC, ≤ 50 cheats; `host`, `url_problem`); community index `web` (whole packs, read leniently one by one); app: `web_export` / `web_pick_file` / `web_read_file` / `web_install` (merge by place, rename on clash, adds the game by exact address), `community_share_web` (`web/<id>.json` through `send_file`); helper `keys` / `state` calls (capture-phase keydown before the game, closed-shadow note, typing in boxes ignored); Web games tab: Share dialog, Open a cheats file, community list; community repo: builder + check accept `web/`, README rules (no competitive or real-money games). The AI helper for web games moves to 1.20 (needs a bridge from the `--mcp` process to the game window) |
| 1.18.0 | Agreement + web games I | User request 2026-11-09 (web games such as browser co-op with invited friends; warning + liability waiver). `agreement.rs` (signed name + date + version; `AgreementGate` before the app; `web_open` / `set` / `lock` need it); `web.rs` + `web_agent.js` (WebviewWindow `webgame`, external URL, agent via `initialization_script_for_all_frames`, calls via `eval_with_callback`, whitelisted methods, JSON-quoted args): WebAssembly.Memory / Instance / instantiate hooks + big byte arrays, JS value walk without getters, find / next / set / lock, saved web cheats per game; Web games tab; `testbed/web`. No IPC for the game window (not in capabilities). Next 1.19: web cheat packs (sharing, hotkeys, AI helper) |
| 1.17.0 | Creator invitations | Plan step 3 (user's choice 2026-11-09: makers choose their terms). `meta.source` + `licence` / `support` (SUPPORT_SITES) / `fromMaker`; `community_share` `terms` (`with_terms`, PR text lists terms, credit, support, maker claim for maintainers); community index entries carry `source`; community rows show credit, terms, Shared-by-its-maker badge, Maker's page / Support the maker (`community_open_credit`, listed addresses only); `docs/CREATORS.md` with the invitation message; community README: MIT unless the pack names other terms, badge checked by maintainers. Next: step 4, tested AI-made packs for popular games the user owns |
| 1.16.0 | Table link directory | Plan step 2: community index `tables` (links only: id, game, exe, title, by, url, cheats, added); app keeps links that are https on `TABLE_SITES` (fearlessrevolution, forum.cheatengine, opencheattables, github) and drops bad ones; Find a cheat table lists the game's links (exe or exact name) and opens only listed pages (`community_open_table`); Share the table's link (`community_share_table`) sends `tables/<id>.json` through the same pull-request path as packs (`send_file`); the community repo's check and index builder accept `tables/` |
| 1.15.0 | Get cheats from tables | Cheat-library plan step 1 (user's choice 2026-11-09: no bulk copying of other people's tables). Games: Find a cheat table (FearLess / web search opened in the browser, only the encoded game name sent); Downloads watcher (`tables_recent`, top level + one folder deep, 30 days, cached by path / time / size, polled every 5 s) matched by exe → recent search → name; Turn into cheats opens the table for the matched game's exe; pack `meta.source` (by / url / file) credit typed on import. Next: 1.16 link directory in the community list, 1.17 creator invitations (credit badge, licence), then AI-made packs, then code caves (design review) |
| 1.14.0 | Themes | Themes tab with live previews; 10 looks (default unchanged) as CSS variable sets in `themes.css` on `data-theme` (greys, accent, button gradient, glows, font, corner radius, text glow, scanlines; light themes reverse the status colours); saved in localStorage, the overlay follows via the storage event |
| 1.13.0 | SQLite saves | `codec::sqlite` (rusqlite 0.41, bundled + serialize; user's go-ahead 2026-11-09): in-memory deserialize, defensive mode, untrusted schema, triggers and views off, quick_check; cells `Table[rowid].Column`, UPDATE one cell, serialize back (WAL header kept); WITHOUT ROWID / virtual tables skipped, blobs read-only; editor refuses while a non-empty `-wal` exists; `sqlite` save codes; MSRV 1.89 |
| 1.12.0 | Memory viewer | Cheats → Memory: `read_partial` (page by page, unreadable bytes marked), `region_at`, `edit_bytes` (data written, code via `apply_code` journaled + restored, read-only refused, max 64 bytes), commands `memory_view` / `memory_edit`, live 256-byte grid with change highlight, inspector + pointer follow, add to address list, open from the address list |
| 1.11.0 | Account copy | Copy a game's saves between the player's Steam accounts on this PC (`ot_saves::accounts`): accounts from `loginusers.vdf` + `userdata`, account folders by path component (SteamID64, or a known account id with 5+ digits), SteamID64 rewritten as text / LE bytes (inside wrappers) and in file names, account id as text only (7+ digits); preview, guard, game closed, backup first, new folders added to the game. Moving saves between PCs is not done (backups stay per PC) |
| 1.10.0 | Save formats | XML and YAML editing (span-preserving), RPG Maker MV lz-string wrapper (MZ = zlib), GameMaker quoted INI numbers, Godot ConfigFile values (multi-line read-only) + encrypted-save note, Skyrim / Fallout 4 summary (read-only); `xml` / `yaml` save codes |
| 1.9.0 | Find a value | Guided value finder in Cheats (`ot_core::finder`: i32/f32/f64 at once, rounded numbers, unknown start read as i32+f32 from one snapshot via `Scan::next_as`), try-each-place with the real value restored, keep/set now, save as a cheat in a `mine-<exe>` pack (`my_cheat_save`: static address, or pointer scan + restart + filter, path checked against the found address); live Scanner values |
| 1.8.1 | One entry per game | Running-game pickers list each game once (pack exe, else most memory, else biggest exe) |
| 1.8.0 | Hide games | Games tab Hide / Unhide and Show hidden; ids kept in `%APPDATA%\OpenTrainer\hidden-games.json` (`library_set_hidden`) |
| 1.7.0 | Official no-anti-cheat modes | guard judges a running game by what is active; anti-cheat files alone are a note (re-checked every 5 s); Games list and attach message explain it |
| 1.6.0 | Hooked pointers | `ot_core::capture` (hardware execute breakpoint, read one register, optional condition, detach), pack `address.capture`, trainer reads on use / refreshes held values, `ct_script` turns pointer-grab hooks into captures; per-script name scopes |
| 1.5.0 | Smarter tables | `ct_script`: reads table scripts as text (never runs them): Lua RIP pointers and aobscanmodule symbols → aob addresses, define/label numbers and offset math, byte-swap scripts → patch cheats; children of non-number entries |
| 1.4.2 | Hardening | Security review fixes: table number overflow and node limit, sharing permission checked by origin, PR text escaped, install = preview, credential null check, updater without zip, workflow hardening; group-header addresses; crash-reporter filter |
| 1.4.1 | Update button | One-click Update in About (check, download, install, restart; else open the newest release); repository made public so in-app updates can work |
| 1.4.0 | Share with everyone | One-click sharing to the community list: GitHub device-flow sign-in (token in Credential Manager), fork + branch + pack file + pull request done by the app; community repo checks pack-only PRs and rebuilds its index after merges |
| 1.3.0 | Cheat Engine tables | `.CT` import: value entries (module addresses, pointer paths, relative children) become a pack; scripts and other entries listed as skipped, never run; choose the exe when the table doesn't say; AI helper takes typed answers and messages while it works |
| 1.2.3 | Game lists | Running games by name in Cheats, AI and Save backups; other programs behind "Show all running programs" |
| 1.2.2 | Fixes | Covers in the installed app (CSP + byte format), auto-load memory across tabs, Xbox logo path check, cover refresh |
| 1.2.1 | Covers | Game cards show real cover art from files already on the PC (Steam's library cache, GOG icon, Xbox logo, else the exe icon); nothing downloaded or saved |

---

## 7. Testing strategy
- `testbed/native` (C++ or Rust): window/console game with health, ammo, gold, float speed, a pointer chain
  (heap object → player → stats), and a rebuild flag that shifts offsets (to test AOB self-healing).
- `testbed/unity-like` / `testbed/dotnet`: a .NET console game with class fields (managed path tests).
- `testbed/saves/`: sample saves for each codec (JSON, zlib JSON, GVAS sample generated by our own writer,
  SQLite, CRC-trailer binary).
- Unit tests for scanner, AOB, pointer resolver, codecs (round-trip byte-identical when unedited), guard.
- Integration tests: launch testbed → load pack → toggle → assert value via a side channel the testbed exposes.
- Never test against real commercial games in CI.

---

## 8. Decisions (confirmed by the user, 2026-11-07, before v0.8.0)
1. Final name: **JuiceBox Trainer** (kept).
2. Default AI backend: **Claude subscription via the Claude Agent SDK** (signed-in Claude Code). API key and
   local models remain options in Settings.
3. Community pack hub: **a GitHub-repo index** (public repo listing packs; the app downloads from it). Built
   with `.juice` import/export (v1.0 or earlier).
4. Exclusive-fullscreen overlay: **accept the limitation** (windowed/borderless only; no in-process overlay).

## 9. Notes on the two source repos
- **universal-modder** (MIT): skills + `um` CLI (Python). Most useful parts: `um scan` (engine, anti-cheat,
  save hints, JSON), `um backup`, `um kb`, the engine playbooks and `safety.md`. Its scope rules match ours.
- **REA** (MIT): MCP server for static reverse engineering (Ghidra/Hopper/IDA bridges, PE, IL2CPP metadata,
  .NET). Its maintainers' DISCLAIMER says they don't support game-cheat development, so: use it as an optional
  analysis backend, keep it swappable, don't ask its community for trainer help, and include its license
  notice.
