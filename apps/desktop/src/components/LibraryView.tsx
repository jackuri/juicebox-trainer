import { useCallback, useEffect, useMemo, useState } from "react";
import {
  api,
  type LibraryGame,
  onLibraryProgress,
  type PlayResult,
  type StoreKind,
  toCommandError,
} from "../lib/ipc";

const STORE: Record<StoreKind, string> = {
  steam: "Steam",
  epic: "Epic",
  gog: "GOG",
  xbox: "Xbox",
  own: "Added by you",
};

interface Props {
  /** A game was started and attached: show its cheats. */
  onPlayed: (r: PlayResult) => void;
  onOpenAi: () => void;
}

/** Games on this PC, with "Play with cheats". */
export function LibraryView({ onPlayed, onOpenAi }: Props) {
  const [games, setGames] = useState<LibraryGame[] | null>(null);
  const [filter, setFilter] = useState("");
  const [addPath, setAddPath] = useState("");
  const [playing, setPlaying] = useState<string | null>(null);
  const [progress, setProgress] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    api.libraryList().then(setGames, (e: unknown) => setError(toCommandError(e).message));
  }, []);

  useEffect(() => load(), [load]);
  useEffect(() => onLibraryProgress(setProgress), []);

  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return (games ?? []).filter((g) => !q || g.name.toLowerCase().includes(q));
  }, [games, filter]);

  async function play(g: LibraryGame) {
    setError(null);
    setPlaying(g.id);
    setProgress(`Starting ${g.name}…`);
    try {
      onPlayed(await api.libraryPlay(g.id));
    } catch (e) {
      const err = toCommandError(e);
      if (err.kind !== "cancelled") setError(err.message);
    } finally {
      setPlaying(null);
      setProgress(null);
    }
  }

  async function add(path = addPath) {
    setError(null);
    try {
      await api.libraryAdd(path.trim());
      setAddPath("");
      load();
    } catch (e) {
      setError(toCommandError(e).message);
    }
  }

  async function browse() {
    setError(null);
    try {
      const picked = await api.pickExe("Pick the game's .exe");
      if (picked) await add(picked);
    } catch (e) {
      setError(toCommandError(e).message);
    }
  }

  return (
    <main className="flex min-h-0 flex-col gap-3 overflow-y-auto p-4" aria-label="Games">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="font-medium">Games on this PC</h2>
        <input
          aria-label="Find a game"
          className="ml-auto w-56 rounded-md bg-zinc-900 px-2 py-1 text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
          placeholder="Find a game…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <button
          className="rounded-md bg-zinc-800 px-3 py-1 text-xs hover:bg-zinc-700"
          onClick={() => {
            setError(null);
            setGames(null);
            load();
          }}
        >
          Refresh
        </button>
      </div>

      {playing && progress && (
        <div className="flex items-center gap-3 rounded-md bg-sky-950/40 px-3 py-2 text-sm text-sky-200" role="status">
          <span className="flex-1">{progress}</span>
          <button className="rounded bg-sky-900/60 px-2 py-1 text-xs hover:bg-sky-800/60" onClick={() => void api.libraryCancel()}>
            Stop waiting
          </button>
        </div>
      )}
      {error && (
        <p className="rounded-md bg-red-950/40 px-3 py-2 text-sm text-red-300" role="alert">
          {error}
        </p>
      )}
      {games === null && <p className="text-sm text-zinc-500">Looking for games (Steam, Epic, GOG, Xbox)…</p>}
      {games !== null && games.length === 0 && (
        <p className="text-sm text-zinc-400">No games found. Add one below by its .exe.</p>
      )}

      <ul className="flex flex-col gap-2">
        {shown.map((g) => (
          <li key={g.id} className="flex flex-wrap items-center gap-3 rounded-md px-3 py-2 ring-1 ring-zinc-800" data-testid={`game-${g.id}`}>
            <div className="min-w-0 flex-1">
              <p className={`text-sm ${g.blocked ? "text-zinc-500" : ""}`}>
                {g.name} <span className="text-xs text-zinc-500">· {STORE[g.store]}</span>
              </p>
              <p className="flex flex-wrap gap-2 text-xs text-zinc-500">
                {g.blocked ? (
                  <span className="text-amber-400">Online / anti-cheat ({g.blocked.replace(/^anti-cheat found:? ?/, "") || "found"}): not supported</span>
                ) : g.pack ? (
                  <span className="text-emerald-400">
                    Pack: {g.pack.name}
                    {g.pack.ai ? " (AI-made)" : ""}
                  </span>
                ) : (
                  <span>No pack yet</span>
                )}
                {g.autoPreset && <span className="text-sky-300">Starts with: {g.autoPreset}</span>}
                {g.launch.kind === "manual" && !g.blocked && <span>You start it yourself; OpenTrainer waits for it</span>}
              </p>
            </div>
            {!g.blocked && !g.pack && (
              <button className="rounded-md bg-zinc-800 px-3 py-1.5 text-xs hover:bg-zinc-700" onClick={onOpenAi}>
                Make one with AI
              </button>
            )}
            <button
              className="rounded-md bg-zinc-800 px-3 py-1.5 text-xs hover:bg-zinc-700"
              aria-label={`Open folder of ${g.name}`}
              onClick={() => void api.libraryOpenFolder(g.id).catch((e: unknown) => setError(toCommandError(e).message))}
            >
              Folder
            </button>
            {g.store === "own" && (
              <button
                className="rounded-md bg-zinc-800 px-3 py-1.5 text-xs hover:bg-zinc-700"
                aria-label={`Remove ${g.name}`}
                onClick={() => void api.libraryRemove(g.id).then(load)}
              >
                Remove
              </button>
            )}
            <button
              className="rounded-md bg-emerald-700 px-3 py-1.5 text-xs font-medium hover:bg-emerald-600 disabled:cursor-not-allowed disabled:opacity-40"
              disabled={Boolean(g.blocked) || playing !== null}
              aria-label={`Play ${g.name}`}
              onClick={() => void play(g)}
            >
              {g.pack ? "Play with cheats" : "Play and attach"}
            </button>
          </li>
        ))}
      </ul>

      <div className="flex flex-wrap items-center gap-2 border-t border-zinc-900 pt-3 text-xs">
        <span className="text-zinc-400">Game not listed?</span>
        <button className="rounded-md bg-zinc-800 px-3 py-1 hover:bg-zinc-700" onClick={() => void browse()}>
          Browse for its .exe…
        </button>
        <span className="text-zinc-500">or paste the path:</span>
        <input
          aria-label="Game exe path"
          className="min-w-72 flex-1 rounded-md bg-zinc-900 px-2 py-1 font-mono outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
          placeholder="C:\Games\MyGame\MyGame.exe"
          value={addPath}
          onChange={(e) => setAddPath(e.target.value)}
        />
        <button
          className="rounded-md bg-zinc-800 px-3 py-1 hover:bg-zinc-700 disabled:opacity-40"
          disabled={!addPath.trim()}
          onClick={() => void add()}
        >
          Add game
        </button>
      </div>
    </main>
  );
}
