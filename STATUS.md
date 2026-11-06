# Status

_Last updated: 2026-11-06 — v0.4.0_

## Done
- CI green on GitHub (private repo jackuri/opentrainer) through v0.3.0. v0.4.0 is pushed and tagged;
  check its CI run on GitHub Actions.
- v0.1.0 skeleton: workspace, Tauri app boots, testbed game (x64/x86/shifted), CI, check + bump scripts.
- v0.2.0 core memory: process list, guarded attach/detach, typed read/write, module map, regions,
  anti-cheat guard with tests, desktop UI to attach and read/write.
- v0.3.0 scanner: multithreaded first/next scans, address list, 50 ms freeze loop, manual-tools UI.
- v0.4.0 packs: v1 schema (Rust types + JSON Schema), validator, `value` and `patch` ops, safe-patch
  instruction checker (iced-x86), trainer runtime with restore on off/unload/detach/exit, build-hash
  check, hotkeys via `RegisterHotKey`, pack store (built-in examples + `%APPDATA%\OpenTrainer\packs`),
  `otpack validate` CLI, example packs for the x64 and x86 testbed, Trainer tab. Testbed code sites are
  now inline asm with fixed bytes. All checks pass locally (`npm run check`), plus the local-only
  hotkey test that presses a real key.

## In progress
- Nothing. Stopped after v0.4.0 as planned.

## Next
- v0.5.0 stability: pointer scanner, AOB maker, code caves (validated `cave` op), original-byte
  verification everywhere, crash-safe restore.
- Small v0.4 follow-ups worth folding into v0.5: hotkey rebinding (saved per pack), "Load pack from
  file…" (needs a file dialog), auto-load a pack on attach.

## Known issues
- Not checked by hand in the real app this session: the Trainer tab was covered by frontend tests,
  the trainer by live-testbed integration tests and hotkeys by the local SendInput test, but nobody
  clicked through `npm run dev` (computer use was declined). Do the manual run in the README.
- A registered hotkey is taken from every other app while a pack is loaded (that is how
  `RegisterHotKey` works). F1–F4 in the example packs therefore don't reach other windows until you
  unload the pack or detach.
- The hotkey test needs an interactive desktop, so it is `#[ignore]`d and not run in CI.
- Example packs list no build hashes (the testbed's hash changes with the compiler), so they show
  "No build info". Their value offsets fit the default build; on the `shifted` build the patches work
  but the values point at the wrong fields.
- Guard false positive found while testing: a game anywhere within three folders of a folder that holds
  an anti-cheat directory is blocked, e.g. `%LOCALAPPDATA%\Programs\Game\game.exe` on a PC that has
  `%LOCALAPPDATA%\BattlEye` from another game. Intended to err on the side of blocking; documented.
- Scan results live in memory; unknown-value scans refuse above 4 GiB. The scanner shows the first
  200 results only.
- The address list is per session and not saved (save cheats as a pack instead).
- No pointer scanner yet (v0.5.0).
- The guard blocks *every* attach while a kernel anti-cheat (Vanguard `vgk`, FACEIT, ACE…) is running.
  Intended; documented for users.
- Some guard signature names (Ricochet, ACE, XIGNCODE, FACEIT services) need confirming on real
  installs. A wrong name can only cause a missed detection, never a false block.
- Driver file-name detection is best effort without admin rights; driver *services* are still checked.
- One v0.3.0 CI run timed out in a frontend test (passed on re-run). If it recurs, give it more time.
- App icon is a placeholder.
