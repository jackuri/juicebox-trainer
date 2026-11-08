import { type ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import { coverBytes } from "../lib/format";
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
    <main className="flex min-h-0 flex-col gap-4 overflow-y-auto p-6" aria-label="Games">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="text-2xl font-semibold tracking-tight">Games on this PC</h2>
        <input
          aria-label="Find a game"
          className="ml-auto w-56 rounded-lg bg-zinc-900 px-2 py-1 text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
          placeholder="Find a game…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <button
          className="rounded-lg bg-zinc-800 px-3 py-1 text-xs hover:bg-zinc-700"
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
        <div className="flex items-center gap-3 rounded-lg bg-sky-950/40 px-3 py-2 text-sm text-sky-200" role="status">
          <span className="flex-1">{progress}</span>
          <button className="rounded bg-sky-900/60 px-2 py-1 text-xs hover:bg-sky-800/60" onClick={() => void api.libraryCancel()}>
            Stop waiting
          </button>
        </div>
      )}
      {error && (
        <p className="rounded-lg bg-red-950/40 px-3 py-2 text-sm text-red-300" role="alert">
          {error}
        </p>
      )}
      {games === null && <p className="text-sm text-zinc-500">Looking for games (Steam, Epic, GOG, Xbox)…</p>}
      {games !== null && games.length === 0 && (
        <p className="text-sm text-zinc-400">No games found. Add one below by its .exe.</p>
      )}

      <ul className="grid grid-cols-[repeat(auto-fill,minmax(236px,1fr))] gap-4">
        {shown.map((g) => (
          <li
            key={g.id}
            className={`group flex flex-col overflow-hidden rounded-2xl bg-zinc-900/80 ring-1 ring-zinc-800 transition hover:ring-zinc-600 ${
              g.blocked ? "opacity-70" : ""
            }`}
            data-testid={`game-${g.id}`}
          >
            <Cover game={g} dim={Boolean(g.blocked)}>
              <span className="absolute left-2.5 top-2.5 rounded-full bg-black/45 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-white/90 backdrop-blur">
                {STORE[g.store]}
              </span>
              {g.pack && !g.blocked && (
                <span className="absolute right-2.5 top-2.5 rounded-full bg-green-500/90 px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider text-green-950">
                  Cheats
                </span>
              )}
            </Cover>
            <div className="flex flex-1 flex-col gap-2 p-3">
              <p className={`line-clamp-2 text-sm font-semibold leading-snug ${g.blocked ? "text-zinc-400" : ""}`}>{g.name}</p>
              <p className="flex flex-col gap-0.5 text-xs text-zinc-500">
                {g.blocked ? (
                  <span className="text-amber-400">Online / anti-cheat ({g.blocked.replace(/^anti-cheat found:? ?/, "") || "found"}): not supported</span>
                ) : g.pack ? (
                  <span className="text-emerald-300">
                    Pack: {g.pack.name}
                    {g.pack.ai ? " (AI-made)" : ""}
                  </span>
                ) : (
                  <span>No pack yet</span>
                )}
                {g.autoPreset && <span className="text-sky-300">Starts with: {g.autoPreset}</span>}
                {g.launch.kind === "manual" && !g.blocked && <span>You start it yourself; OpenTrainer waits for it</span>}
              </p>
              <div className="mt-auto flex flex-wrap items-center gap-1.5 pt-1">
                <button
                  className="btn-accent flex-1 rounded-lg px-3 py-2 text-xs font-semibold disabled:cursor-not-allowed disabled:opacity-40"
                  disabled={Boolean(g.blocked) || playing !== null}
                  aria-label={`Play ${g.name}`}
                  onClick={() => void play(g)}
                >
                  {g.pack ? "Play with cheats" : "Play and attach"}
                </button>
                <button
                  className="rounded-lg bg-zinc-800 px-2.5 py-2 text-xs ring-1 ring-zinc-700 hover:bg-zinc-700"
                  aria-label={`Open folder of ${g.name}`}
                  onClick={() => void api.libraryOpenFolder(g.id).catch((e: unknown) => setError(toCommandError(e).message))}
                >
                  Folder
                </button>
                {g.store === "own" && (
                  <button
                    className="rounded-lg bg-zinc-800 px-2.5 py-2 text-xs ring-1 ring-zinc-700 hover:bg-zinc-700"
                    aria-label={`Remove ${g.name}`}
                    onClick={() => void api.libraryRemove(g.id).then(load)}
                  >
                    Remove
                  </button>
                )}
                {!g.blocked && !g.pack && (
                  <button
                    className="w-full rounded-lg bg-emerald-950/60 px-3 py-1.5 text-xs font-medium text-emerald-200 ring-1 ring-emerald-800 hover:bg-emerald-900/70"
                    onClick={onOpenAi}
                  >
                    Make one with AI
                  </button>
                )}
              </div>
            </div>
          </li>
        ))}
      </ul>

      <div className="flex flex-wrap items-center gap-2 border-t border-zinc-900 pt-3 text-xs">
        <span className="text-zinc-400">Game not listed?</span>
        <button className="rounded-lg bg-zinc-800 px-3 py-1 hover:bg-zinc-700" onClick={() => void browse()}>
          Browse for its .exe…
        </button>
        <span className="text-zinc-500">or paste the path:</span>
        <input
          aria-label="Game exe path"
          className="min-w-72 flex-1 rounded-lg bg-zinc-900 px-2 py-1 font-mono outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
          placeholder="C:\Games\MyGame\MyGame.exe"
          value={addPath}
          onChange={(e) => setAddPath(e.target.value)}
        />
        <button
          className="rounded-lg bg-zinc-800 px-3 py-1 hover:bg-zinc-700 disabled:opacity-40"
          disabled={!addPath.trim()}
          onClick={() => void add()}
        >
          Add game
        </button>
      </div>
    </main>
  );
}

