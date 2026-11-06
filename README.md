# OpenTrainer

A free, open-source Windows app that combines a game **trainer**, a **save manager/editor**, and an
**AI agent that makes cheat packs**, for **PC single-player games only**.

> Early development. See [`docs/PLAN.md`](docs/PLAN.md) for scope and roadmap and
> [`STATUS.md`](STATUS.md) for where things stand.

## Rules this project never relaxes

- Single-player / offline games only. No multiplayer advantage of any kind.
- No anti-cheat, DRM, ownership-check or HWID bypass. If anti-cheat is present, OpenTrainer will not
  attach, and the guard has no off switch.
- No antivirus evasion. Releases will be code-signed instead.
- No game files, extracted assets or decompiled code in this repo.

## Repository layout

| Path | What |
|---|---|
| `apps/desktop` | Tauri 2 + React + TypeScript + Tailwind desktop app |
| `crates/ot-core` | Process attach, memory read/write, module map, anti-cheat guard |
| `crates/ot-packs`, `ot-saves`, `ot-mcp` | Cheat packs, saves, guarded MCP server (later versions) |
| `testbed/native` | Fake "game" used by tests (x64 and x86) |
| `scripts/` | `check.ps1` (all checks), `bump-version.mjs` |

## Develop

Requirements: Windows 10/11 x64, Rust stable (MSVC) with `i686-pc-windows-msvc`, Node.js LTS,
Visual Studio Build Tools with "Desktop development with C++". WebView2 ships with Windows.

```powershell
npm ci                 # JS dependencies
npm run dev            # launch the desktop app (Vite + Tauri)
npm run check          # every check CI runs
npm run bump -- 0.2.0  # bump the version in all manifests
```

Run the testbed game on its own:

```powershell
cargo run -p testbed-native                                          # x64
cargo run -p testbed-native --target i686-pc-windows-msvc            # x86 (WOW64)
cargo run -p testbed-native --features shifted                       # different offsets
```

## License

MIT — see [LICENSE](LICENSE).
