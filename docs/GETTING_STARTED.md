# Getting started with JuiceBox Trainer

<!-- Made from apps/desktop/src/lib/guide.ts by `npm run guide`. Change that file, not this one. -->

The same tutorial and questions are in the app: open **Help**. Each lesson there has a **Show me**
button that opens the right screen. The [user guide](USER_GUIDE.md) explains everything in more detail.

## Tutorial

### 1. Your first cheats

Start a game with cheats and turn them on. (about 3 minutes)

1. Open **Games**. It lists the games installed on your PC (Steam, Epic, GOG and Xbox).
2. Click **Play with cheats** on a game. JuiceBox Trainer starts the game, waits for it, attaches to it and loads its cheats.
3. Or start the game yourself, open **Cheats**, pick it under **Your running games** and click **Attach**. Not in the list? Tick **Show all running programs** and pick the game.
4. No cheats for that game yet? Click **Get a pack…**, or make your own (the next lessons show how).
5. Turn cheats on and off with their switches, or with their keys (like `F1`) while you play.

**Tip:** Every change goes back to how the game had it when you turn a cheat off, close the game or click **Detach**. Your saves are backed up when you attach.

### 2. Make a cheat yourself (no AI)

Find a number in the game, like gold or health, and keep it where you want it. (about 5 minutes)

1. Start the game and attach to it in **Cheats**, then open **Find a value**.
2. Pick what to change (**Gold**, **Health**, **Ammo**) or type a name.
3. Type the number the game shows now, like `1250`, and click **Search**.
4. Change it in the game (spend some gold, take a hit), then type the new number. Do this a few times, until only a few places are left.
5. JuiceBox Trainer tries them one by one and asks **Does it show … now?** Answer **Yes** or **No**.
6. Found it? Choose **Keep it at** (it stays there, like infinite health) or **Set it once to**, and a number.
7. Click **Save as a cheat (with a hotkey)**. It goes in your own pack for the game, named after it with "(my cheats)", with the next free key.

**Tip:** No number on screen, like a health bar or a flashlight battery? Click **There's no number on screen**, then say **It went down**, **It went up** or **It stayed the same** each time it changes.

### 3. Make cheats with AI

Set up the AI helper once, then let it find cheats with you. (about 10 minutes)

1. Install **Node.js** (free): the LTS version from nodejs.org. You only do this once.
2. Open **Make a trainer (AI)** and click **Download the AI engine** (about 330 MB). Also only once.
3. Choose how to pay for the AI under **AI account**: your Claude subscription (click **Sign in** once), an Anthropic API key, or a free AI on your own PC (Ollama or LM Studio, slower and makes more mistakes).
4. Start the game, pick it, tick the **Cheats you want** (or type your own, like "never run out of potions") and click **Start**.
5. The helper asks you to do small things in the game, like "take some damage". Press **Done**, **Yes** or **No**, or type your own answer.
6. When it's done, the new pack is in **Cheats**, marked "AI-made".

**Tip:** You set the most it may spend. It can only use JuiceBox Trainer's safe tools, and its test changes are undone when it stops.

### 4. Fix a cheat while you play

Tell the AI what's wrong, in your own words, without stopping the game. (about 2 minutes)

1. With a pack loaded in **Cheats**, click **Not working? Fix with AI** under the cheat.
2. Say what's wrong, like "it only works with the gun I start with", and click **Fix it now**.
3. Want a cheat the pack doesn't have? Click **Ask the AI** at the top and say what you want.
4. Keep playing: your other cheats stay on. When it's done, your cheats are loaded again with the fix and turned back on.

**Tip:** It uses the AI account from **Make a trainer (AI)**, so set that up first (the lesson before this one).

### 5. Instant cheats for Unreal games

One click makes cheats for many games made with Unreal Engine. (about 2 minutes)

1. Start the game and load your save, so your character is in the world.
2. Attach to it in **Cheats**. For an Unreal Engine game with no pack yet, you see **Make instant cheats**: click it.
3. It can take a few minutes the first time on a big game. Then you get cheats like game speed, move speed, jump height and infinite health or ammo.

