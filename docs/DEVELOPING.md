# Developing JuiceBox Trainer

How to build JuiceBox Trainer from source and try every feature against the fake test game
(`testbed/native`). Players: see the [user guide](USER_GUIDE.md) instead.

## Try it (no typing)

Double-click **`Try JuiceBox Trainer.cmd`** in the main folder of the repository. It opens two windows: the fake test game and
JuiceBox Trainer. (The first start takes a few minutes while everything is built. It needs Rust and
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

Since 1.23.0 the app moves an OpenTrainer install's folders (`%APPDATA%\OpenTrainer`,
`%LOCALAPPDATA%\OpenTrainer`, `org.opentrainer.desktop`) to JuiceBox Trainer's on every GUI start
(`jb_core::names::move_legacy_folders`). On a PC that still runs OpenTrainer, start dev or test
builds with `APPDATA` and `LOCALAPPDATA` pointing at a scratch folder, or the move happens for real.

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
2. In a second terminal, run `npm run dev` to open JuiceBox Trainer.
3. Pick `testbed_native.exe` (search for it or its PID). The guard should say "No anti-cheat found".
   Click **Attach**.
4. Paste the `gold` address from the testbed's `"addr"` line into **Address**, keep type `i32`, and
   click **Read** (250). Type `9999`, click **Write**, then type `state` in the testbed terminal.
5. Pointer chain: read `ptr` at the `chain.root` value (e.g. `testbed_native.exe+0x3A200`), add the
   offsets from `chain.offsets` and you'll land on the same stats block.

## Find and freeze a value with the scanner

1. Attach to the testbed as above. The **Scanner** tab opens first.
2. Type `250` (the starting gold), keep `i32`, click **First scan**.
3. In the testbed terminal type `earn 17`. Back in JuiceBox Trainer type `267`, click **Next scan**. One
   result should be left: the gold address.
4. Click **Add** on it. It appears in the **Address list** below with its live value.
5. Tick **Freeze**, then type `spend 100` and `state` in the testbed: gold snaps back to 267.
   Click the value, type `5000` and press Enter to freeze it at a new value. Untick to let it change.
6. Unknown values: click **New scan**, pick **Unknown initial value**, **First scan**. Type `hit 5` in
   the testbed, pick **Decreased**, **Next scan**. Repeat with `heal` + **Increased** and with no change
   + **Unchanged** until health is one of the few results.

## Use a cheat pack (trainer)

1. Start the testbed (`cargo run -p testbed-native`) and `npm run dev`, then attach as above. The
   **Trainer** tab opens first and lists **JuiceBox Trainer Testbed (x64)**. Click **Load**.
2. Turn on **Infinite Health** (or press **F1**), then type `hit 25` in the testbed: health stays 100.
   Open **What it changes** to see `29 D0 → 90 90` (`sub eax,edx → nop; nop`). Turn it off and `hit 25`
   again: 75.
3. **F2** toggles Infinite Ammo (`fire 5`). **Health Lock at 999** (F3) freezes health at 999.
   **+1000 Gold** (F4) adds gold; **Set Gold** writes what you type (try `-1`: refused, out of range).
   Drag **Move Speed** and type `state`; it shows *held* until you click **Release**.
4. Click **Unload pack** (or Detach): every patch is restored and the hotkeys are released.

Packs are plain JSON. Put your own in `%APPDATA%\JuiceBox Trainer\packs\<game-id>\pack.json` (the
Trainer tab has **Open packs folder**), and check them with:

```powershell
cargo run -p jb-packs --bin juicepack -- validate packs/examples/testbed-native/pack.json
cargo run -p jb-packs --bin juicepack -- schema        # the JSON Schema
```

## Find a pointer path that survives a restart

1. Attach to the testbed and find gold with the **Scanner** (see above). Click **Find pointers** on
   its row: the **Pointers** tab opens with the address filled in.
2. Keep 4 levels and offset `1000`, click **Find pointers**.
3. Close the testbed and start it again, attach again, find gold again with the scanner, and enter the
   new address under "Restarted the game?". Click **Check paths**. The path
   `[[[testbed_native.exe+0x…]+0x10]+0x10]+0xC` is still there.
4. Click **Add** on it: the address list follows that path from now on. **Copy as pack cheat** puts
   a `value` cheat for that path on the clipboard (type from "Add as"), ready to paste into a pack's
   `cheats` list; change its `id` and `name`.

## Make a signature for a piece of code

Open **Signatures** and enter a code address, e.g. the damage code: search the module for the
pattern `0F 1F 80 54 4F 54 01` and add 7. **Make signature** gives the shortest pattern that finds
only that code, with addresses wildcarded.

