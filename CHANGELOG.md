# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [1.13.0] - 2026-11-09

### Added
- **SQLite saves** in the save editor: games that keep their save as a small database
  (`.db`, `.sqlite`, often with other names) now open as a list of values, one per cell,
  `Table[row].Column` (for example `Player[1].Gold`). Numbers and text can be changed; each edit
  changes only that one cell (the database's own triggers don't run) and the database is written
  back, also inside gzip / zlib wrappers. Blobs are shown (size and first bytes) but not changed.
  Tables without row ids are not listed. If the game left changes in a `-wal` file, OpenTrainer
  says so and won't edit until the game has been started and closed normally.
  The database is opened in memory from the file's bytes, in SQLite's defensive mode with the
  file's own code untrusted and no extensions.
- Save codes in packs can use `"codec": "sqlite"` (`"path": "Player[1].Gold"`).

### Changed
- Uses the `rusqlite` library (MIT) with SQLite (public domain) built in. Building OpenTrainer from
  source now needs Rust 1.89 or newer.

## [1.12.0] - 2026-11-09

### Added
- **Memory viewer** (Cheats → **Memory** tab, for experts): the game's memory as 16 bytes a row with
  the text next to them, updated every second; bytes that changed light up and memory that can't
  be read shows `??`. Type an address (`0x…`, `game.exe+0x…` or a pointer path), page up and down,
  and see whether the memory is readable, writable or code and which module it is in. Click a byte
  to see the 8/16/32/64-bit numbers, float and double it starts, and the pointer it holds (with
  **Go there**). **Write bytes** changes it: data is written straight away; game code is changed
  the same checked way as a cheat's patch and put back when you detach; read-only memory is
  refused. **Add to address list** keeps it with a type of your choice, and the address list has a
  ⌗ button that opens an entry in the viewer.

## [1.11.0] - 2026-11-09