**Tip:** Not every one works in every game. If one does nothing, use **Not working? Fix with AI** on it. After a game update, click **Make them again**.

### 6. Edit and keep your cheats

Change cheats, pick your own keys and save the ones you like. (about 3 minutes)

1. A **pack** is the list of cheats for one game: a small file with no programs inside. Yours are in **Packs**.
2. Click **See and edit cheats** next to a pack. **Edit** on a cheat changes its name, value, key or note; then click **Save changes**.
3. In **Cheats**, the ★ puts a favourite at the top, and **Keys & note** under a cheat sets your own key or a controller combo like `LB+RB+DPadUp`.
4. Turn on the cheats you like, type a name like `Story mode` and click **Save what's on now**. **Apply** turns them all on again later.
5. **Export** saves a `.juice` file you can send a friend. Double-click a `.juice` file to open one.

**Tip:** Cheat Engine tables (`.CT` files) work too: in **Games**, click **Find a cheat table** on the game, download the table, then click **Turn into cheats**.

### 7. Cheats in web games

Games that run in a web page get cheats too. (about 5 minutes)

1. Open **Web games**. Under **Add a web game**, type a name, paste the game's address from your browser and click **Add**.
2. Click **Play**. The game opens in JuiceBox Trainer's own game window.
3. Under **Find a value**, type a number you see in the game and click **Find**. Change it in the game, then type the new number, or click **Went up** or **Went down**.
4. Click **Add to my cheats** and name it. Now you can **Set** it to a number or **Lock** it, and give it a key.
5. Or let the AI look: click **Find cheats with AI**, or **Ask the AI** to say what you want in your own words.

**Tip:** Found the same thing many times, like each enemy's health? **Combine…** makes one cheat for all of them.

### 8. Back up and edit saves

Never lose a save, and change values in save files. (about 3 minutes)

1. Saves are backed up for you when you attach, when the game starts and when it closes. **Back up now** makes one any time.
2. **Restore** puts a backup back (close the game first). Your current saves are backed up first, so a restore can be undone.
3. **Compare with now** shows what changed since a backup, and **Explain with AI** says what the values probably are.
4. To edit, open a save file, change a value and click **Save changes**. Close the game first.

**Tip:** Steam Cloud can overwrite a restored save. If Steam asks which copy to keep, choose the local one.

### 9. Share and level up

Share your packs with everyone, rate others' and earn XP. (about 3 minutes)

1. In **Packs**, click **Share with everyone** next to one of your packs. The first time, click **Sign in with GitHub** (a free account) and follow the steps.
2. A maintainer checks each pack before it's listed. Each accepted pack gives you **10 XP**.
3. Got a pack from **Community packs**? Under it, **Tried it? Rate it** with 1 to 3 stars for **2 XP**.
4. Click your level at the bottom of the menu to see the titles and the leaderboard, and to turn on the daily check-in (**2 XP** a day, **10 XP** more for every 5 days in a row).

**Tip:** No cheats for your game anywhere? In **Games**, click **Ask the community for cheats** on it.

## Questions

### Getting started

**Is JuiceBox Trainer free?**

Yes. It's free and open source. Only the AI helper can cost something, and only if you use it: it runs on your own Claude subscription or API key, or on a free AI on your own PC.

**Which games does it work with?**

PC games without anti-cheat: single-player games, co-op with friends, and online games that have no anti-cheat. Games that run in a web page work too, in **Web games**.

**Do I need to know about hacking or memory?**

No. **Play with cheats** does everything for you, **Find a value** asks simple questions, and the AI helper does the hard parts. The expert tools (**Scanner**, **Pointers**, **Memory**) are there if you want them.

**Where do I start?**

Open **Help** and follow the tutorial, starting with **Your first cheats**. Each lesson has a **Show me** button that opens the right screen.

**Windows said "Windows protected your PC" when I installed it**

JuiceBox Trainer isn't code-signed yet, which costs money and is planned. Click **More info**, then **Run anyway**. Only do this for the file from JuiceBox Trainer's releases page on GitHub.

### Safety

**Why does it refuse my game ("Anti-cheat found")?**

