# JuiceBox Trainer user guide

JuiceBox Trainer (JUICE: Jackuri's Ultimate In-game Cheat Engine) was called **OpenTrainer** before
version 1.23.0. Updating from OpenTrainer removes the old program and brings your packs, cheats,
save backups and settings along on the first start; nothing is lost. Packs now end in `.juice`
(and web cheats in `.juiceweb`); OpenTrainer's `.otpack` and `.otweb` files still open.

JuiceBox Trainer gives you cheats in **PC games without anti-cheat** (single-player, co-op with
friends, and online games that have no anti-cheat): infinite health, more gold, faster
movement and so on. It also backs up and edits your saves, and can make new cheats with AI.

It never works while a game's anti-cheat is running. It checks every game first and refuses those.
That can't be switched off. Online games without anti-cheat are fine; please don't use cheats to
spoil other players' fun, for example in public, competitive or ranked matches.

Some games come with anti-cheat but have an official way to play without it, like Halo: The Master
Chief Collection ("Anti-Cheat Disabled" when you press Play in Steam). Started that way, nothing of
the anti-cheat runs and JuiceBox Trainer can attach. It checks again every few seconds and stops if the
anti-cheat starts. Games whose anti-cheat always runs (for example Elden Ring, Dragon Ball
Xenoverse 2 or Dune: Awakening started normally) stay refused. JuiceBox Trainer never switches anti-cheat
off or gets around it.

## Install

1. Download `JuiceBoxTrainer_<version>_x64-setup.exe` from the
   [releases page](https://github.com/jackuri/juicebox-trainer/releases).
2. Double-click it.
3. Windows may say **"Windows protected your PC"**. JuiceBox Trainer isn't code-signed yet, which costs
   money and is planned. Click **More info**, then **Run anyway**.
4. Follow the installer. It installs for your Windows account only and doesn't need admin rights.

To uninstall, use **Windows Settings → Apps → Installed apps → JuiceBox Trainer → Uninstall**. Your packs,
settings and save backups are kept (see "Where your things are" below). Ticking "Delete the
application data" also removes the downloaded AI engine.

**Antivirus warnings:** trainers read and change other programs' memory, so some antivirus programs
flag them. JuiceBox Trainer doesn't hide what it does. If yours complains, check that you downloaded it
from the releases page above. You can compare the file with `SHA256SUMS.txt` there.

## The agreement

The first time you open JuiceBox Trainer (and if the agreement ever changes), it asks you to read a
short agreement: play fair (cheats only alone or with friends you invited) and use JuiceBox Trainer at
your own risk (its makers aren't responsible if an account is banned or a save breaks). Tick the
box and type your name to sign. **About** shows it again.

## The tabs

The tabs are in the bar on the left. The bottom of that bar says which game JuiceBox Trainer is
attached to, if any.

| Tab | What it's for |
|---|---|
| **Games** | Your installed games (Steam, Epic, GOG, Xbox) with their cover art, and "Play with cheats" |
| **Cheats** | Attach to a running game, load its cheats and use them |
| **Packs** | Your cheat packs, sharing them, and packs from other players |
| **Save backups** | Back up, compare, restore and edit saves |
| **Make a trainer (AI)** | Let the AI helper find cheats for a game with you |
| **Web games** | Games that run in a web page: find and lock values |
| **Themes** | Change how JuiceBox Trainer looks |
| **Help** | What went wrong lately and what to try, and common questions |
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
   just started it). JuiceBox Trainer checks it for anti-cheat. A game JuiceBox Trainer doesn't recognise
   (not in the Games tab and no pack yet)? Tick **Show all running programs** and pick its exe.
   The game pickers in **Make a trainer (AI)** and **Save backups** work the same way.
3. Click **Attach**. Your saves are backed up first.
4. If there is one pack for the game, it is loaded for you (nothing is turned on yet). If there are
   several, click **Load** on the one you want. No pack yet? Click **Get a pack…**.
5. Turn cheats on and off with their switches or keys (shown next to each cheat, like `F1`).

When you close the game, unload the pack, detach or close JuiceBox Trainer, every change goes back to how
the game had it. If JuiceBox Trainer ever crashes, it undoes leftover changes the next time it starts.

### Instant cheats for Unreal games

For a game made with Unreal Engine (4.23 or newer), **Cheats** shows **Make instant cheats** when
there's no pack yet. Load your save first, so your character is in the world, then click it.
JuiceBox reads the game engine's own names (this can take a few minutes the first time on a big
game) and makes cheats straight away: game speed, move speed, jump height, gravity and jumps in the
air, plus infinite health, stamina, mana or ammo and setting your money when the game uses the
usual names. They're saved as a normal pack, so you can turn them on with their keys, change them
and share them. They work in many Unreal games, not every one: if one does nothing, the game keeps
that value its own way, and the AI helper can look for it. After a game update, click **Make them
again**.

### No pack for your game? Find a value

Attach to the game, then open **Find a value** (next to Trainer). It finds a number for you by
asking simple questions, with no value types or memory addresses:

1. Pick what to change (**Gold**, **Health**, **Ammo**, or type a name).
2. Type the number the game shows now (`1250` or `1,250`), then **Search**. No number on screen,
   like a health bar or a flashlight battery? Click **There's no number on screen**, then say
   whether it **went down** or **went up** each time you change it. For something that drains by
   itself, let it drain and say **It went down**, then stop using it for a moment and say **It
   stayed the same**; do that a few times. Big games take a minute or two the first time. Still
   stuck? Ask the AI (below).
3. Change it in the game (spend some gold, take a hit) and type the new number. Repeat until only a
   few places are left. JuiceBox Trainer then tries them one by one and asks **Does it show 625 now?**
   Answer **Yes** or **No**. Your real number is put back after each try. Some games only redraw
   the number when something happens: open a menu or move, then look again.
4. Found it? Choose **Keep it at** or **Set it once to** and a number, then:
   - **Use it now**: works right away, until you close the game (it's in the address list below).
   - **Save as a cheat (with a hotkey)**: adds it to your own pack for this game, "*Game* (my
     cheats)", with the next free key (`F1`, `F2`, …). If the value moves each time the game starts,
     JuiceBox Trainer asks you to restart the game and find it once more, then click **Finish saving**.
     Keep JuiceBox Trainer open while you do that.

The old tools (**Scanner**, **Pointers** and so on) are still there for experts. Scanner results now
update their values live. **Memory** shows the game's memory as a grid of bytes, updated every
second (changed bytes light up). Click a byte to see the numbers it starts, change it, follow a
pointer or add it to the address list. The ⌗ button in the address list opens an entry there.
Changes to the game's code are put back when you detach.

### See and edit your cheats

**See and edit cheats** (in **Packs** next to a pack, on a game in **Games**, or in **Cheats** while
you play) shows every cheat in the pack by group: what it does and its key. No game needed. Click
**Edit** on one to change its name, group, description, the value a switch keeps it at, a slider's
range, a button's amount, its key and your note, or to remove it; the star marks favourites. Click
**Save changes**: the edit is checked first, and the old pack is kept as a backup (a built-in pack
is saved as your own copy). Where a cheat looks in the game never changes here, so an edit can't
break it. If the pack is in use in Cheats, saving loads it again: turn your cheats back on.

A cheat that doesn't work? **It doesn't work: ask the AI to fix it**, say what's wrong, then **Open
the AI helper**: it opens with your request filled in. Start the game and click **Start**. If the
game is already running with this pack loaded in Cheats, it opens the AI helper right there instead
(see below).

### Fix or make cheats with AI while you play

In **Cheats**, with a pack loaded, each cheat has **Not working? Fix with AI**. Say what's wrong
("it only works with the gun I start with") and click **Fix it now**. Want a cheat the pack doesn't
have? Click **Ask the AI** at the top and say what you want ("infinite flashlight battery: it
drains, but there's no number").

Keep playing: your other cheats stay on while the AI works. It may ask you to do small things in the
game (press **Done**, **Yes** or **No**, or type your own answer), and you can tell it things while
it works. When it's done, your cheats are loaded again with the fix, and turned back on. **Stop**
ends it (a fix it already saved is kept). It uses the AI settings from **Make a trainer (AI)** (set it up
there first).

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
  code works once. **Turn off** ends it. It's off every time JuiceBox Trainer starts.

## Packs

A **pack** is the list of cheats for one game. It's a small file with no programs inside.
JuiceBox Trainer checks every pack before using it, and a pack can only change game values and a few
checked bytes of game code. Everything is put back when the cheat is turned off.

In the **Packs** tab:

- **Share a pack:** click **Export** and save the `.juice` file. Send it to a friend.
- **Open a pack someone sent you:** double-click the `.juice` file (or an `.otpack` from OpenTrainer), drop it on the JuiceBox Trainer
  window, or click **Open a pack or table…**. A plain `pack.json` (from a forum or the community
  repository) works too. You see what's inside before anything is installed. If you
  already have a pack for that game, JuiceBox Trainer asks before replacing it and keeps the old one as a
  backup.
- **Use a Cheat Engine table:** many games have free cheat tables (`.CT` files) shared on forums
  such as FearLessRevolution. The easy way: in **Games**, click **Find a cheat table** on the game,
  then **Search FearLess Revolution**, or one of the other places: the **Cheat Engine forum**,
  **OpenCheatTables**, **GitHub**, **Reddit r/cheatengine** or **The whole web** (only the game's
  name is sent). If other players already linked the
  game's table page, a green button opens that page directly. Your browser opens the page or the
  search; open the table's page and download its `.CT` file (FearLess Revolution asks you to sign
  in first). Back in
  JuiceBox Trainer, **Cheat tables in your Downloads** shows it within a few seconds, already matched to
  the game: click **Turn into cheats**. You can also drop a table on the window or click **Open a
  pack or table…**. Before installing, type who made the table and its page address: they're kept
  with the pack and shown to anyone it's shared with. JuiceBox Trainer never downloads or re-uploads
  tables itself.
  - **Tables other players found:** **Find a cheat table** also lists tables other players linked
    for that game, with who made them. **Open its page** opens the table's own page. Some tables are
    on the community list itself, because their maker gave them an open licence or said they may be
    shared there: **Turn into cheats now** gets them in one click, with the maker's credit and terms.
  - **Credit and terms:** community packs show who made the cheats, their terms (MIT unless the
    maker chose others, such as "shared with permission; the maker keeps all rights"), a **Shared
    by its maker** badge when the maker shared it, and **Maker's page** / **Support the maker**.
    When you share a pack, say who made it and pick the terms; if someone else made it, pick the
    terms they agreed to. Table makers: see [For cheat table makers](CREATORS.md).
  - **Share a table's link:** after turning a table into cheats (with its maker and page filled
    in), click **Share the table's link** next to it in **Packs**. Only the link goes to the
    community list, so other players find the table on its maker's page. JuiceBox Trainer turns its number values (health, money, ammo…, also ones found through
  pointers) into a pack. You see which cheats came over and which entries were left out, and why.
  - **Scripts are never run.** JuiceBox Trainer reads them only to understand them. The common kinds it
    redoes its own safe way: scripts that find a base address (so the values under it come over),
    save-slot numbers, and scripts that just swap a few bytes of game code (they become on/off
    cheats), and scripts that hook a line of game code just to grab a pointer (like `battle_ptr`):
    JuiceBox Trainer reads that pointer itself instead. Scripts that inject their own logic ("unlimited
    health for the player only") are left out; freezing the value (e.g. Current Health) usually does
    the same job.
  - **Values found while you play:** some values (often battle stats) are found the moment the game
    uses them. Until then they show "not found yet". Click **Set** and do it in the game (start a
    battle, take a hit): JuiceBox Trainer waits a few seconds and says "Waiting for the game…". To read
    them it attaches to the game as a debugger for a moment, like "find what writes". It never hides
    that; a game that refuses debuggers may close.
  - If the table doesn't say which game it's for, click **Choose the game's exe…** and pick the
    game's `.exe` (in its install folder).
  - In Cheats, type a value and click **Set**: it stays at that value (like ticking the box in
    Cheat Engine) until you click **Release**.
  - A table is made for one version of a game. If a value shows `??` after a game update, look for
    a newer table.
- **Share a pack with everyone:** click **Share with everyone** next to one of your packs.
  JuiceBox Trainer puts it on the community list for you. You need a free GitHub account: the first time,
  click **Sign in with GitHub**, then **Copy the code and open GitHub**, paste the code on GitHub's
  page and click **Authorize**. Tick that every cheat works, optionally say which game version you
  tested, and click **Share**. A maintainer checks each pack before it shows up in the list. A pack
  made from someone's Cheat Engine table can only be shared if you made the table or its author
  allows it. **Sign out** (in the same window) removes the sign-in from this PC.
- **Community packs:** packs other players shared. Click **See what's inside**, then **Install**.
  To add yours to the list, click **Share your pack**.
- **Remove** moves a pack out of the way. A copy is kept in `%APPDATA%\JuiceBox Trainer\packs-removed`.

When a game update breaks a pack, load it anyway and click **Try to repair** in Cheats. JuiceBox Trainer
looks for the same code in the new version.

## Save backups

JuiceBox Trainer backs up a game's saves when you attach to it, when the game starts and when it closes.
You can also click **Back up now**.

- **Compare with now** shows what changed since a backup. **Explain with AI** says what the values
  probably are, like "this is your gold".
- **Add a game or a save folder** JuiceBox Trainer didn't find: click **Browse…** and pick the game's
  `.exe` or the folder.
- **Restore** puts a backup back. The game must be closed. Your current saves are backed up first,
  so a restore can be undone.
- **Edit saves:** open a save file, change a value and click **Save changes**. The game must be closed,
  and the saves are backed up first. **Quick codes** from a game's pack do common edits in one click.
  Saves JuiceBox Trainer can show as named values: JSON, XML, INI (also GameMaker and Godot settings
  files), YAML, SQLite databases, Unreal Engine saves, Unity PlayerPrefs and RPG Maker MV / MZ saves, also when they
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

The helper can only use JuiceBox Trainer's guarded tools. It can't run programs or read your files. It
refuses games with anti-cheat, and its test changes are undone when it stops. You set the most it
may spend. Every step is logged in `%LOCALAPPDATA%\JuiceBox Trainer\ai-logs`.

Choices on the AI screen (1.21 and newer):

- **Let the helper look at the game window**, on unless you untick it: it takes pictures of the
  game's window (only that window) to read numbers itself, so it asks you less. The pictures go to
  the AI you picked and aren't saved. Some full-screen games can't be pictured; then it asks you.
- **How hard it tries:** **Best results** (Claude Opus) or **Cheaper and faster** (Claude Sonnet),
  which is fine for simple games.

The helper also checks first whether you or the community list already have cheats for the game,
looks at the values next to one it found (health is often next to max health and ammo), only saves
cheats it tried in the game, and keeps a short note of what worked for next time (in
`%LOCALAPPDATA%\JuiceBox Trainer\ai-notes`; delete the folder to forget them).

## Web games

Some games run in a web page. JuiceBox Trainer can open them in its own window and change their
numbers.

1. Open **Web games**. Under **Add a web game**, type a name and paste the game's address (copy it
   from your browser), then click **Add**.