/** Picture URLs by game id, kept while the app runs (covers rarely change). */
const coverUrls = new Map<string, string>();

/** A picture URL for a game's cover. Kept per game and kind while the app runs; misses are asked again. */
async function coverUrl(id: string, kind: string): Promise<string | null> {
  const key = `${kind}|${id}`;
  const known = coverUrls.get(key);
  if (known) return known;
  const bytes = coverBytes(await api.libraryCover(id).catch(() => null));
  if (!bytes) return null;
  const url = URL.createObjectURL(new Blob([bytes]));
  coverUrls.set(key, url);
  return url;
}

/**
 * A game's cover: its artwork from the store's files on this PC, or its icon on a coloured
 * background. Until the picture is there (or if there is none): a colour from its name and its
 * initials.
 */
function Cover({ game, dim = false, children }: { game: LibraryGame; dim?: boolean; children?: ReactNode }) {
  const [loaded, setLoaded] = useState<{ id: string; url: string | null } | null>(null);
  useEffect(() => {
    if (!game.cover) return;
    let live = true;
    const key = `${game.cover}|${game.id}`;
    void coverUrl(game.id, game.cover).then((url) => live && setLoaded({ id: key, url }));
    return () => {
      live = false;
    };
  }, [game.id, game.cover]);
  const url = game.cover && loaded?.id === `${game.cover}|${game.id}` ? loaded.url : null;
  const [failed, setFailed] = useState<string | null>(null);
  const picture = url && failed !== url ? url : null;

  let h = 0;
  for (const ch of game.name) h = (h * 31 + ch.charCodeAt(0)) % 360;
  const initials = game.name
    .replace(/[^\p{L}\p{N} ]/gu, " ")
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0]!.toUpperCase())
    .join("");
  const art = picture && game.cover === "art";
  return (
    <div
      className={`relative flex aspect-[460/215] items-center justify-center overflow-hidden ${dim ? "grayscale" : ""}`}
      style={
        art
          ? undefined
          : {
              backgroundImage: `radial-gradient(120% 90% at 85% 0%, hsl(${(h + 50) % 360} 85% 60% / 0.55), transparent 60%), linear-gradient(135deg, hsl(${h} 65% 32%), hsl(${(h + 40) % 360} 60% 14%))`,
            }
      }
    >
      {art ? (
        <>
          <img
            src={picture}
            alt=""
            draggable={false}
            onError={() => setFailed(picture)}
            className="absolute inset-0 h-full w-full object-cover transition-transform duration-300 group-hover:scale-[1.03]"
          />
          <div className="pointer-events-none absolute inset-0 bg-gradient-to-t from-zinc-900/70 via-transparent to-black/25" />
        </>
      ) : picture ? (
        <img
          src={picture}
          alt=""
          draggable={false}
          onError={() => setFailed(picture)}
          className="h-[58%] w-auto max-w-[45%] object-contain drop-shadow-[0_6px_16px_rgb(0_0_0/0.5)]"
        />
      ) : (
        <span className="text-4xl font-black tracking-tight text-white/85 drop-shadow-[0_2px_8px_rgb(0_0_0/0.4)]">
          {initials || "?"}
        </span>
      )}
      {children}
    </div>
  );
}