On purpose: JuiceBox Trainer never works while a game's anti-cheat runs, and that can't be switched off. If the game's maker offers a way to play without anti-cheat, start it that way. Anti-cheat from another game blocks too: close that game first.

**Can I get banned?**

Games played alone don't watch for cheats. In online games without anti-cheat, other players can still report you, so please don't spoil their fun in public, competitive or ranked matches. You use JuiceBox Trainer at your own risk (see the agreement in **About**).

**My antivirus warns about JuiceBox Trainer**

Trainers change other programs' memory, so some antivirus programs flag them. JuiceBox Trainer doesn't hide what it does. Make sure you downloaded it from its releases page on GitHub; you can compare the file with `SHA256SUMS.txt` there.

**Can a cheat break my game or my save?**

Cheats only change the running game, and every change goes back when you turn the cheat off or close the game. Your saves are backed up when you attach, so if a save goes wrong, **Restore** it in **Save backups**.

**Is it safe to open packs other people made?**

A pack is a list of cheats with no programs inside. JuiceBox Trainer checks every pack, shows you what's inside before installing it, and a pack can only change game values and a few checked bytes of game code. Only add web cheat files from people you trust.

### Using cheats

**A cheat does nothing**

Click **Not working? Fix with AI** under it and say what's wrong. Some games keep a value their own way, so a cheat made for another version may miss it. After a game update, try **Try to repair** first.

**A cheat stopped working after a game update**

Click **Try to repair** on the Cheats screen. If that doesn't fix it, use **Not working? Fix with AI**, or ask the AI helper (**Make a trainer (AI)**) to make the cheat again.

**How do I change a cheat's key, or use a controller?**

Open **Keys & note** under the cheat in **Cheats** and press the key you want. You can also set a controller button combo there, like `LB+RB+DPadUp`.

**Can I see my cheats on top of the game, or use my phone?**

**Show overlay** puts a see-through list over the game (windowed or borderless games, not exclusive fullscreen). **Phone remote** shows a QR code: scan it with the free **JuiceBox Remote** app for Android (it stays paired, even after a restart) or with any phone's camera to open the remote in its browser. The phone must be on the same Wi-Fi.

**What's the difference between "Keep it at" and "Set it once to"?**

**Keep it at** holds the number there the whole time, like infinite health. **Set it once to** changes it one time, and the game can change it again (spend the gold, take a hit).

### Making cheats

**Should I make cheats by hand or with AI?**

By hand (**Find a value**) is free and quick for numbers you can see, like gold or ammo. The AI helper is better for harder things: values with no number on screen, "enemies can't hurt me", or cheats that change how the game works.

**The game doesn't show a number (a health bar, a flashlight battery)**

In **Find a value**, click **There's no number on screen**. Each time it changes, say **It went down** or **It went up**; when it hasn't changed, say **It stayed the same**. For something that drains by itself, let it drain, then stop using it for a moment, a few times. Still stuck? Click **Ask the AI** in **Cheats**.

**The search finds too many places**

Keep going: change the number in the game again and type the new one. Each change rules out more places. When only a few are left, JuiceBox Trainer tries them and asks you.

**My cheat works until I restart the game**

Some games put a value somewhere new each time they start. When you click **Save as a cheat (with a hotkey)**, JuiceBox Trainer asks you to restart the game and find it once more, then click **Finish saving**. Keep JuiceBox Trainer open while you do that.

**Can I change a cheat after I made it?**

Yes. Click **See and edit cheats** next to the pack in **Packs**, then **Edit** on the cheat. You can change its name, the value it keeps, a slider's range, its key and your note. The old pack is kept as a backup.

### The AI helper

**What do I need for the AI helper?**

**Node.js** (free, from nodejs.org), the AI engine (click **Download the AI engine** in **Make a trainer (AI)**, about 330 MB), and a way to pay for the AI: your Claude subscription, an Anthropic API key, or a free AI on your own PC with Ollama or LM Studio.

**How much does the AI cost?**

With a Claude subscription it uses your plan. With an API key you pay for what it uses, and you set the most it may spend. Ollama and LM Studio are free, but slower and they make more mistakes. **Cheaper and faster** costs less than **Best results** and is fine for simple games.

