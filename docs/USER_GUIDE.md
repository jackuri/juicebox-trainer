# OpenTrainer user guide

OpenTrainer gives you cheats in **single-player PC games**: infinite health, more gold, faster
movement and so on. It also backs up and edits your saves, and can make new cheats with AI.

It never works in online or multiplayer games, or in any game with anti-cheat. It checks every game
first and refuses those. That can't be switched off.

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
| **Games** | Your installed games (Steam, Epic, GOG, Xbox) and "Play with cheats" |
| **Cheats** | Attach to a running game, load its cheats and use them |
| **Packs** | Your cheat packs, sharing them, and packs from other players |
| **Save backups** | Back up, compare, restore and edit saves |
| **Make a trainer (AI)** | Let the AI helper find cheats for a game with you |
| **About** | Version, updates, this guide |

## Use cheats

The quickest way is **Games → Play with cheats**. It starts the game, waits for it, attaches, loads
the game's pack and turns on your start preset.

Or do it yourself:

1. Start the game.
2. In **Cheats**, pick the game in the list on the left. OpenTrainer checks it for anti-cheat.
3. Click **Attach**. Your saves are backed up first.
4. If there is one pack for the game, it is loaded for you (nothing is turned on yet). If there are
   several, click **Load** on the one you want. No pack yet? Click **Get a pack…**.
5. Turn cheats on and off with their switches or keys (shown next to each cheat, like `F1`).

When you close the game, unload the pack, detach or close OpenTrainer, every change goes back to how
the game had it. If OpenTrainer ever crashes, it undoes leftover changes the next time it starts.

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
  window, or click **Open a pack file…**. A plain `pack.json` (from a forum or the community
  repository) works too. You see what's inside before anything is installed. If you
  already have a pack for that game, OpenTrainer asks before replacing it and keeps the old one as a
  backup.
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

Steam Cloud can overwrite restored saves. If Steam asks which copy to keep, choose the local one.

## Make a trainer with AI

The AI helper finds cheats for a game together with you. It asks you to do things in the game, like
"take some damage" or "spend some gold", then press **Done** or answer **Yes / No**. When it's done,
the new pack appears in Cheats, marked "AI-made".

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

Open **About** to see your version and check for updates. If an update is found, **Install and
restart** puts the games' original code back, installs it and opens the new version. Updates are
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

- **"Anti-cheat found" / the game is refused:** this is on purpose. OpenTrainer doesn't work with
  those games.
- **A cheat shows as failed:** the game was probably updated. Try **Try to repair**, or make a new
  pack with the AI helper.
- **The overlay doesn't show:** switch the game to windowed or borderless.
- **The phone can't connect:** the PC and phone must be on the same Wi-Fi, and Windows must allow
  OpenTrainer on private networks (it asks the first time).
- **The AI helper says "Not signed in":** click **Sign in** in the AI tab and sign in in the browser.
