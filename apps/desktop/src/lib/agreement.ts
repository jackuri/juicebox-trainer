/**
 * The agreement every player signs before using JuiceBox Trainer (v1.18.0). Raise
 * `agreement::VERSION` in src-tauri when this changes in a way players must see again.
 * Not legal advice: have it checked by someone qualified.
 */
export const AGREEMENT_TITLE = "Before you use JuiceBox Trainer";

export const PLAY_FAIR: readonly string[] = [
  "Use cheats only in single-player games, or in co-op games with friends you invited yourself (private games or LAN).",
  "Never use cheats in games with strangers, in public, competitive or ranked matches, or in games with anti-cheat. JuiceBox Trainer refuses games with anti-cheat, but it can't recognise every online game.",
  "Other players can report you, and games, stores and websites can ban accounts for cheating, even in co-op.",
  "Cheats and save edits can break a game or a save. Keep backups (JuiceBox Trainer's Save backups can help).",
];

export const WAIVER: readonly string[] = [
  "I use JuiceBox Trainer and its cheats at my own risk. JuiceBox Trainer is free software, provided \"as is\" without any warranty (MIT licence).",
  "JuiceBox Trainer's makers and contributors are not responsible for anything that happens because I used it, including: my account being banned, suspended or limited in any game, store or website; being reported by other players; lost or broken saves, progress or items; and problems with a game or my PC.",
  "I'm responsible for following the rules of every game and website I use it with.",
  "Cheat packs and links shared by other players are theirs, not JuiceBox Trainer's. Packs for a game or website may be removed at any time.",
];
