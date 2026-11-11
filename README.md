# JuiceBox Trainer

<img src="apps/desktop/src-tauri/icons/128x128.png" alt="" width="96" align="right">

**JUICE** = **J**ackuri's **U**ltimate **I**n-game **C**heat **E**ngine. Formerly called OpenTrainer
(renamed in 1.23.0).

A free, open-source Windows app for **PC single-player games**:

- a **trainer**: infinite health, more gold, faster movement… with hotkeys, controller combos,
  presets, an overlay and a phone remote;
- a **save manager and editor**: automatic backups, compare, restore, edit values;
- an **AI helper** that finds new cheats for a game with you and saves them as a cheat pack;
- **cheat packs** you can share as `.juice` files (`.otpack` from OpenTrainer also open) or get from the community list.

**Get it:** download the installer from the [releases page](https://github.com/jackuri/juicebox-trainer/releases)
and read the [user guide](docs/USER_GUIDE.md).

## Rules this project never relaxes

- Single-player / offline games only. No multiplayer advantage of any kind.
- No anti-cheat, DRM, ownership-check or HWID bypass. If anti-cheat is present, JuiceBox Trainer will not
  attach, and the guard has no off switch.
- No antivirus evasion. Releases are meant to be code-signed instead (not yet: see the user guide).
- AI-made cheats are checked data, never programs. The AI touches games only through JuiceBox Trainer's
  guarded, logged and undoable tools.
- No game files, extracted assets or decompiled code in this repo.

## Repository layout

| Path | What |
|---|---|
| `apps/desktop` | Tauri 2 + React + TypeScript + Tailwind desktop app |
| `crates/jb-core` | Process attach, memory read/write, module map, anti-cheat guard |
| `crates/jb-packs` | Cheat pack schema, validator, store, `.juice` sharing, trainer runtime, `juicepack` CLI |
| `crates/jb-saves` | Save locator, backups, compare, guarded restore, editor, installed games |
| `crates/jb-mcp` | Guarded MCP server (tools) for the AI helper |
| `sidecars/ai-agent` | AI helper: Claude Agent SDK, talks to the app and to `jb-mcp` |
| `packs/examples` | Example packs for the testbed |
| `testbed/native`, `testbed/harness` | Fake "game" used by tests (x64 and x86), and test helpers |
| `scripts/` | `check.ps1` (all checks), `bump-version.mjs`, release helpers |
| `Try JuiceBox Trainer.cmd` | Double-click: opens the test game and the app from source |

## Develop

Requirements: Windows 10/11 x64, Rust stable (MSVC) with `i686-pc-windows-msvc`, Node.js LTS,
Visual Studio Build Tools with "Desktop development with C++". WebView2 ships with Windows.

```powershell
npm ci                 # JS dependencies
npm run dev            # launch the desktop app (Vite + Tauri)
npm run check          # every check CI runs
npm run bump -- 1.0.1  # bump the version in all manifests
```

[docs/DEVELOPING.md](docs/DEVELOPING.md) walks through every feature with the fake test game,
[docs/RELEASING.md](docs/RELEASING.md) covers installers, signing and updates, and
[docs/PLAN.md](docs/PLAN.md) has the design. [STATUS.md](STATUS.md) says where things stand.

## License

MIT — see [LICENSE](LICENSE). The AI engine (Anthropic's Claude Agent SDK) is not part of
JuiceBox Trainer; the app downloads it on first use and its own terms apply.