## Back up and restore saves

1. Start the fake game (or double-click `Try JuiceBox Trainer.cmd`) and type `save` in its window. It
   writes a save file, like a real game would.
2. In JuiceBox Trainer click **Save backups** at the top, pick **testbed_native.exe** under "Add a game"
   and click **Add**. JuiceBox Trainer finds the save folder (`AppData\LocalLow\TestCo\Testbed`) by itself.
   (Attaching to a game adds it too, and backs up its saves before you use any cheat.)
3. Click **Back up now**. Then type `earn 1000` and `save` in the fake game.
4. Click **Compare with now** on the backup: it shows `"gold"` going up by 1000.
5. Type `quit` in the fake game (JuiceBox Trainer backs up again when a game closes). Click **Restore** on
   the first backup and confirm. Start the fake game again and type `load`: the gold is back to what
   it was.

Restoring needs the game closed and is refused for games with anti-cheat. Your saves from just
before a restore are kept as a "Before a restore" backup, so you can undo it. Backups are stored in
`%LOCALAPPDATA%\JuiceBox Trainer\saves`.

## Edit a save

1. In the fake game type `save`, then `quit` (saves can only be changed while the game is closed).
2. In **Save backups**, pick the testbed. Under **Edit saves**, click **Open** next to `save1.json`.
3. Change `player.gold` to `5000` and click **Save changes**. (Your saves are backed up first, as
   "Before an edit".)
4. Start the fake game again and type `load`: gold is 5000.

Or click a **Quick code** such as "999,999 gold (in the save file)": it comes from the testbed's
cheat pack. Files JuiceBox Trainer can't read as values open as bytes, with a number inspector.

## Make a trainer with AI

This uses your Claude subscription: sign in to Claude Code on this PC once. Or set an Anthropic API
key as `ANTHROPIC_API_KEY` in Windows and pick "API key".

1. Start the fake game (or double-click `Try JuiceBox Trainer.cmd`).
2. In JuiceBox Trainer click **Make a trainer (AI)**, pick `testbed_native.exe`, tick the cheats you want
   and click **Start**.
3. Do what it asks in the fake game's window (for example `spend 50`), then press **Done** or answer
   **Yes / No**.
4. When it finishes, go to **Cheats**, attach to the game, and load the new pack (marked AI) on the
   Trainer tab.

The helper can only use JuiceBox Trainer's guarded tools: it can't run programs or read your files, it
refuses games with anti-cheat, and its test changes are undone when it stops. Every step it takes is
logged in `%LOCALAPPDATA%\JuiceBox Trainer\ai-logs`.

Other AI options: an Anthropic API key, or **a free AI running on this PC** with
[Ollama](https://ollama.com) or [LM Studio](https://lmstudio.ai): start it, pick "A free AI running
on this PC" and type the model's name (one that can use tools, with a large context). It's free but
slower and less reliable. Under **Advanced** you can add a code-analysis MCP server you installed
yourself (for example REA); JuiceBox Trainer works without it, and if you use REA its own license applies.

For "infinite" cheats the helper finds the game code that changes the value (it briefly attaches as
a debugger while you, say, take some damage) and turns that code off, so it rarely needs a restart.

## When a game update breaks a pack

Load the pack on the **Trainer** tab. If some cheats show as failed, click **Try to repair**:
JuiceBox Trainer looks for the same code in the new version, and saves a repaired copy of the pack (your
old one is kept as `pack.json.bak`).

## Ask the AI what changed in a save

In **Save backups**, make a backup, do something in the game and save, then click **Compare with
now** on that backup and **Explain with AI**. The helper tells you which values changed and what they
probably are ("this is your gold").

## If JuiceBox Trainer crashes

Changes to game code are written down before they are made
(`%LOCALAPPDATA%\JuiceBox Trainer\restore`). The next start undoes anything left behind and shows a notice.

To see the guard refuse, copy `target\debug\testbed_native.exe` into a new folder that also contains
an empty `BattlEye` folder, run the copy, and pick it in JuiceBox Trainer.

## Share packs from the command line

```powershell
cargo run -p jb-packs --bin juicepack -- export packs/examples/testbed-native/pack.json   # writes testbed-native-1.0.0.juice
cargo run -p jb-packs --bin juicepack -- inspect testbed-native-1.0.0.juice
cargo run -p jb-packs --bin juicepack -- index <packs-folder> index.json                   # community list index
```

To try the Packs tab against a local community list, serve a folder with `index.json` and
`packs/<id>/pack.json` and start the app with `JB_PACK_INDEX=http://127.0.0.1:<port>/index.json`.

## Build the installer

See [RELEASING.md](RELEASING.md).
