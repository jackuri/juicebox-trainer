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
| `crates/ot-packs` | Cheat pack schema, validator, store, trainer runtime, `otpack` CLI |
| `crates/ot-saves`, `ot-mcp` | Saves, guarded MCP server (later versions) |
| `packs/examples` | Example packs for the testbed |
| `testbed/native`, `testbed/harness` | Fake "game" used by tests (x64 and x86), and test helpers |
| `scripts/` | `check.ps1` (all checks), `bump-version.mjs` |
| `Try OpenTrainer.cmd` | Double-click: opens the test game and the app |

## Try it (no typing)

Double-click **`Try OpenTrainer.cmd`** in this folder. It opens two windows: the fake test game and
OpenTrainer. (The first start takes a few minutes while everything is built. It needs Rust and
Node.js installed.) Then follow "Use a cheat pack" below, typing commands like `hit 25` into the fake
game's window.

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

## Try attaching to the testbed

1. In one terminal, start the fake game. It prints its PID and the addresses of its values:

   ```powershell
   cargo run -p testbed-native
   ```

   Type `hit 10`, `fire`, `earn 100` or `state` to change and show its values.
2. In a second terminal, run `npm run dev` to open OpenTrainer.
3. Pick `testbed_native.exe` (search for it or its PID). The guard should say "No anti-cheat found".
   Click **Attach**.
4. Paste the `gold` address from the testbed's `"addr"` line into **Address**, keep type `i32`, and
   click **Read** (250). Type `9999`, click **Write**, then type `state` in the testbed terminal.
5. Pointer chain: read `ptr` at the `chain.root` value (e.g. `testbed_native.exe+0x3A200`), add the
   offsets from `chain.offsets` and you'll land on the same stats block.

## Find and freeze a value with the scanner

1. Attach to the testbed as above. The **Scanner** tab opens first.
2. Type `250` (the starting gold), keep `i32`, click **First scan**.
3. In the testbed terminal type `earn 17`. Back in OpenTrainer type `267`, click **Next scan**. One
   result should be left: the gold address.
4. Click **Add** on it. It appears in the **Address list** below with its live value.
5. Tick **Freeze**, then type `spend 100` and `state` in the testbed: gold snaps back to 267.
   Click the value, type `5000` and press Enter to freeze it at a new value. Untick to let it change.
6. Unknown values: click **New scan**, pick **Unknown initial value**, **First scan**. Type `hit 5` in
   the testbed, pick **Decreased**, **Next scan**. Repeat with `heal` + **Increased** and with no change
   + **Unchanged** until health is one of the few results.

## Use a cheat pack (trainer)

1. Start the testbed (`cargo run -p testbed-native`) and `npm run dev`, then attach as above. The
   **Trainer** tab opens first and lists **OpenTrainer Testbed (x64)**. Click **Load**.
2. Turn on **Infinite Health** (or press **F1**), then type `hit 25` in the testbed: health stays 100.
   Open **What it changes** to see `29 D0 → 90 90` (`sub eax,edx → nop; nop`). Turn it off and `hit 25`
   again: 75.
3. **F2** toggles Infinite Ammo (`fire 5`). **Health Lock at 999** (F3) freezes health at 999.
   **+1000 Gold** (F4) adds gold; **Set Gold** writes what you type (try `-1`: refused, out of range).
   Drag **Move Speed** and type `state`; it shows *held* until you click **Release**.
4. Click **Unload pack** (or Detach): every patch is restored and the hotkeys are released.

Packs are plain JSON. Put your own in `%APPDATA%\OpenTrainer\packs\<game-id>\pack.json` (the
Trainer tab has **Open packs folder**), and check them with:

```powershell
cargo run -p ot-packs --bin otpack -- validate packs/examples/testbed-native/pack.json
cargo run -p ot-packs --bin otpack -- schema        # the JSON Schema
```

## Find a pointer path that survives a restart

1. Attach to the testbed and find gold with the **Scanner** (see above). Copy its address.
2. Open **Pointers**, paste the address, keep 4 levels and offset `1000`, click **Find pointers**.
3. Close the testbed and start it again, attach again, find gold again with the scanner, and enter the
   new address under "Restarted the game?". Click **Check paths**. The path
   `[[[testbed_native.exe+0x…]+0x10]+0x10]+0xC` is still there.
4. Click **Add** on it: the address list follows that path from now on.

## Make a signature for a piece of code

Open **Signatures** and enter a code address, e.g. the damage code: search the module for the
pattern `0F 1F 80 54 4F 54 01` and add 7. **Make signature** gives the shortest pattern that finds
only that code, with addresses wildcarded.

## If OpenTrainer crashes

Changes to game code are written down before they are made
(`%LOCALAPPDATA%\OpenTrainer\restore`). The next start undoes anything left behind and shows a notice.

To see the guard refuse, copy `target\debug\testbed_native.exe` into a new folder that also contains
an empty `BattlEye` folder, run the copy, and pick it in OpenTrainer.

## License

MIT — see [LICENSE](LICENSE).