### Added
- **Copy saves to another Steam account.** For games that keep saves per Steam account (Steam
  Cloud's `userdata` folders, or a game folder named after your Steam ID), Save backups has a new
  section: pick the account to copy from and the one to copy to, click **Check what will be
  copied** to see every file (and which ones would be replaced), then **Copy**. Your Steam ID
  inside the saves, and in file names, is changed to the other account's, so games that check it
  accept the save; compressed and checksummed saves are unpacked for this and packed again. The
  saves you copy from are never changed, all the game's saves are backed up first ("Before copying
  to another account"), the game must be closed, and the anti-cheat guard applies as for restoring.
  The other account's new folder is added to the game's save folders so it is backed up too.
  Accounts are the ones Steam knows on this PC (names from Steam's sign-in list).

## [1.10.0] - 2026-11-09

### Added
- **More save formats in the save editor.** These now open as named values you can change, with
  everything else in the file left exactly as it was:
  - **XML** saves (common in Unity and XNA games): text inside elements and attributes.
  - **YAML** saves: `key: value` maps, `- item` lists, quoted text, several documents (Unity's
    YAML). Lists or maps written on one line, aliases and text over several lines are shown but
    can't be changed.
  - **RPG Maker MV** saves (`.rpgsave`, lz-string compressed); written back the same way the game
    writes them. **RPG Maker MZ** saves (`.rmmzsave`) already opened (zlib JSON).
  - **GameMaker** INI saves with numbers in quotes (`gold="250.000000"`): the number inside is
    shown and edited, the quotes are kept.
  - **Godot** `ConfigFile` saves: `key=value` with Godot values; lists and maps over several lines
    are shown but can't be changed. Godot saves encrypted by the game say so (they aren't
    decrypted).
  - **Skyrim / Skyrim Special Edition and Fallout 4** saves show their summary (character name,
    level, place, race, play time), read-only.
- Save codes in packs can use `"codec": "xml"` and `"codec": "yaml"`.

## [1.9.0] - 2026-11-09

### Added
- **Find a value** in the Cheats tab: a guided helper that finds a number in the game without value
  types or addresses. You type the number the game shows (or say "went up / went down" for a bar);
  it searches whole numbers and decimals at once, accepts rounded and `1,250`-style numbers, then
  tries each place it found and asks "Does it show 625 now?", putting your real number back after
  each try. Found values can be kept, set once, or saved as a cheat with a free hotkey in your own
  pack for the game ("*Game* (my cheats)"). A value that moves each time the game starts is saved
  after a restart: OpenTrainer looks for pointer paths to it, you find it once more, and only a
  path that still leads to it is saved.
- Scanner results update their values live (every second while the Scanner is open).

## [1.8.1] - 2026-11-09

### Fixed
- **Each game is listed once when picking a running game.** Games that start a launcher, web views
  and other helpers from their folder (Mecha BREAK showed nine entries) now appear once, as their
  main program: the one its cheat pack is made for, else the one using the most memory. The
  helpers are still under **Show all running programs** if you need one.

## [1.8.0] - 2026-11-09

### Added
- **Hide games in the Games tab.** Each game has a **Hide** button; hidden games leave the list and
  **Show hidden (N)** brings them back, marked "Hidden", with **Unhide**. The list of hidden games is
  kept only on this PC (`%APPDATA%\OpenTrainer\hidden-games.json`); the game and its pack are not
  touched.

## [1.7.0] - 2026-11-09

### Changed
- **Games that come with anti-cheat but can be played without it.** The guard now judges a running
  game by what is actually running. Anti-cheat files in the game's folder no longer block on their
  own: when the game was started through an official no-anti-cheat option (Halo: The Master Chief
  Collection's "Anti-Cheat Disabled"), nothing of it runs and you can attach. Anti-cheat that is
  running still always blocks, re-checked every 5 seconds; there is still no off switch, and
  OpenTrainer never turns anti-cheat off or gets around it. Changing a game's files while it isn't
  running (save restore) still refuses games with anti-cheat files.
- Games list: such games say "Has Easy Anti-Cheat: works only when the game runs without it"
  instead of "not supported". The attach message says when anti-cheat comes with the game but isn't
  running, and when it is running, that some games have an official way to play without it.

## [1.6.0] - 2026-11-09

### Added
- **Values behind hooked pointers.** Many Cheat Engine tables grab the player's data with a script
  that hooks a line of game code and copies a register into a variable (`player_ptr`,
  `battle_ptr`). OpenTrainer now redoes that without changing any game code: when you use such a
  value, it briefly puts a hardware breakpoint on that line, reads the same register once, removes
  the breakpoint and lets go of the game. Conditions like "only when ebx is 0" are kept. Held
  values re-read it every few seconds so they follow the game (e.g. a new battle).
- Packs can say this too: an `aob` address can have `"capture": { "register": "rdx", "add":
  "0x10", "when": { "register": "ebx", "equals": 0 } }` (64-bit and 32-bit games).
- In Cheats, such a value shows "not found yet" until the game has used it, and an action that has
  to wait for the game says "Waiting for the game…" with what to do.
- On the Dragon Ball Xenoverse 2 table this adds the 39 battle values (current health, ki, stamina…):
  137 cheats in all.

### Changed
- Entries under a script use the names that script defines (tables often define the same name in
  several scripts).

## [1.5.0] - 2026-11-08

### Added
- **Smarter Cheat Engine tables.** Many tables find their addresses with small scripts.
  OpenTrainer now reads those scripts as text (never runs them) and redoes the common patterns its
  own checked way, so the values built on them come over:
  - a pointer found from the game's code by a Lua script (`getAddress` + `readInteger` + 4) or an
    `aobscanmodule` + `registersymbol` script;
  - numbers a script sets (`define`, or a label placed at a number, like a save slot) and offsets
    with simple math (`slot*6180-6180+2F90`);
  - scripts that only swap a few bytes of game code (`db 90 90` on, the original bytes off) become
    OpenTrainer's checked patch cheats (64-bit games).
- Entries that sit under a text or other non-number entry (a "Base Address" line) still come over.
- On a real FearLessRevolution table (Dragon Ball Xenoverse 2) this goes from 0 to 98 cheats (92
  values like Zeni, TP Medals and experience, and 6 patches).

### Changed
- Clearer reasons for entries that still can't come over, e.g. values that depend on an address a
  script grabs by injecting code (planned next).

## [1.4.2] - 2026-11-08

Fixes from a full re-test of the real app and a security review.

### Security
- **Cheat Engine tables:** absurd numbers in an address (e.g. `+7FFFFFFFFFFFFFFF`) are refused
  instead of wrapping around into a wrong address, and a table may hold at most 300,000 XML nodes
  (a huge file could use hundreds of MB of memory). Pointer offsets with two signs (`--8`) are
  refused in every pack.
- **Sharing:** whether a pack is yours is now checked by the app itself, not only by hiding the
  button: packs from the community list can't be shared again, and a pack you got as a file needs
  your word that you made it or its maker allows sharing it (like Cheat Engine tables already did).
  Names and notes from a pack can no longer mention people or add links in the GitHub request.
- **Installing a pack file** installs exactly what the preview showed; if the file changed in
  between, it asks you to open it again.
- The GitHub sign-in read from Windows Credential Manager is checked for an empty entry first.
- The updater no longer includes its old zip support (OpenTrainer's updates are the signed setup
  file), which removes a zip library with known extraction bugs.
- Release and check workflows: the version tag is passed safely, and build steps no longer keep the
  job's GitHub token.

### Fixed
- A Cheat Engine group that has its own address now passes it to the entries inside it
  (`+10`-style addresses), instead of skipping them.
- Games with "crash" in their name (e.g. Crash Bandicoot) were hidden from **Your running games**;
  only real crash reporters are left out now.

## [1.4.1] - 2026-11-08

### Changed
- **One-click Update in About.** The **Update** button looks for the newest version and, if there
  is one, downloads it, installs it and restarts OpenTrainer. A copy that can't update itself
  opens the newest release page instead.
- The OpenTrainer repository is now public, so release builds can update themselves (once the
  updater key is set on GitHub) and anyone can download OpenTrainer from the releases page.

## [1.4.0] - 2026-11-08

### Added
- **Share with everyone.** Next to each of your packs in **Packs**, one click puts it on the
  community list: OpenTrainer signs in to GitHub (once, with a code you paste on GitHub's page, so
  it never sees your password), makes your copy of the list, adds the pack and sends it to the
  maintainers as a pull request. You see a link to it on GitHub.
- Before sharing, you confirm every cheat works; a pack converted from a Cheat Engine table needs
  your word that you made the table or its author allows sharing it. Built-in examples and packs
  from the community list aren't offered.

### Security
- The GitHub sign-in (scope `public_repo`) is kept in Windows Credential Manager, sent only to
  GitHub, and removed with **Sign out**. The app only opens GitHub's code page and the community
  list's pull requests in the browser.
- The community repository now accepts only `packs/<id>/pack.json` changes in pull requests and
  rebuilds `index.json` itself after a merge.

## [1.3.0] - 2026-11-08

### Added
- **Use Cheat Engine tables.** Open a `.CT` file (Packs → **Open a pack or table…**, or drop it on
  the window) and its number values become a pack: 1, 2, 4 and 8-byte numbers and decimals, at
  `game.exe+offset` addresses, pointer paths (`<Offsets>` and the `[[game.exe+10]+20]+8` form) and
  entries relative to their parent. Groups become categories. Each value can be set and held, like
  Cheat Engine's freeze, until released.
- Before installing you see every cheat that came over and every entry that didn't, with the
  reason. Scripts (Auto Assembler, Lua), fixed addresses, names only a table script defines, and
  text or byte values are left out and never run.
- A table that doesn't name the game's exe asks you to choose it.
- **Talk to the AI helper in your own words.** Every question now has a box for your own answer
  (e.g. "Yes, but fire damage still hurts" instead of Yes), and while the helper works a message box
  at the bottom passes your note to it after its current step (or before it finishes), so it can fix
  a cheat that only partly works.

### Fixed
- A test of engine detection could fail now and then on Windows (test cases shared a temporary
  folder that Windows doesn't always delete at once).

### Security
- Tables are read as data with a size limit (16 MB), no DTDs, depth and entry limits, and the
  result goes through the normal pack validator. New dependency: `roxmltree` (MIT/Apache-2.0,
  read-only XML parser, no dependencies of its own).

## [1.2.3] - 2026-11-07

### Changed
- **Game lists show your games, not every program.** The Cheats tab, "Make a trainer (AI)" and
  Save backups now list the running programs that are your games, under the game's name (a program
  counts as a game when it runs from an installed game's folder, or a cheat pack is made for it).
  Other programs appear only after ticking **Show all running programs**, and Windows' own programs
  never do. When no game is running, the list says how to start one.

## [1.2.2] - 2026-11-07

Fixes found by testing the real app end to end and by a code review.

### Fixed
- **Cover art didn't show in the installed app** (1.2.1): the app's security policy didn't allow
  in-memory pictures, and the bytes arrive as a plain number list that the Games tab didn't accept.
  Both fixed; on the test PC all 30 games now show a picture.
- **Auto-load came back after "Unload pack"** when you went to another tab and back. The choice is
  now remembered for that game until it closes.
- **Xbox logo paths** like `\Users\…` or `D:file` in a game's `appxmanifest.xml` could point
  outside the game folder. Only plain names inside the folder are used now.
- A game whose cover changes (e.g. Steam downloads its art later) shows the new picture after
  **Refresh** instead of the old one.
- **Release workflow:** when several version tags were released at once, GitHub marked the one
  that finished last as "Latest" (1.1.0 instead of 1.2.1; corrected by hand). The workflow now
  marks a release as latest only if no higher version exists.

## [1.2.1] - 2026-11-07

### Changed
- **Games** cards show the game's real cover art instead of a colour and initials. It comes only
  from files already on this PC; nothing is downloaded, copied or saved:
  - Steam games: the artwork Steam caches for its own library (`appcache\librarycache`), the wide
    header picture first.
  - GOG games: the icon GOG puts in the game folder. Xbox games: the logo named in the game's
    `appxmanifest.xml`.
  - Any other game: its `.exe` icon (drawn by Windows), shown large on a coloured background.
  - No picture at all: the colour and initials as before.
  The app only reads the pictures it found itself, never a path the screen asks for.

## [1.2.0] - 2026-11-07

A new look, in the style of modern game-trainer apps (the user asked for something like Wand /
WeMod). Our own colours, icon and drawings; no other app's logo, name or artwork.

### Changed
- **Sidebar** on the left with icons for every tab, OpenTrainer's logo and version at the top and
  whether a game is attached at the bottom.
- **Deep navy colours** with one blue-violet accent (instead of grey and green), soft background
  glow, rounder corners, thin dark scrollbars. Main buttons use the accent as a gradient.
- **Cheats:** a large "Attached" header for the game; cheats in rounded cards by category, with
  bigger ON / OFF switches, hotkeys drawn as keyboard keys and one-shot cheats as pill buttons.
  Tool tabs (Trainer, Scanner, …) are pills. The game list is cleaner and shows the chosen game.
- **Games:** a grid of game cards. Each card has a cover made from the game's name (a colour and
  its initials; no game art is downloaded), the store, a "Cheats" tag when a pack exists and a big
  Play button. Games with anti-cheat are greyed out.
- Bigger page titles in Packs, Save backups, Make a trainer (AI) and About.
- **App icon** and the phone remote page use the new colours.

## [1.1.0] - 2026-11-07

Polish: fewer pasted paths, fewer clicks.

### Added
- **Browse…** buttons for adding a game (Games, Save backups) and a save folder, using the normal
  Windows pickers. Pasting a path still works.
- **Open a plain `pack.json`** as well as an `.otpack`: with **Open a pack file…**, or by dropping
  it on the window. It gets the same "what's inside" check, size limit and validation.
- **Cheats → Trainer** loads the pack by itself when it is the only one for the game (nothing is
  turned on). After **Unload pack** it isn't loaded again. With no pack, **Get a pack…** opens the
  Packs tab.
- **Find pointers** on each Scanner result opens the Pointers tab with the address and type
  filled in.
- **Copy as pack cheat** on each pointer path puts a ready-to-paste `value` cheat (JSON) on the
  clipboard.

### Changed
- STATUS.md no longer lists hotkey rebinding as a follow-up (own keys and conflict checks shipped
  in 0.10.0).

## [1.0.0] - 2026-11-07

First release with an installer.

### Added
- **Windows installer** (NSIS). It installs for the current user without admin rights, has a new
  app icon and opens `.otpack` files from Explorer. A second start hands the file to the running
  OpenTrainer. A **Release** workflow builds the installer for each version tag and publishes it
  on GitHub with `SHA256SUMS.txt` (`docs/RELEASING.md`).
- **Sharing packs as `.otpack` files**: **Export** a pack. Open one by double-clicking it, dropping
  it on the window or with **Open a pack file…**. You see the game, the cheats (and which change
  game code) and any warnings before installing. Replacing a pack you already have needs a click
  and keeps the old one as `pack.json.bak`. An `.otpack` is a zip with only `otpack.json` and
  `pack.json`. It is read in memory with size limits, never unpacked or run, and validated like any
  pack. **Remove** moves a pack to `packs-removed` instead of deleting it. `otpack` gains `export`,
  `inspect` and `index` commands.
- **Community pack list** in the new **Packs** tab, read from the public repository
  `jackuri/opentrainer-packs`. Downloads are size-limited, checked against the list's SHA-256 and
  validated before they are shown. Installed packs record where they came from ("community",
  "imported") and show "Update available" when the list has a newer copy.
- **About** tab: version, update check, links to the user guide and releases, and what OpenTrainer
  never does.
- **Automatic updates** (Tauri updater). Updates must be signed with OpenTrainer's own key, and the
  signed version must match. Before installing, games get their original code back. Updates are
  **off** unless the build was made by the release workflow on a public repository, so they are
  off for now. The app says so and links the releases page.
- **AI engine on first use**: the installed app ships the AI helper's own scripts, but not
  Anthropic's engine (about 330 MB, not ours to redistribute). The AI tab offers **Download the AI
  engine**. It runs `npm install` of the exact tested versions with no install scripts, and points
  to nodejs.org when Node.js is missing. About can delete the engine again.
- Signing-ready release builds: with a code-signing certificate in the repository secrets, the
  installer is signed. Without one it isn't, and Windows warns when installing.
- Docs: `docs/USER_GUIDE.md` (for players), `docs/DEVELOPING.md` (the testbed walkthroughs that
  were in the README) and `docs/RELEASING.md`. The README is now short.
- Dependencies (all MIT / Apache-2.0, maintained by the Tauri team or widely used): the Tauri
  plugins `updater`, `dialog` (save / open file windows) and `single-instance`, `reqwest` +
  `rustls` (the updater's own internet library, also used for the pack list) and `zip` (deflate
  only).

## [0.10.0] - 2026-11-07

### Added
- **Games tab (library)**: finds installed games from Steam (all library folders), Epic, GOG and
  Xbox / PC Game Pass (`ot_saves::installs`), plus games added by their exe. Each game shows its
  pack, its start preset and whether the anti-cheat guard blocks it (online / anti-cheat games
  can't be started from here). **Play with cheats** starts the game through its store (or uses it
  if it's already running), waits for it, attaches (guarded as always), loads its pack and applies
  its start preset. Xbox games are started from the Xbox app while OpenTrainer waits.
- **Presets** (`ot_packs::settings`): save what's on as "Story mode" and similar, apply with one
  click, or pick one to use when the game starts from the Games tab. Cheats that can't turn on
  yet (e.g. the value only exists once a level has loaded) keep trying for up to 10 minutes and
  stop as soon as you change something yourself.
- **Favourites, notes and your own keys** per cheat: favourites show first; set your own keyboard
  key by pressing it (or none), and reset to the pack's key. Kept in
  `%APPDATA%\OpenTrainer\settings\<game-id>.json`, apart from the pack.
- **Controller hotkeys** (XInput): button combos like `LB+RB+DPadUp`, set by pressing them on the
  controller or typing them. Two or more buttons are required, because the game sees the same
  buttons. Polling only runs while a pack with combos is loaded.
- **Overlay**: a small see-through window on top of the game listing the cheats, their keys and
  which are on. Clicks pass through it; nothing is injected into the game. Works with windowed and
  borderless games (exclusive fullscreen hides it, as decided in PLAN §8).
- **Phone remote**, off by default: turn it on and scan the QR code with a phone on the same Wi-Fi.
  Each QR code works once and gives that phone its own random key; other requests need it, changes
  also need a custom header, and the PC must be addressed by IP. The phone can only do what the
  trainer's buttons do. "Unpair phones" and "Turn off" end every phone's access.
- Dependency: `qrcode` 0.14 (MIT / Apache-2.0, SVG output only) for the pairing QR code.

## [0.9.3] - 2026-11-07

### Added
- **Unreal object reader** (`ot_core::unreal`, AI tool `unreal_find`) for UE 4.23+ and UE5 games
  while they run. It reads the engine's name list and its classes, then lists the game's classes and
  properties by name (e.g. "Health") with their types and offsets, and reads the live objects of a
  class with their values and addresses. Object layout (where a class keeps its parent and
  properties) is worked out from well-known engine classes instead of fixed offsets.
- **Slower search**: when a game's object list can't be read directly, the reader finds classes and
  objects by searching the game's memory, like the value scanner does. It doesn't decode or get
  around anything. The first search can take several minutes on a big game; the result is kept for
  the rest of the session, so later searches are quick.
- Tested on a real single-player UE 5.7 game (played solo, read-only): 12,592 classes and structs,
  the player character's `Health` and `MaxHealth` read live (100 / 100). Objects found by searching
  memory must also look alive (function table in the exe, a readable name, an outer object), which
  removes stale copies.

### Changed
- Unreal tips in `game_recon` point the AI to `unreal_find`.
- `ot-core` is built optimized in development builds and tests (memory scans walk gigabytes).

## [0.9.2] - 2026-11-07

### Added
- **Unity IL2CPP names** (`ot_core::il2cpp`, in `game_classes`): reads class and field names from
  `global-metadata.dat`. The layout differs between Unity versions, so the reader works out where
  each class keeps its field count and first field by checking which positions add up to the file's
  totals and cover every field exactly once, instead of trusting one fixed layout. Checked on a real
  format-39 (Unity 6) file: 37,181 classes and 164,280 fields. Types live in GameAssembly.dll and
  show as `?`. Protected or encrypted metadata is refused with a clear message, never decrypted.
- **Unreal engine version** from the game's exe (`++UE4+Release-4.27` style text), with a tip for
  UE5 (positions are 64-bit doubles). Checked on real UE 4.19 and 5.7 games.
- `game_classes` says when a game's code includes anti-cheat parts (so it's probably online).

### Changed
- Name search hides compiler-made helper classes, shows C# property storage
  (`<Health>k__BackingField`) as `Health`, and lists field-name matches before class-name matches.
- Checked the Unity Mono reader on a real Unity game (643 classes, types read correctly).
- The roadmap moves the Unreal object reader to 0.9.3 (it needs a running game to test).

## [0.9.1] - 2026-11-07

### Added
- **Unity (Mono) and .NET game names** (`ot_core::dotnet`, AI tool `game_classes`): reads the class
  and field names and types from the game's own assemblies (`Assembly-CSharp.dll`, or an
  XNA / FNA / MonoGame game's exe and DLLs) by parsing their metadata tables. No code is decompiled,
  loaded or run, and nothing is saved. The helper uses it to learn, for example, that health is a
  `float` before it scans.
- **Free AI on your own PC**: a third AI option, "A free AI running on this PC", for Ollama
  (`http://localhost:11434`) or LM Studio (`http://localhost:1234`) with the model you choose. It uses
  their Claude-compatible Messages API. Expect it to be slower and to find cheats less often.
- **Optional code-analysis tool** (Advanced, in the AI view): an MCP server you install yourself (for
  example REA) for reading game files. The helper may use its tools only to understand code it found;
  saves explanations never get it. OpenTrainer doesn't ship or require it.
- The AI view remembers its settings on this PC.

### Changed
- The roadmap moves the IL2CPP and Unreal readers to 0.9.2.

## [0.9.0] - 2026-11-07

### Added
- **Find what writes to an address** (`ot_core::writes`): OpenTrainer attaches as a debugger (visibly;
  it doesn't hide), sets a hardware write breakpoint on the value in every thread, and lists the
  instructions that wrote to it while the player played. Nothing in the game is changed; the
  breakpoints are cleared and the debugger leaves without closing the game. x64 and 32-bit games.
- **Code-patch cheats from the AI helper**: new tools `watch_writes_start` / `watch_writes_stop`
  (with a ready-made patch idea: the instruction's bytes replaced by NOPs). The helper now prefers
  this for "infinite" cheats, which needs no game restarts.
- **Pack repair after game updates** (`ot_packs::heal`, Trainer tab **Try to repair**, AI tool
  `pack_heal`): cheats whose signature stopped matching are found again by the closest match in the
  new code (only a single clear winner above 60% is accepted), get a fresh unique signature, and the
  pack is saved as your copy with a bumped version and the new build's hash (old file kept as
  `pack.json.bak`).
- **Engine detection** (`ot_core::engine`, AI tool `game_recon`): Unity (Mono / IL2CPP, with the
  Unity version), Unreal, Godot, GameMaker, Ren'Py, RPG Maker MV/MZ and XP/VX, XNA/FNA/MonoGame, with
  tips for finding cheats in each. Adds `um scan --json` output when the universal-modder `um`
  command is installed (optional).
- **Explain with AI** on a save comparison (backup vs now): the helper reads the diff with read-only
  save tools and labels each change in plain words ("player.gold went from 250 to 300: your gold").

- Tested for real on the testbed: asked for "Infinite health", the helper found the damage code with
  find-what-writes and saved a working code-patch cheat in one session, with no game restarts
  (about $0.19).

### Fixed
- Saving an AI pack for a game that already had one replaced the earlier cheats. `pack_write` now
  keeps them (new cheats with the same id replace old ones) and keeps the old file as
  `pack.json.bak`.

### Changed
- The roadmap splits AI II: engine fast paths (Mono / IL2CPP / Unreal readers), REA and local models
  move to 0.9.1.

## [0.8.0] - 2026-11-07

### Added
- **Make a trainer (AI)** view: pick your running game, tick the cheats you want, and an AI helper
  finds them with you. It asks for one small thing at a time ("Spend some gold, then press Done",
  "Does your gold show 12345?") with big Done / Yes / No buttons, shows what it is doing and the
  estimated cost, can be cancelled at any time, and saves a cheat pack marked as AI-made and tagged
  with your game's build. Load it on the Trainer tab as usual.
- **`ot-mcp`**: the guarded tool server the AI uses (MCP over stdio; also `opentrainer-desktop --mcp`).
  Tools: list processes, attach (anti-cheat guard first, saves backed up), game info, first/next
  scans, read, test writes (undone by `revert_all`, detach or exit), pointer scan and re-check, AOB
  maker and uniqueness check, pack validate / try / cheat on-off / write, save snapshot / diff /
  read. Every call is logged to `%LOCALAPPDATA%\OpenTrainer\ai-logs`.
- **AI helper sidecar** (`sidecars/ai-agent`, Node + the Claude Agent SDK). The agent gets no
  built-in tools (no shell, files or web): only the guarded OpenTrainer tools and "ask the player",
  everything else is denied, and no user settings or plugins are loaded. Default model Claude Opus
  5.5; uses your Claude subscription by default, or `ANTHROPIC_API_KEY` if you choose. A spending
  limit (default $5) stops long runs.
- **Sign in to Claude** from the helper: the app checks whether the helper's Claude engine is signed
  in (`auth status`) and, if not, offers a Sign in button that opens the engine's browser login.
  Claude settings inherited from another Claude program are not passed to the helper.
- Tested for real on the testbed: the helper made a working "Lots of gold" pack in one session
  (about $0.36). That run showed a value set by a cheat during the helper's tests stayed after
  "undo"; `cheat_set` now keeps the original value so `revert_all` and detach put it back.
- Decisions recorded (PLAN §8): keep the name OpenTrainer, Claude subscription as the default AI,
  a GitHub-repo pack index for sharing (later), no in-game overlay for exclusive fullscreen.
- New dependencies: `@anthropic-ai/claude-agent-sdk` (Anthropic's terms; its Windows engine is about
  330 MB) and `zod` (MIT), in the sidecar only.

### Changed
- `Try OpenTrainer.cmd` installs packages when the AI helper's are missing and builds the helper.
- The check script and CI build the AI helper and run its tests and typecheck.

### Not yet
- "Find what writes to this address" (hardware breakpoints) and local AI models are planned for a
  later version.

## [0.7.0] - 2026-11-07

### Added
- **Save editor** (`ot_saves::codec`, `ot_saves::editor`; **Edit saves** in the Save backups view).
  Open any file in a game's save folders and change its values:
  - **JSON** and **INI** saves: every value listed by path (`player.gold`, `items[2].count`,
    `Player.Gold`); an edit replaces only that value's text, so spacing and order stay as they were.
  - **Unreal saves** (`.sav`, GVAS, up to UE 5.3): the property tree, with numbers and true/false
    values editable in place. Text, engine structs, maps and sets are shown read-only.
  - **Unity PlayerPrefs** in the registry (`HKCU\Software\<company>\<product>`): found automatically
    for Unity games, backed up, compared and restored like a folder, and editable (each value keeps
    its registry type).
  - **Wrappers** detected and put back on saving: gzip, zlib, Base64, and checksums (CRC32, MD5,
    SHA-1, SHA-256 at the start or end of the file), which are recomputed after an edit.
  - **Hex view** for anything else (or any file, via "Show bytes"): pages of bytes, a number
    inspector (8/16/32/64-bit, float, double) and "Change bytes here".
  - Opening and saving with no changes gives back the same bytes. Files are parsed from their
    bytes; nothing is deserialised or run.
  - Saving is refused while the game runs or when the guard finds anti-cheat in the game's folder,
    and every save folder is backed up first ("Before an edit").
- **Quick codes**: a cheat pack's `saveCodes` now show as one-click buttons for that game (same
  checks and backup). `file` starts with a folder placeholder (`{documents}`, `{mygames}`,
  `{savedgames}`, `{appdata}`, `{localappdata}`, `{localappdatalow}`, `{playerprefs}`) and must be in
  one of the game's save folders. The validator now checks codecs, placeholders and values. The
  example packs have "999,999 gold" and "99 ammo" codes for the testbed's save file.
- New dependencies (small, MIT/Apache): `flate2`, `base64`, `crc32fast`, `md-5`, `sha1`.

### Changed
- SQLite saves are not in this version (they need a large library); the plan moves them later.

## [0.6.0] - 2026-11-07

### Added
- **Save backups** (`ot-saves`, desktop **Save backups** view). OpenTrainer finds where a game keeps
  its saves and backs them up when the game starts, when it closes, when you attach to it (before
  any cheat), and whenever you click **Back up now** (with an optional note).
  - Save locator: Unity (`<exe>_Data\app.info` → `AppData\LocalLow\<company>\<product>`), Unreal
    (`Saved\SaveGames`), Steam Cloud folders (`Steam\userdata\<id>\<appid>\remote`), and folders
    named like the game in `Documents\My Games`, `Documents`, `Saved Games`, `%APPDATA%`,
    `%LOCALAPPDATA%` and `LocalLow`. The game's install folder is never picked. You can add or
    remove folders yourself.
  - Backups live in `%LOCALAPPDATA%\OpenTrainer\saves`. Each file's contents are stored once, so
    repeated backups of unchanged saves cost no space. Automatic backups identical to the newest one
    are skipped, and only the newest 30 automatic backups are kept; your own backups stay until you
    delete them. Folders over 20,000 files or 2 GB are refused.
  - **Compare**: any backup with the saves now, or with the backup before it. Text saves show the
    changed lines (long one-line JSON is cut to the part that changed); other files show changed bytes
    with the 32-bit number at that spot.
  - **Restore**: only while the game is closed, after the anti-cheat guard checks the game's folder
    (no restores for online games). Every stored file is checked first, and your current saves are
    backed up ("Before a restore") so a restore can be undone. Files not in the backup are left alone
    unless you tick the box to delete them.
  - **Steam Cloud warnings** when Steam syncs a game's saves (and whether Steam is running).
- Guard: `ot_core::guard::check_install(exe)` checks a game that isn't running (install-folder markers
  and the online-only list), failing closed.
- Testbed: `save` writes `AppData\LocalLow\TestCo\Testbed\save1.json`, `load` reads it back, and the
  testbed writes a Unity-style `testbed_native_Data\app.info` so the locator finds it.
- New dependency: `similar` (Apache-2.0) for line diffs.

## [0.5.0] - 2026-11-06

### Added
- **Crash-safe restore** (`ot_core::journal`): every code change (patch) is recorded with its original
  bytes before it is written. The desktop app also writes that record to
  `%LOCALAPPDATA%\OpenTrainer\restore\<pid>-<start time>.json`. If OpenTrainer crashes or is killed,
  the next start puts the original code back (and says so in a notice), and a crash handler does the
  same when the app itself fails. Recovery attaches through the anti-cheat guard like any attach, only
  ever writes the recorded original bytes over the recorded patched bytes, and skips anything else.
  Journals of games that have closed are deleted.
- **Pointer scanner** (`ot_core::pointer`, desktop **Pointers** tab): finds paths from the exe or a
  DLL to a value, Cheat Engine style (`[[game.exe+0x3A000]+0x10]+0x18`), shortest first, up to 7
  levels. After restarting the game, enter the value's new address to keep only the paths that still
  work. Results survive detaching and re-attaching. Paths can be added to the address list.
- **Signature maker** (`ot_core::signature`, desktop **Signatures** tab): turns a code address into
  the shortest byte pattern that matches once in its module. Jump and call targets, RIP-relative and
  absolute addresses are wildcarded; struct offsets and constants are kept.
- **Pointer paths in the address list** and every address field: `[[module.exe+0x10]+0x18]+0x40`.
  Pointer entries follow the value when it moves.
- **`Try OpenTrainer.cmd`**: double-click to open the fake test game and the app together. It stops a
  leftover OpenTrainer web server that would block the app, says so if the app is already open, and
  keeps its window open when something fails.
- Testbed: gold pickups (`earn`) now run fixed-byte code too (marker 4).

### Changed
- All code writes go through `AttachedProcess::apply_code` / `revert_code`, which check the bytes
  first, refuse a change that overlaps another one, and record it. Raw code writes are no longer
  public. Detaching restores every recorded change, even one the trainer never turned off.
- The Trainer tab shows each patch as on or off from the bytes really in the game, refreshed on every
  poll.
- Code caves (`"kind": "cave"`) are not part of this version; the plan moves them to a later one.
  Packs that use them still get a clear message.

## [0.4.0] - 2026-11-06

### Added
- **Cheat packs** (`ot-packs`): the v1 pack format from `docs/PLAN.md` §4 as strict Rust types
  (unknown fields rejected) plus a JSON Schema at `crates/ot-packs/schema/pack.v1.json`.
- **Validator** with JSON paths in every message: unique ids, hotkey syntax and conflicts, settings that
  match the cheat's UI, numbers that fit their value type, slider ranges, address forms, AOB and byte
  syntax, patch length and safe instructions, SemVer and build hashes. Clear messages for an unknown
  schema version and for `cave` (arrives in v0.5).
- **`value` op**: a typed value at a static base (`module.exe+0x…`) or an AOB signature (raw, `abs32`
  for x86 code, `rip32` for x64 code), then a Cheat Engine-style pointer chain. Toggles freeze (at a
  fixed value or the current one), sliders and number inputs set (optionally held), buttons add or set.
  Frozen pointer chains are re-resolved on every refresh, so a moved object is followed.
- **`patch` op**: replaces a few bytes at an AOB-located site. The signature must match exactly once,
  the original bytes are checked before every write and on restore, and a site left patched (e.g. by a
  crash) is recognised. Replacement bytes are decoded with iced-x86 and only plain instructions are
  allowed: no calls, indirect jumps, system calls, interrupts, privileged or I/O instructions, and
  near jumps must land inside the game module. Limit 32 bytes.
- **Trainer runtime** (`ot_packs::Trainer`): checks the exe name, architecture and SHA-256 build
  against the pack (verified / listed / unknown / untagged), restores every patch on toggle off,
  unload, detach, drop and app exit. Shows what each cheat touches (module offset, bytes and
  disassembly for patches, address chain for values).
- **Hotkeys**: `F1`, `Ctrl+Num1`, `Alt+Shift+F5`, `Num+` … parsed and normalised; letters, digits and
  Space need Ctrl or Alt. The desktop app registers them with Win32 `RegisterHotKey` (no keyboard hooks)
  on its own thread while a pack is loaded, reports keys another app already holds, and unregisters
  them with the pack.
- **Pack store**: example packs built into the app and user packs in
  `%APPDATA%\OpenTrainer\packs\<game-id>\pack.json` (4 MiB limit).
- **`otpack` CLI**: `otpack validate <pack.json>...` and `otpack schema`.
- **Example packs** for the x64 and x86 testbed (`packs/examples/`): Infinite Health and Infinite Ammo
  (patches), Health Lock at 999 (freeze), +1000 Gold (button), Set Gold (number), Move Speed (slider).
- Desktop: **Trainer** tab (now the first tab): packs for the attached exe and arch, load/unload,
  cheats grouped by category with switches, buttons, number inputs and sliders, hotkey badges and
  conflicts, held values with Release, failed cheats with the reason, build badge and warning,
  AI-made badge, "What it changes" details, pack warnings. Hotkey presses refresh the UI at once.
- `ot-core`: AOB patterns and module scanning, guarded code writes (`write_code`: temporary
  protection change, restore, instruction-cache flush, logged), pointer-chain resolution, exe SHA-256,
  iced-x86 patch checker and disassembler.
- Testbed: the damage, fire and root-pointer code are inline assembly behind fixed marker bytes, so
  AOB signatures are identical on every build, compiler version and architecture.
- Tests: 25 `ot-packs` unit tests; 7 trainer integration tests on live x64 and x86 testbeds (every
  example cheat, hotkeys, restore on drop, static pointer chain, drifted/missing/ambiguous signatures,
  null pointer, build hash match, wrong exe/arch refused, leftover patch restored); 8 new `ot-core`
  unit tests (AOB, patch checker, hashing); desktop command tests; a local-only hotkey test that
  presses a real key (`cargo test -p opentrainer-desktop -- --ignored hotkeys`); 7 new frontend tests.

### Changed
- Testbed test helpers moved to a `testbed-harness` crate shared by `ot-core` and `ot-packs`. Their
  scratch folders now live under `target/tmp` instead of `%TEMP%` (the guard looks three folders up,
  and `%LOCALAPPDATA%` can hold a real anti-cheat folder on a developer PC).
- The app restores patches and stops freezes on exit.

## [0.3.0] - 2026-11-06

### Added
- **Scanner** (`ot_core::scan`): first scans (exact, between, unknown initial value) and next scans
  (exact, between, changed, unchanged, increased, decreased) for every value type. Runs on all cores
  in 16 MiB blocks. Each block is kept dense (snapshot + bitmap) or sparse (offsets + values),
  whichever is smaller. Options: writable-only, aligned/unaligned, float tolerance. Progress and
  cancel via `ScanControl`; a cancelled or failed next scan keeps the previous results. Unknown
  scans refuse above 4 GiB of memory. Scans read through `AttachedProcess`, so the guard applies.
- **Freeze loop** (`ot_core::Freezer`): rewrites frozen values every 50 ms on a background thread.
  Stops for good if the guard blocks or the game exits, and keeps the reason. Each freeze is logged
  once with its address and value; the repeated writes aren't logged one by one.
- Desktop: Scanner tab (type, scan kind, value/range, options, progress bar with cancel, first 200
  results with live and previous values, add to list); address list with live values (polled every
  500 ms), freeze toggles, inline value editing (a frozen value re-freezes at the new value), labels,
  manual add by `0x…` or `module.exe+0x…`; tabs for Scanner / Read & write / Modules.
- Tests: 9 scanner unit tests; 11 new integration tests on live testbeds (exact scan x64, x86 and
  unaligned; unknown → decreased/increased/unchanged/changed; float range; cancel keeps results;
  type mismatch; scans blocked after the guard trips; freezer holds/unfreezes; freezer stops on guard
  block and on game exit); 5 new frontend tests.

### Changed
- The session is now shared (`Arc<AttachedProcess>`) between the UI and the freeze thread.
- When several threads hit the 5-second guard recheck at once, only one of them runs it.
- Integration-test helpers moved to `crates/ot-core/tests/common/`.

### Fixed
- The app now notices a game that exits while attached even if nothing is reading its memory (it
  used to keep showing "Attached").

### Removed
- The unused `read_bytes` desktop command.

## [0.2.0] - 2026-11-06

### Added
- `ot-core` process list with image path and architecture (WOW64-aware via `IsWow64Process2`).
- `AttachedProcess`: guarded attach/detach, typed read/write (`i8`…`u64`, `f32`, `f64`, pointer-sized
  `ptr`), raw byte read/write, pointer reads, committed-region map (`VirtualQueryEx`), liveness check.
  Every write is logged with `tracing`.
- Module map via ToolHelp; 64-bit WOW64 layer modules are filtered out for x86 targets.
- Address expressions: `0x1234ABCD`, bare hex, `module.exe+0x1F0`.
- **Anti-cheat guard** (`ot_core::guard`) with no off switch: blocks on known anti-cheat processes,
  services, kernel drivers, modules in the target, install-folder markers (exe folder plus 3 parents),
  the built-in online-only list, or any check that fails to run. The signature list is compiled in.
  Attached sessions re-run the guard every 5 s and stay blocked once it trips.
- Desktop UI: process picker (search, hide Windows processes), guard verdict, attach/detach, read/write
  panel with activity log, module table.
- Tests: 34 `ot-core` unit tests (guard decisions, signature list shape, a source scan that fails if the
  guard gains an env var or feature switch), 11 integration tests against live x64 and x86 testbeds
  (read/write confirmed through the testbed's side channel, pointer chain, fake anti-cheat process,
  BattlEye/EAC folder markers, online-only exe name, anti-cheat appearing after attach), 8 frontend tests.

## [0.1.0] - 2026-11-06

### Added
- Monorepo skeleton: Rust workspace (`ot-core`, `ot-packs`, `ot-saves`, `ot-mcp`, `testbed-native`,
  desktop shell) and npm workspace for `apps/desktop`.
- Tauri 2 desktop app (React 19, TypeScript, Tailwind 4, Vite) that boots and shows its version via an
  `app_info` command.
- `testbed/native`: a fake console game with health, ammo, gold and speed behind a heap pointer chain
  (static root → World → Player → Stats), a JSON stdin/stdout side channel for tests, x86 builds, and a
  `shifted` feature that changes offsets.
- CI on `windows-latest`: rustfmt, clippy (`-D warnings`, plus x86 testbed), cargo tests, ESLint,
  TypeScript typecheck, Vitest.
- `scripts/check.ps1` (runs every CI check locally) and `scripts/bump-version.mjs` (bumps all manifests).
- MIT licence, README, STATUS.
