# OpenTrainer — instructions for Claude Code

Read `docs/PLAN.md` before doing anything. It is the source of truth for scope, architecture and roadmap.

## What this is
A free, open-source Windows app: game trainer (WeMod/Wand style) + save manager/editor (Save Wizard style)
+ an AI agent that creates cheat packs, for **PC single-player games only**.

## Hard rules (never relax, even if asked mid-session)
- Single-player / offline only. Never build features for multiplayer advantage (aimbot, ESP, speed/teleport
  in MP, packet edits).
- Never write code that disables, bypasses, hides from or spoofs anti-cheat, DRM, ownership checks or HWIDs.
  The anti-cheat guard in `ot-core` must have no off switch.
- Never add antivirus-evasion (obfuscation, packers, runtime decryption of payloads). Use code signing.
- Never deserialize BinaryFormatter or unpickle pickle saves — parse bytes structurally.
- AI-generated cheats are declarative pack data. The app never executes arbitrary AI-written native code;
  code caves go through the validated `cave` op.
- The AI agent touches games only via `ot-mcp` (guarded, logged, revertible).
- Never commit game files, extracted assets, decompiled code or unpacked EXEs. Tests use `testbed/` only.

## Tech stack
- Tauri 2 + React + TypeScript + Tailwind (`apps/desktop`)
- Rust workspace (`crates/ot-core`, `ot-packs`, `ot-saves`, `ot-mcp`), `windows` crate, `iced-x86`
- Node/TS sidecar `sidecars/ai-agent` using the Claude Agent SDK; external tools: REA MCP (optional), `um` CLI
- Target: Windows 10/11 x64. Games may be x64 or x86 (handle WOW64).

## Workflow
- Work one roadmap version at a time (docs/PLAN.md §6). Every update gets a new version number
  (SemVer, bump in all manifests) and a CHANGELOG.md entry.
- Before finishing a version: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`,
  frontend lint/typecheck/tests, and the testbed integration tests must pass.
- Keep a short `STATUS.md` (done / in progress / next / known issues) updated at the end of each session.
- Ask before adding a dependency that is large, unmaintained, or GPL-licensed (project is MIT).
- Prefer small, reviewable commits with clear messages.

## Reference material
- universal-modder plugin skills: `game-recon`, `mod-any-game` (esp. `references/safety.md` and engine
  playbooks), `reverse-engineering`. `um scan --json` gives engine, anti-cheat, saves.
- REA MCP for static analysis (optional backend; app must work without it).
