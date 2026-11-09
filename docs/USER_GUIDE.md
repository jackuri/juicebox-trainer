# OpenTrainer user guide

OpenTrainer gives you cheats in **single-player PC games**: infinite health, more gold, faster
movement and so on. It also backs up and edits your saves, and can make new cheats with AI.

It never works in online or multiplayer games, or while a game's anti-cheat is running. It checks
every game first and refuses those. That can't be switched off.

Some games come with anti-cheat but have an official way to play without it, like Halo: The Master
Chief Collection ("Anti-Cheat Disabled" when you press Play in Steam). Started that way, nothing of
the anti-cheat runs and OpenTrainer can attach. It checks again every few seconds and stops if the
anti-cheat starts. Games whose anti-cheat always runs (for example Elden Ring, Dragon Ball
Xenoverse 2 or Dune: Awakening started normally) stay refused. OpenTrainer never switches anti-cheat
off or gets around it.

## Install

1. Download `OpenTrainer_<version>_x64-setup.exe` from the
   [releases page](https://github.com/jackuri/opentrainer/releases).
2. Double-click it.
3. Windows may say **"Windows protected your PC"**. OpenTrainer isn't code-signed yet, which costs
   money and is planned. Click **More info**, then **Run anyway**.
4. Follow the installer. It installs for your Windows account only and doesn't need admin rights.

To uninstall, use **Windows Settings → Apps → Installed apps → OpenTrainer → Uninstall**. Your packs,
settings and save backups are kept (see "Where your things are" below). Ticking "Delete the
application data" also removes the downloaded AI engine.

**Antivirus warnings:** trainers read and change other programs' memory, so some antivirus programs
flag them. OpenTrainer doesn't hide what it does. If yours complains, check that you downloaded it
from the releases page above. You can compare the file with `SHA256SUMS.txt` there.

## The tabs

The tabs are in the bar on the left. The bottom of that bar says which game OpenTrainer is
attached to, if any.

| Tab | What it's for |
|---|---|
| **Games** | Your installed games (Steam, Epic, GOG, Xbox) with their cover art, and "Play with cheats" |
| **Cheats** | Attach to a running game, load its cheats and use them |
| **Packs** | Your cheat packs, sharing them, and packs from other players |
| **Save backups** | Back up, compare, restore and edit saves |
| **Make a trainer (AI)** | Let the AI helper find cheats for a game with you |
| **About** | Version, updates, this guide |

## Use cheats

The quickest way is **Games → Play with cheats**. It starts the game, waits for it, attaches, loads
the game's pack and turns on your start preset.

To keep a game out of the Games list, click **Hide** on it. Hidden games are remembered on this PC
only (nothing is uploaded). Tick **Show hidden** at the top to see them again; **Unhide** puts a game
back in the list. Hiding only changes the list: the game and its pack stay as they are.

Or do it yourself:

1. Start the game.
2. In **Cheats**, pick the game under **Your running games** on the left (click **Refresh** if you
   just started it). OpenTrainer checks it for anti-cheat. A game OpenTrainer doesn't recognise
   (not in the Games tab and no pack yet)? Tick **Show all running programs** and pick its exe.
   The game pickers in **Make a trainer (AI)** and **Save backups** work the same way.
3. Click **Attach**. Your saves are backed up first.
4. If there is one pack for the game, it is loaded for you (nothing is turned on yet). If there are
   several, click **Load** on the one you want. No pack yet? Click **Get a pack…**.
5. Turn cheats on and off with their switches or keys (shown next to each cheat, like `F1`).

When you close the game, unload the pack, detach or close OpenTrainer, every change goes back to how
the game had it. If OpenTrainer ever crashes, it undoes leftover changes the next time it starts.

### No pack for your game? Find a value

Attach to the game, then open **Find a value** (next to Trainer). It finds a number for you by
asking simple questions, with no value types or memory addresses:

1. Pick what to change (**Gold**, **Health**, **Ammo**, or type a name).
2. Type the number the game shows now (`1250` or `1,250`), then **Search**. No number on screen,
   like a health bar? Click **There's no number on screen**, then say whether it **went down** or
   **went up** each time you change it.
3. Change it in the game (spend some gold, take a hit) and type the new number. Repeat until only a
   few places are left. OpenTrainer then tries them one by one and asks **Does it show 625 now?**
   Answer **Yes** or **No**. Your real number is put back after each try. Some games only redraw
   the number when something happens: open a menu or move, then look again.
4. Found it? Choose **Keep it at** or **Set it once to** and a number, then:
   - **Use it now**: works right away, until you close the game (it's in the address list below).
   - **Save as a cheat (with a hotkey)**: adds it to your own pack for this game, "*Game* (my
     cheats)", with the next free key (`F1`, `F2`, …). If the value moves each time the game starts,
     OpenTrainer asks you to restart the game and find it once more, then click **Finish saving**.
     Keep OpenTrainer open while you do that.

The old tools (**Scanner**, **Pointers** and so on) are still there for experts. Scanner results now
update their values live.

### Make cheats yours

- **★ Favourites:** click the star to put a cheat at the top.
- **Keys & note** (under each cheat): press your own key for it, set a controller button combo like
  `LB+RB+DPadUp`, or write a note.
- **Presets:** turn on the cheats you like, type a name such as `Story mode` and click **Save what's
  on now**. **Apply** turns them all on again. Choose a preset to use when the game starts from the
  Games tab.
- **Show overlay:** a see-through list of cheats on top of the game. It works in windowed and
  borderless games, not exclusive fullscreen.
- **Phone remote:** click **Turn on** and scan the QR code with your phone on the same Wi-Fi. Each
  code works once. **Turn off** ends it. It's off every time OpenTrainer starts.

## Packs

A **pack** is the list of cheats for one game. It's a small file with no programs inside.
OpenTrainer checks every pack before using it, and a pack can only change game values and a few
checked bytes of game code. Everything is put back when the cheat is turned off.

In the **Packs** tab:

- **Share a pack:** click **Export** and save the `.otpack` file. Send it to a friend.
- **Open a pack someone sent you:** double-click the `.otpack` file, drop it on the OpenTrainer
  window, or click **Open a pack or table…**. A plain `pack.json` (from a forum or the community
  repository) works too. You see what's inside before anything is installed. If you
  already have a pack for that game, OpenTrainer asks before replacing it and keeps the old one as a
  backup.
- **Use a Cheat Engine table:** many games have free cheat tables (`.CT` files) shared on forums
  such as FearLessRevolution. Download one, then drop it on the window or click **Open a pack or
  table…**. OpenTrainer turns its number values (health, money, ammo…, also ones found through
  pointers) into a pack. You see which cheats came over and which entries were left out, and why.
  - **Scripts are never run.** OpenTrainer reads them only to understand them. The common kinds it
    redoes its own safe way: scripts that find a base address (so the values under it come over),
    save-slot numbers, and scripts that just swap a few bytes of game code (they become on/off
    cheats), and scripts that hook a line of game code just to grab a pointer (like `battle_ptr`):
    OpenTrainer reads that pointer itself instead. Scripts that inject their own logic ("unlimited
    health for the player only") are left out; freezing the value (e.g. Current Health) usually does
    the same job.
  - **Values found while you play:** some values (often battle stats) are found the moment the game
    uses them. Until then they show "not found yet". Click **Set** and do it in the game (start a
    battle, take a hit): OpenTrainer waits a few seconds and says "Waiting for the game…". To read
    them it attaches to the game as a debugger for a moment, like "find what writes". It never hides
    that; a game that refuses debuggers may close.
  - If the table doesn't say which game it's for, click **Choose the game's exe…** and pick the
    game's `.exe` (in its install folder).
  - In Cheats, type a value and click **Set**: it stays at that value (like ticking the box in
    Cheat Engine) until you click **Release**.
  - A table is made for one version of a game. If a value shows `??` after a game update, look for
    a newer table.
- **Share a pack with everyone:** click **Share with everyone** next to one of your packs.
  OpenTrainer puts it on the community list for you. You need a free GitHub account: the first time,
  click **Sign in with GitHub**, then **Copy the code and open GitHub**, paste the code on GitHub's
  page and click **Authorize**. Tick that every cheat works, optionally say which game version you
  tested, and click **Share**. A maintainer checks each pack before it shows up in the list. A pack
  made from someone's Cheat Engine table can only be shared if you made the table or its author
  allows it. **Sign out** (in the same window) removes the sign-in from this PC.
- **Community packs:** packs other players shared. Click **See what's inside**, then **Install**.
  To add yours to the list, click **Share your pack**.
- **Remove** moves a pack out of the way. A copy is kept in `%APPDATA%\OpenTrainer\packs-removed`.

When a game update breaks a pack, load it anyway and click **Try to repair** in Cheats. OpenTrainer
looks for the same code in the new version.

## Save backups

OpenTrainer backs up a game's saves when you attach to it, when the game starts and when it closes.
You can also click **Back up now**.

- **Compare with now** shows what changed since a backup. **Explain with AI** says what the values
  probably are, like "this is your gold".
- **Add a game or a save folder** OpenTrainer didn't find: click **Browse…** and pick the game's
  `.exe` or the folder.
- **Restore** puts a backup back. The game must be closed. Your current saves are backed up first,
  so a restore can be undone.
- **Edit saves:** open a save file, change a value and click **Save changes**. The game must be closed,
  and the saves are backed up first. **Quick codes** from a game's pack do common edits in one click.
  Saves OpenTrainer can show as named values: JSON, XML, INI (also GameMaker and Godot settings
  files), YAML, Unreal Engine saves, Unity PlayerPrefs and RPG Maker MV / MZ saves, also when they
  are compressed or Base64 text. Skyrim and Fallout 4 saves show their summary (name, level, place)
  but can't be changed. Anything else opens as bytes (hex).

- **Copy saves to another Steam account:** for games that keep saves per Steam account, pick the
  account to copy from and the one to copy to, click **Check what will be copied**, then **Copy**.
  Your Steam ID inside the saves is changed to the other account's. The saves you copy from stay
  as they are, and everything is backed up first. Then sign in to Steam with the other account and
  start the game.

Steam Cloud can overwrite restored saves. If Steam asks which copy to keep, choose the local one.

## Make a trainer with AI

The AI helper finds cheats for a game together with you. It asks you to do things in the game, like
"take some damage" or "spend some gold", then press **Done** or answer **Yes / No**. When it's done,
the new pack appears in Cheats, marked "AI-made".

You can also talk to it in your own words:

- **Instead of Done / Yes / No**, type your answer in the box under the question and click
  **Send**, for example "Yes, but fire damage still hurts".
- **While it works**, type in the box at the bottom, for example "Infinite ammo only works with the
  pistol". It reads your message after its current step and says what it will do about it.

What it needs:

1. **Node.js** (free): install the LTS version from [nodejs.org](https://nodejs.org).
2. **The AI engine:** the first time you open the tab, click **Download the AI engine**. It's
   Anthropic's Claude engine, about 330 MB. You can delete it again in About.
3. **A way to pay for the AI**, one of:
   - your Claude subscription. Click **Sign in** in the AI tab once.
   - an Anthropic API key, set as the Windows environment variable `ANTHROPIC_API_KEY`.
   - a free AI running on your own PC, with [Ollama](https://ollama.com) or
     [LM Studio](https://lmstudio.ai). This is slower and makes more mistakes.

The helper can only use OpenTrainer's guarded tools. It can't run programs or read your files. It
refuses games with anti-cheat, and its test changes are undone when it stops. You set the most it
may spend. Every step is logged in `%LOCALAPPDATA%\OpenTrainer\ai-logs`.

## Updates

Open **About** and click **Update**. If there's a newer version, OpenTrainer downloads it, puts the
games' original code back, installs it and opens again in the new version, all in one click. If you
already have the newest version, it says so. Updates are
checked against OpenTrainer's own signature before they're installed.

Automatic updates switch on once OpenTrainer is published publicly. Until then, About says so, and
new versions are on the releases page.

## Where your things are

| What | Folder |
|---|---|
| Packs | `%APPDATA%\OpenTrainer\packs` |
| Presets, your keys, notes | `%APPDATA%\OpenTrainer\settings` |
| Save backups | `%LOCALAPPDATA%\OpenTrainer\saves` |
| AI logs | `%LOCALAPPDATA%\OpenTrainer\ai-logs` |
| AI engine | `%LOCALAPPDATA%\org.opentrainer.desktop\ai-helper` |

## Something isn't working

- **"Anti-cheat found" / the game is refused:** this is on purpose. OpenTrainer doesn't work while
  anti-cheat runs. If the game's maker offers a way to play without anti-cheat, start it that way.
  Note that a running anti-cheat from *another* game also blocks: close that game first.
- **A cheat shows as failed:** the game was probably updated. Try **Try to repair**, or make a new
  pack with the AI helper.
- **The overlay doesn't show:** switch the game to windowed or borderless.
- **The phone can't connect:** the PC and phone must be on the same Wi-Fi, and Windows must allow
  OpenTrainer on private networks (it asks the first time).
- **The AI helper says "Not signed in":** click **Sign in** in the AI tab and sign in in the browser.
