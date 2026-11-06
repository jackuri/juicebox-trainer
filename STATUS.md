# Status

_Last updated: 2026-11-06 — v0.2.0_

## Done
- v0.1.0 skeleton: workspace, Tauri app boots, testbed game (x64/x86/shifted), CI, check + bump scripts.
- v0.2.0 core memory: process list, guarded attach/detach, typed read/write, module map, regions,
  anti-cheat guard with tests, desktop UI to attach and read/write. Verified end to end in the real
  app against the testbed (attach, read gold, write gold, guard refusing pid 4).

## In progress
- Nothing. Stopped after v0.2.0 as planned.

## Next
- v0.3.0 scanner: exact/unknown/changed scans (multithreaded), address list, freeze loop, manual-tools UI.

## Known issues
- CI has not run yet: the repo has no GitHub remote.
- The guard blocks *every* attach while a kernel anti-cheat (Vanguard `vgk`, FACEIT, ACE…) is running
  on the PC, even for unrelated single-player games. Intended; documented for users.
- Some guard signature names (Ricochet, ACE, XIGNCODE, FACEIT services) are best-known names and need
  confirming on real installs. A wrong name can only cause a missed detection, never a false block.
- Driver file-name detection is best effort without admin rights; driver *services* are still checked.
- Pointer chains can be followed by hand (`ptr` reads) but there's no chain syntax yet (comes with packs).
- App icon is a placeholder.
