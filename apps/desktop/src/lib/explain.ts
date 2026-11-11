import { type Problem, toCommandError } from "./ipc";

/**
 * Error messages in plain words, each with what to try (v1.22.0). The
 * engine's messages are precise but technical ("could not read 4 byte(s) at
 * 0x1A2B3C"); players see these instead. Kinds not listed keep their message,
 * which OpenTrainer already writes for players.
 */
const PLAIN: Record<string, string> = {
  accessDenied:
    "Windows didn't let OpenTrainer into this game. Close OpenTrainer, then right-click it and choose \"Run as administrator\", and try again.",
  processNotFound: "That game isn't running any more. Start it again, then pick it in the list.",
  processExited: "The game closed. Start it again, then click Play to use cheats.",
  notAttached: "No game is connected yet. Start the game and click Play (or pick it in the list) first.",
  read:
    "OpenTrainer couldn't read that value right now. If you're on a menu or a loading screen, get into the game first and try again. If the game was updated, the cheat may need fixing.",
  nullPointer:
    "That value isn't there yet. Load your save and get into the game, then try again. If it keeps happening after a game update, the cheat needs fixing.",
  write:
    "The game didn't let OpenTrainer change that value. Get into the game (not a menu) and try again.",
  needsRepair:
    "The game was updated, so this cheat no longer fits it. Click Repair, or ask the AI helper to make the cheat again.",
  signatureNotFound:
    "This cheat couldn't find its place in the game, usually because the game was updated. Click Repair, or ask the AI helper to make it again.",
  signatureAmbiguous:
    "This cheat found more than one possible place in the game, so it didn't guess. Ask the AI helper to make it again for this version.",
  wrongArch: "This pack is for another version of the game (32-bit or 64-bit). Look for a pack made for your version.",
  scanTooLarge:
    "This search is too big. Choose a more exact kind of number, or search for the exact value you see in the game.",
  cancelled: "Stopped.",
  invalidValue: "That isn't a number this value can hold. Check the number and try again.",
  noWebGame: "Open the game first: go to Web games and click Play.",
  codeOverlap: "Two cheats try to change the same part of the game. Turn one of them off first.",
  pointerMapTooLarge:
    "The game uses too much memory for this search. Use the AI helper instead, or search again with fewer settings.",
};

/** Kinds whose own message helps more than a general sentence. */
const TECHNICAL = new Set(["win32", "journal", "debugger", "io"]);

/** The message to show for an error, in plain words with what to try. */
export function friendlyError(e: unknown): string {
  const err = toCommandError(e);
  if (err.closed) return `${err.closed.title}. ${err.closed.why}`;
  const plain = PLAIN[err.kind];
  if (plain) return plain;
  if (TECHNICAL.has(err.kind)) {
    return `Something went wrong (${err.message}). Try again; if it keeps happening, restart the game and OpenTrainer.`;
  }
  return err.message;
}

/** The problem as plain text, for copying into a message or a bug report. */
export function problemText(p: Problem): string {
  const when = new Date(p.id).toLocaleString();
  return [
    `${p.title} (${when})`,
    p.why,
    ...(p.advice.length ? ["What to try:", ...p.advice.map((a) => `- ${a}`)] : []),
    ...(p.details.length ? ["Details:", ...p.details.map((d) => `- ${d}`)] : []),
  ].join("\n");
}