**What can the AI do on my PC?**

Only JuiceBox Trainer's own safe tools: it can't run programs or read your files. It refuses games with anti-cheat, its test changes are undone when it stops, and every step is written down in the `ai-logs` folder.

**Can I tell the AI what I want in my own words?**

Yes. Type in the box under a question instead of pressing **Done**, **Yes** or **No**, or type in the box at the bottom while it works, like "infinite ammo only works with the pistol".

**The AI helper says "Not signed in"**

Click **Sign in** in **Make a trainer (AI)** and sign in to Claude in the browser that opens.

### Packs and sharing

**What is a pack?**

The list of cheats for one game, in a small file (`.juice`). It has no programs inside. Your packs are in **Packs**, and **Community packs** lists the ones other players shared.

**How do I share a pack with a friend?**

In **Packs**, click **Export** and save the `.juice` file, then send it. Your friend double-clicks it, or drops it on the JuiceBox Trainer window, checks what's inside and installs it.

**How do I share a pack with everyone?**

Click **Share with everyone** next to your pack and sign in with a free GitHub account. A maintainer checks it before it's listed, and each accepted pack gives you 10 XP.

**Can I use Cheat Engine tables?**

Yes. In **Games**, click **Find a cheat table** on the game, download the `.CT` file, then click **Turn into cheats** under **Cheat tables in your Downloads**. Scripts in tables are never run: the common kinds are redone JuiceBox Trainer's safe way, and the rest are left out.

**How do I earn XP and level up?**

10 XP for each pack or web cheats you share that's accepted, 2 XP for each community pack you rate, and with the daily check-in on, 2 XP each day you open JuiceBox Trainer plus 10 XP more for every 5 days in a row. Click your level at the bottom of the menu to see it all.

**Why do I need a GitHub account?**

The community list lives on GitHub. A free account shows who shared or rated a pack, so each player counts once and gets their XP. You only need it for sharing, rating and levels: cheats work without it.

### Web games

**Which web games work?**

Most games that run in a web page. Games inside another site's frame, or running in a background worker, may not show their numbers yet. After a game update, find its values again.

**How do I make one cheat for all the enemies?**

Find a few of them first ("Enemy 1", "Enemy 2"…), then click **Combine…**, tick them and click **Combine into one** (it shows how many). For enemies' health, choose **never lets it go above**, so dead ones stay dead. To cover every enemy in the game, **Ask the AI**.

**A web game froze or went blank**

Click **Reload game** on its cheats. Searching a big game's memory can make the page pause for a few seconds. Some web games also freeze when your internet drops.

### Saves

**How do I get an old save back?**

Close the game, open **Save backups**, pick the backup and click **Restore**. Your current saves are backed up first, so you can undo it.

**Can I move my saves to another Steam account?**

Yes, for games that keep saves per Steam account: in **Save backups**, use **Copy saves to another Steam account**, click **Check what will be copied**, then **Copy**. Everything is backed up first.

### Something went wrong

**A game closed or crashed**

JuiceBox Trainer says why under **Recent problems**: whether it closed normally or crashed, where it crashed, and which cheats were on. If it crashed right after you turned a cheat on, try the game without that cheat.

**JuiceBox Trainer can't connect to my game**

Start the game first, then click **Attach**. If Windows refuses, close JuiceBox Trainer and open it with right-click, **Run as administrator**. Games with anti-cheat are blocked on purpose.

**JuiceBox Trainer's screen went blank**

It restarts its screen by itself and notes it under **Recent problems**. If it stays blank, close JuiceBox Trainer and open it again. Only one JuiceBox Trainer can be open at a time.

**The overlay or the phone remote doesn't show**

The overlay needs the game in windowed or borderless mode. For the phone, the PC and the phone must be on the same Wi-Fi, and Windows must allow JuiceBox Trainer on private networks (it asks the first time).

**My internet stopped working**

JuiceBox Trainer never changes your network settings, and cheats only change numbers inside the game. Restart your router, or ask your internet provider if it stays down after restarting your PC.
