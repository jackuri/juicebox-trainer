import { useEffect, useState } from "react";
import { friendlyError } from "../lib/explain";
import { api, type LibraryGame, type Wanted } from "../lib/ipc";
import { players, wantedFor } from "../lib/wanted";

const small = "rounded-lg bg-zinc-800 px-2.5 py-1.5 text-xs ring-1 ring-zinc-700 hover:bg-zinc-700";

/**
 * Ask the community for cheats for a game that has none (v1.25.0), or add a vote when players
 * already asked. Opens the request form on GitHub in the browser: only the game's name and exe
 * go into it, and the player sends it themselves.
 */
export function AskCommunity({
  game,
  wanted,
  onError,
}: {
  game: LibraryGame;
  wanted: Wanted[];
  onError: (m: string) => void;
}) {
  const [opened, setOpened] = useState(false);
  const asked = wantedFor(game, wanted);
  async function go() {
    try {
      if (asked) await api.communityOpenRequest(asked.number);
      else await api.communityAsk(game.name, game.exe?.split(/[\\/]/).pop() ?? null);
      setOpened(true);
    } catch (e) {
      onError(friendlyError(e));
    }
  }
  return (
    <div className="flex w-full flex-col gap-1">
      <button
        className="w-full rounded-lg px-3 py-1 text-xs text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100"
        aria-label={asked ? `Add your vote for ${game.name}` : `Ask the community for cheats for ${game.name}`}
        onClick={() => void go()}
      >
        {asked ? `Asked by ${players(asked.votes)}: add your vote` : "Ask the community for cheats"}
      </button>
      {opened && (
        <p className="px-1 text-[11px] text-emerald-300" role="status">
          {asked
            ? "Opened in your browser: click 👍 on the request (sign in to GitHub first)."
            : "Opened in your browser: check the game's name and click Create (sign in to GitHub first)."}
        </p>
      )}
    </div>
  );
}

/**
 * "Most wanted": the games players asked cheats for, most votes first (v1.25.0). Games on this
 * PC get "Make cheats with AI", so anyone can help.
 */
export function MostWanted({ onOpenAi }: { onOpenAi: () => void }) {
  const [wanted, setWanted] = useState<Wanted[] | null>(null);
  const [mine, setMine] = useState<LibraryGame[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [all, setAll] = useState(false);

  useEffect(() => {
    let live = true;
    api.communityWanted(false).then(
      (w) => live && setWanted(Array.isArray(w) ? w : []),
      (e: unknown) => live && setError(friendlyError(e)),
    );
    api.libraryList().then(
      (g) => live && setMine(Array.isArray(g) ? g : []),
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, []);

  const owned = (w: Wanted) => mine.find((g) => wantedFor(g, [w]) !== null && !g.blocked) ?? null;
  const shown = (wanted ?? []).slice(0, all ? 50 : 8);

  return (
    <section className="flex flex-col gap-2 border-t border-zinc-900 pt-4" aria-label="Most wanted">
      <div className="flex items-center gap-2">
        <h2 className="text-lg font-semibold tracking-tight">Most wanted</h2>
        <span className="text-xs text-zinc-500">games players asked cheats for</span>
      </div>
      {error && <p className="text-sm text-amber-200">Couldn&apos;t load the list: {error}</p>}
      {wanted && wanted.length === 0 && (
        <p className="text-sm text-zinc-400">
          No requests yet. On the Games tab, games without cheats have &quot;Ask the community for cheats&quot;.
        </p>
      )}
      <ul className="flex flex-col gap-1.5">
        {shown.map((w) => {
          const have = owned(w);
          return (
            <li key={w.number} className="flex flex-wrap items-center gap-2 rounded-lg bg-zinc-900/70 px-3 py-2 text-sm ring-1 ring-zinc-800">
              <span className="min-w-0 flex-1 break-words">
                <span className="font-medium text-zinc-100">{w.game}</span>
                <span className="text-xs text-zinc-400"> · asked by {players(w.votes)}</span>
              </span>
              {have && (
                <button className="btn-accent rounded-lg px-3 py-1.5 text-xs font-semibold" onClick={onOpenAi}>
                  You have it: make cheats with AI
                </button>
              )}
              <button
                className={small}
                aria-label={`Vote for ${w.game}`}
                onClick={() => void api.communityOpenRequest(w.number).catch((e: unknown) => setError(friendlyError(e)))}
              >
                Vote
              </button>
            </li>
          );
        })}
      </ul>
      {wanted && wanted.length > 8 && (
        <button className="self-start text-xs text-zinc-400 hover:text-zinc-100" onClick={() => setAll((a) => !a)}>
          {all ? "Show fewer" : `Show more (${Math.min(wanted.length, 50)})`}
        </button>
      )}
    </section>
  );
}
