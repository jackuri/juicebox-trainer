# For cheat table makers

This page is for people who make Cheat Engine tables. It explains what JuiceBox Trainer does with your
work, how you're credited, and how to share your tables on its community list if you want to.
There's also a short message at the end that JuiceBox Trainer's maintainers send when inviting makers.

## What JuiceBox Trainer is

[JuiceBox Trainer](https://github.com/jackuri/juicebox-trainer) is a free, open-source (MIT) Windows app for
single-player PC games: a trainer with on/off cheats and hotkeys, a save manager and an AI helper
that makes cheat packs. It refuses games while their anti-cheat runs.

## What it does with a table

- **It never copies or re-uploads your table.** Players open your page in their own browser and
  download the `.CT` from you. JuiceBox Trainer only notices the file in their Downloads folder.
- **It converts the parts it can use** into a small "pack" on the player's PC: number values
  (health, money, ammo, also through pointers) and simple byte swaps.
- **It never runs your scripts.** Auto Assembler and Lua are read as text to understand common
  patterns, and the rest is listed as "not imported" with the reason. Your table stays the full
  experience; JuiceBox Trainer gives a simpler on/off view of part of it.
- **You're credited.** When a player converts your table, JuiceBox Trainer asks for your name and your
  table's page, keeps them in the pack, and shows them wherever the pack goes.

## The community list

JuiceBox Trainer has a public community list, [juicebox-packs](https://github.com/jackuri/juicebox-packs),
that every copy of the app can browse. It can hold two things for your tables:

1. **A link to your table's page** (no file). Players see "Tables other players found" for the game,
   with your name, and download from your page.
2. **A ready-made pack made from your table**, if you choose to share one. Then players can install
   it in one click, and see your name, your terms and your support link next to it.

Nothing of yours is put on the list as a pack without you, or without your permission.

## Your terms

When a pack is shared, the sharer picks the terms, and you choose them if you share it yourself:

- **Free to reuse and change (MIT).**
- **Shared with permission; the maker keeps all rights.** Players can use it, but not re-share or
  sell it elsewhere without your OK.
- **Other terms:** one short line, for example `CC BY 4.0`.

You can also add a **support link** (Patreon, Ko-fi, Buy Me a Coffee, GitHub or YouTube). It's shown
as "Support the maker" next to your pack.

## How to share a pack yourself

1. Install JuiceBox Trainer (1.17 or newer) and open your table in it: drop the `.CT` on the window, or
   download it and click **Turn into cheats**. Fill in your name and your table's page.
2. Try the cheats in the game.
3. In **Packs**, click **Share with everyone** next to it, sign in to GitHub once with the code it
   shows, choose **I did** for who made it, pick your terms, add your support link if you like and
   click **Share**.
4. Prove it's you, so the pack gets the **Shared by its maker** badge: link your GitHub account from
   your forum profile or your table's thread (or post the pull request's address there). A
   maintainer checks this, then merges it.

To change your terms or have a pack or link removed, open an issue on the community list or
reply on the pull request. Requests from makers are handled first.

## Invitation message

A short message for inviting a maker (by forum private message or on their table's thread). Change
the parts in brackets.

> Hi [name], thanks for your [game] table, it's great.
>
> I help with JuiceBox Trainer, a free open-source trainer app for single-player PC games
> (https://github.com/jackuri/juicebox-trainer). Players can turn a downloaded table into simple on/off
> cheats, and it credits the table's maker by name and links to their page. It never runs table
> scripts and never re-uploads tables.
>
> Would you like your table on our community list? You choose how:
> - just a link to your page (players still download from you), or
> - a ready-made pack made from your table, under your own terms (free to reuse, or "shared with
>   permission, all rights kept"), with a link to your Patreon / Ko-fi if you have one.
>
> If you'd rather not, no problem at all, and nothing of yours will be listed as a pack. Here's how
> it works: https://github.com/jackuri/juicebox-trainer/blob/main/docs/CREATORS.md
