# Status

_Last updated: 2026-11-06 — v0.3.0_

## Done
- CI green on GitHub (private repo jackuri/opentrainer) through v0.3.0.
- v0.1.0 skeleton: workspace, Tauri app boots, testbed game (x64/x86/shifted), CI, check + bump scripts.
- v0.2.0 core memory: process list, guarded attach/detach, typed read/write, module map, regions,
  anti-cheat guard with tests, desktop UI to attach and read/write.
- v0.3.0 scanner: multithreaded exact/between/unknown first scans and
  exact/between/changed/unchanged/increased/decreased next scans, address list, 50 ms freeze loop,
  manual-tools UI. Verified end to end in the real app against the testbed (scan → narrow → add →
  freeze holds against in-game spending → write while frozen → unfreeze → unknown/decreased scan →
  session dropped when a fake anti-cheat process starts → game exit noticed).

## In progress
- Nothing. Stopped after v0.3.0 as planned.

## Next
- v0.4.0 packs: pack schema + validator, value/patch ops, hotkeys, trainer UI, example packs vs testbed.

## Known issues
- One CI run of v0.3.0 on `main` timed out in the frontend scanner test (51 s on the runner,
  milliseconds locally, 15/15 local passes); the re-run and the tag run passed. If it recurs, give
  that test more time or split it.
- Scan results live in memory. Unknown-value scans copy all scanned memory and refuse above 4 GiB;
  a disk-backed store would lift that.
- The scanner shows the first 200 results only (count is always exact).
- The address list is per session and not saved; packs (v0.4.0) are where saved cheats go.
- Pointer chains: no pointer-chain syntax and no pointer scanner yet (the scanner comes in v0.5.0).
- The guard blocks *every* attach while a kernel anti-cheat (Vanguard `vgk`, FACEIT, ACE…) is running
  on the PC, even for unrelated single-player games. Intended; documented for users.
- Some guard signature names (Ricochet, ACE, XIGNCODE, FACEIT services) are best-known names and need
  confirming on real installs. A wrong name can only cause a missed detection, never a false block.
- Driver file-name detection is best effort without admin rights; driver *services* are still checked.
- App icon is a placeholder.
