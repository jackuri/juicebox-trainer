# OpenTrainer — Product & Technical Plan (v0 draft)

> Working name. One free, open-source Windows app that combines a **WeMod/Wand-style game trainer** with a
> **Save Wizard-style save editor** for **PC single-player games**, plus a built-in **AI agent that creates
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

### Non-goals (permanent — these are design constraints, not "later")

- **No online / multiplayer games.** No aimbots, ESP/wallhacks, speed/teleport for multiplayer, packet edits.
- **No anti-cheat bypass, no DRM bypass, no HWID spoofing.** If anti-cheat is present, the trainer will not
  attach. Period.
- **No antivirus evasion.** Memory-editing tools get flagged by Defender; we handle that with code signing and
  clear docs, never with obfuscation or packing.
- **No redistribution of game files**, extracted assets, decompiled code, or unpacked EXEs.

This mirrors `universal-modder`'s `skills/mod-any-game/references/safety.md`, which the AI agent must load.

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
- Plain-file cheat packs (§4), stored in `%APPDATA%/OpenTrainer/packs/<game-id>/`.
- Import/export `.otpack` (zip). Optional community index later as a GitHub repo of packs (no game data in
  packs — only signatures, offsets, codec paths, metadata).
- Badges: verified (tested on this exact exe hash) / AI-made / community / broken-on-this-version.

---

## 3. Architecture

```
opentrainer/
├─ apps/desktop/                 Tauri 2 app (React + TypeScript + Tailwind UI)
│  └─ src-tauri/                 Rust shell: commands, window mgmt, overlay window, tray, hotkeys
├─ crates/
│  ├─ ot-core/                   process enum/attach, memory read/write/query, scanner, pointer scanner,
│  │                             AOB search, freeze loop, patch engine, x86/x64 assembler (iced-x86),
│  │                             module map, exe hashing, ANTI-CHEAT GUARD
│  ├─ ot-packs/                  pack schema (serde + JSON Schema), loader, validator, resolver
│  ├─ ot-saves/                  locator, snapshots, codecs (plugin trait), checksum fixers, diff
│  └─ ot-mcp/                    MCP server (stdio) exposing ot-core + ot-saves as GUARDED tools for the AI
├─ sidecars/
│  └─ ai-agent/                  Node/TS: Claude Agent SDK; connects ot-mcp + REA MCP; shells out to `um`
├─ testbed/                      fake games for CI (see §7)
├─ packs/examples/               example packs targeting testbed games
└─ docs/
```

**Why Tauri + Rust:** small install, native speed for scanning gigabytes of memory, Rust's `windows` crate for
`OpenProcess`/`ReadProcessMemory`/`WriteProcessMemory`/`VirtualQueryEx`/`VirtualAllocEx`, and a web UI that's
fast to iterate. REA is Node and `um` is Python, so they run as sidecars rather than being linked in.

**Key rule:** the AI never gets raw process access. Everything it does to a game goes through `ot-mcp`, which
calls the same `ot-core` functions as the UI, through the same anti-cheat guard and the same logging. AI output
is **declarative pack data**, never arbitrary native code executed by the app.

### 3.1 Anti-cheat guard (ot-core, non-bypassable)
Before any attach/write, block if any of these are true:
- `um scan --json` reports `anti_cheat` non-empty for the install.
- A known anti-cheat process/service/driver is running or loaded in the target (EasyAntiCheat, BEService /
  BattlEye, vgc / vgk, EA Javelin, Ricochet, ACE, nProtect GameGuard, XIGNCODE, mhyprot, etc.).
- The game is on the built-in online-only / live-service list.
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

`packs/<game-id>/pack.json` (JSON Schema lives in `crates/ot-packs/schema/pack.v1.json`):

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
- `ot-mcp` (guarded): `list_processes`, `attach`, `scan_first`, `scan_next`, `read`, `write_test`,
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
- Agent may only write to game memory through `ot-mcp`; every write logged and revertible.
- Back up saves before the first cheat is ever applied to a game.
- Show token/cost estimate before long runs; cancel button always visible.

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
| 0.7.0 | Saves II | Codecs (JSON/INI/gzip/zlib/Base64/GVAS/PlayerPrefs), structured editor, hex fallback, checksum fixers, quick codes. SQLite moved to a later version (large dependency; user chose to wait) |
| 0.8.0 | AI I | ai-agent sidecar, ot-mcp, guided "Make a trainer" for native games on testbed. `find_what_writes` and local models moved to 0.9.0 |
| 0.9.0 | AI II | `find_what_writes` (hardware breakpoints) and code-patch cheats, self-healing packs, engine recon (+ optional `um scan`), AI save diff labelling |
| 0.9.1 | AI III | Unity Mono / .NET class and field names (`game_classes`), optional REA analysis server, local models (Ollama / LM Studio) |
| 0.9.2 | AI IV | Unity IL2CPP metadata names (self-calibrating, formats 24–39), Unreal engine version and tips, anti-cheat note |
| 0.9.3 | AI V | Unreal object/property reader (GNames / GObjects) in a running game, with a memory-search fallback; tested on a real single-player UE 5.7 game with the user's OK |
| 0.10.0 | Extras | Overlay window, phone remote, controller hotkeys, presets (+ favourites, notes, own keys), library auto-detect (Steam / Epic / GOG / Xbox / own) + "Play with cheats". `um scan --list` not used yet |
| 1.0.0 | Release | Installer (NSIS, per-user, `.otpack` file association), signing-ready release workflow (unsigned until a certificate exists), auto-update (off until the repo is public), user / developer / release docs, `.otpack` import/export, community pack list (GitHub repo index), AI engine downloaded on first use. The repo stays private for now (user's choice) |
| 1.1.0 | Polish | "Browse…" pickers for games and save folders, plain `pack.json` import, auto-load the only pack on attach, "Find pointers" from scanner results, "Copy as pack cheat" for pointer paths. Added after 1.0.0 from STATUS follow-ups (no new dependencies) |
| 1.2.0 | Look | New look in the style of modern trainer apps (user's request: "like Wand / WeMod"): sidebar, navy + blue-violet theme, cheat cards with big switches and key caps, game cards with generated covers, new icon colours. Own design; no other app's branding or art |
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
1. Final name: **OpenTrainer** (kept).
2. Default AI backend: **Claude subscription via the Claude Agent SDK** (signed-in Claude Code). API key and
   local models remain options in Settings.
3. Community pack hub: **a GitHub-repo index** (public repo listing packs; the app downloads from it). Built
   with `.otpack` import/export (v1.0 or earlier).
4. Exclusive-fullscreen overlay: **accept the limitation** (windowed/borderless only; no in-process overlay).

## 9. Notes on the two source repos
- **universal-modder** (MIT): skills + `um` CLI (Python). Most useful parts: `um scan` (engine, anti-cheat,
  save hints, JSON), `um backup`, `um kb`, the engine playbooks and `safety.md`. Its scope rules match ours.
- **REA** (MIT): MCP server for static reverse engineering (Ghidra/Hopper/IDA bridges, PE, IL2CPP metadata,
  .NET). Its maintainers' DISCLAIMER says they don't support game-cheat development, so: use it as an optional
  analysis backend, keep it swappable, don't ask its community for trainer help, and include its license
  notice.