2. Click **Play**. The game opens in JuiceBox Trainer's game window. Play as usual (sign in, start the
   game).
3. Back in JuiceBox Trainer, under **Find a value**, type a number you see in the game (gold, health,
   ammo…) and click **Find**. Change it in the game (spend gold, take a hit), then click **It's now
   this value** with the new number, or **Went up** / **Went down**. Repeat until only a few are
   left.
4. Click **Add to my cheats** and name it. Now you can **Set** it to any number or **Lock** it.
   Your cheats are saved for that game.
5. To use a key while you play, click the **Key** box next to a cheat and press a key (for example
   F1). While the game window is in front, the key locks or unlocks the cheat; choose **Set to…**
   to set it once instead. A note in the corner of the game says what the key did. Backspace in
   the Key box removes the key.
6. Or let the AI helper look: with the game open, click **Find cheats with AI**, tick what you'd
   like and click **Start**. It may ask you to read a number or do something in the game, then
   adds the cheats that work to the game's cheats. Keep the game window open while it works.
7. **Ask the AI** (in the game's cheats) lets you tell the helper what you want or what you're
   stuck on, in your own words, while you play. **Fix with AI** next to a cheat asks it to fix
   that one. Your locks stay on while it works. It may change or put together the cheats you
   asked about (the old list is kept as a backup), and you can type to it while it works.

**One cheat for many of the same thing.** Found several cheats for the same kind of thing, like
"Weak enemy 1", "Weak enemy 2"…? Click **Combine…**, tick them and click **Combine into one**. It
works when they sit evenly spaced in the game's memory (a list of enemies) or differ in one part
of their name. Name it, choose how its lock keeps the number, and save. **Never above** lowers a
number but never raises it: right for enemies' health, so dead ones stay dead. A combined cheat
covers the places you found and the ones between them; to cover every enemy in the game, ask the
AI.

### Star ratings

Community packs show how players rate them: ★ broken, ★★ works but has bugs, ★★★ works great,
with the average and how many players rated. Tick **Best rated first** in **Community packs** to
see the best ones at the top. Installed a community pack? **Tried it? Rate it** under it: click 1,
2 or 3 stars. Rating uses the same free GitHub sign-in as sharing (the first time, you paste a code
on GitHub's page). Each account counts once per pack; rating again changes it. New ratings show up
on the list within the hour.

### Levels and titles

Sharing and rating earn **packer XP**:

- **10 XP** for each pack or web cheats you share that's accepted into the community list;
- **2 XP** for each community pack you rate (your own too; once per pack).

Your level, title and XP bar are at the bottom of the menu on the left. Click it to see how to earn
XP, all the titles and the leaderboard. Levels go from 1 to 99: level 2 takes 10 XP, and each next
level takes 10 XP more than the one before. Titles: Recruit Packer (level 1), Pack Rat (3), Intern
Packer (5), Pro Packer (10), Pack Master (20), Leader of the Pack (30), Pack King (45), Pack Legend
(60) and Pack God (99). A **Level up!** card shows when you reach a new level.

XP goes with your GitHub name, the same sign-in as sharing and rating. GitHub counts it every hour,
so new XP shows up within the hour. The leaderboard shows GitHub names.

**Daily check-in:** click your level, then tick **Check me in every day I open JuiceBox Trainer**.
From then on, each day you open the app gives **2 XP**, and every **5 days in a row** give **10 XP**
more. A card shows your days in a row: five boxes fill up toward the next reward. It puts a 👍 with
your GitHub name on that day's check-in page of the community list (anyone can see it). Days go by
world time (UTC), so a new day may start in your afternoon or evening; just after it starts, the
check-in can take a few minutes to open.

### Ask for cheats, or help others

- **No cheats for your game?** On the **Games** tab, click **Ask the community for cheats** on it.
  Your browser opens a request on the community page, already filled in with the game's name; sign
  in to GitHub and click **Create**. If other players already asked, the button says how many and
  opens their request instead: add a 👍 there.
- **Most wanted** (in **Packs**) lists the games players asked for, most votes first. If you have
  one, click **You have it: make cheats with AI** and share what the helper makes.
- When the AI helper finishes with cheats that work, it offers **Share with everyone** straight away.

### Sharing web cheats

- **With friends:** click **Share** next to the game, then **Save to a file**. Send the file to your
  friend. They open **Web games**, click **Open a cheats file**, check what's in it and click
  **Add to my web games**.
- **With everyone:** click **Share**, then **Share with everyone…** (you sign in to GitHub once). A
  maintainer checks the cheats before they're listed.
- **Getting cheats:** under **Web cheats from the community**, click **Get these cheats**.

Only add cheat files from people you trust. Web cheats aren't listed for competitive games
(ranked, PvP, leaderboards) or games with items that can be traded for real money.

**Play fair:** cheats are fine alone, with friends, or in online games that have no anti-cheat.
Please don't spoil other players' fun in public, competitive or ranked matches: other players can
report you and sites can ban accounts.

Games inside another site's frame, or running in a background worker, may not show their numbers
yet. If a game is updated, find its values again.

## Themes

Open **Themes** and click a look. It changes straight away and is remembered next time; the in-game
overlay follows. **JuiceBox Trainer** is the standard look. There are game-inspired ones (Retro Arcade,
Next-Gen Blue, Console Green, Pocket LCD, Visor HUD, Wasteland Terminal, Amber Terminal, Inferno)
and a light one, **Daylight**.

The juice box dresses up for the holidays (Valentine's in February, Halloween in October,
Thanksgiving in November, Christmas in December) in the app's corner and on the taskbar. Untick
**Holiday juice boxes** in **Themes** to keep the everyday one. It also winks when something worked
and shows crossed-out eyes when something went wrong.

## Updates

Open **About** and click **Update**. If there's a newer version, JuiceBox Trainer downloads it, puts the
games' original code back, installs it and opens again in the new version, all in one click. If you
already have the newest version, it says so. Updates are
checked against JuiceBox Trainer's own signature before they're installed.

Automatic updates switch on once JuiceBox Trainer is published publicly. Until then, About says so, and
new versions are on the releases page.

## Where your things are

| What | Folder |
|---|---|
| Packs | `%APPDATA%\JuiceBox Trainer\packs` |
| Presets, your keys, notes | `%APPDATA%\JuiceBox Trainer\settings` |
| Save backups | `%LOCALAPPDATA%\JuiceBox Trainer\saves` |
| AI logs | `%LOCALAPPDATA%\JuiceBox Trainer\ai-logs` |
| AI engine | `%LOCALAPPDATA%\org.juiceboxtrainer.desktop\ai-helper` |
| Problems (Help page) | `%LOCALAPPDATA%\JuiceBox Trainer\problems.json` |

## Something isn't working

When something goes wrong, JuiceBox Trainer says what happened and what to try, and keeps it on the
**Help** page (a red dot shows on **Help** when there's something new):

- **A game closed:** the Cheats screen says whether it closed normally or crashed, where it crashed
  (the game itself, or the graphics driver), which cheats were on and what you changed just before.
  If it crashed right after you turned a cheat on, try the game without that cheat.
- **A web game page crashed or froze:** its cheats show what happened and a **Reload game** button.
- **JuiceBox Trainer's screen went blank or froze:** it restarts its screen by itself and says so.

Click **Copy details** on a problem to paste it into a message when asking for help. The list stays
on your PC (`%LOCALAPPDATA%\JuiceBox Trainer\problems.json`).

- **"Anti-cheat found" / the game is refused:** this is on purpose. JuiceBox Trainer doesn't work while
  anti-cheat runs. If the game's maker offers a way to play without anti-cheat, start it that way.
  Note that a running anti-cheat from *another* game also blocks: close that game first.
- **A cheat shows as failed:** the game was probably updated. Try **Try to repair**, or make a new
  pack with the AI helper.
- **The overlay doesn't show:** switch the game to windowed or borderless.
- **The phone can't connect:** the PC and phone must be on the same Wi-Fi, and Windows must allow
  JuiceBox Trainer on private networks (it asks the first time).
- **The AI helper says "Not signed in":** click **Sign in** in the AI tab and sign in in the browser.
