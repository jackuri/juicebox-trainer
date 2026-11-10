import { useCallback, useEffect, useState } from "react";
import { api, toCommandError, type WebGame } from "../lib/ipc";
import { WebCheatsPanel } from "./WebCheatsPanel";

const button = "rounded-lg bg-zinc-800 px-3 py-1.5 text-xs hover:bg-zinc-700 disabled:cursor-not-allowed disabled:opacity-40";
const field = "rounded-lg bg-zinc-950 px-3 py-1.5 text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600";

const host = (url: string) => {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
};

/** Games that run in a web page: add them, open them in OpenTrainer's game window, find and lock values. */
export function WebGamesView() {
  const [games, setGames] = useState<WebGame[] | null>(null);
  const [current, setCurrent] = useState<WebGame | null>(null);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [confirmRemove, setConfirmRemove] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    api.webGames().then((g) => setGames(Array.isArray(g) ? g : []), (e: unknown) => setError(toCommandError(e).message));
  }, []);

  useEffect(() => {
    load();
    // Notice when the player closes the game window.
    const check = () => {
      api.webCurrent().then((g) => setCurrent(g ?? null), () => undefined);
    };
    check();
    const timer = window.setInterval(check, 2000);
    return () => window.clearInterval(timer);
  }, [load]);

  async function run(f: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try {
      await f();
    } catch (e) {
      setError(toCommandError(e).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="flex min-h-0 flex-col gap-4 overflow-y-auto p-6 text-sm" aria-label="Web games">
      <section>
        <h2 className="text-2xl font-semibold tracking-tight">Web games</h2>
        <p className="text-zinc-400">
          Games that run in a web page. OpenTrainer opens the game in its own window, where it can find and change numbers like gold,
          health or ammo.
        </p>
      </section>

      <p className="rounded-xl bg-amber-950/40 px-4 py-3 text-amber-200 ring-1 ring-amber-800/60" role="note">
        <strong>Play fair:</strong> only use cheats when you play alone, or in a private game with friends you invited. Never in games
        with strangers or in public matches: other players can report you, and sites can ban accounts. You agreed to this when you signed
        OpenTrainer&apos;s agreement.
      </p>

      {error && (
        <p className="rounded-lg bg-red-950/40 px-3 py-2 text-red-300" role="alert">
          {error}
        </p>
      )}

      {current && (
        <WebCheatsPanel
          key={current.id}
          game={current}
          onClose={() =>
            void run(async () => {
              await api.webClose();
              setCurrent(null);
            })
          }
        />
      )}

      <section className="flex flex-col gap-2" aria-label="Your web games">
        <h3 className="font-medium">Your web games</h3>
        {games === null ? (
          <p className="text-zinc-500">Loading…</p>
        ) : games.length === 0 ? (
          <p className="text-zinc-500">None yet. Add one below with its web address.</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {games.map((g) => (
              <li key={g.id} className="flex flex-wrap items-center gap-2 rounded-xl bg-zinc-900/70 px-3 py-2 ring-1 ring-zinc-800">
                <span className="min-w-0 flex-1">
                  <span className="font-medium">{g.name}</span> <span className="text-xs text-zinc-500">{host(g.url)}</span>
                  {current?.id === g.id && <span className="ml-2 text-xs text-emerald-300">Open</span>}
                </span>
                <button
                  className="btn-accent rounded-lg px-4 py-1.5 text-xs font-semibold disabled:opacity-40"
                  aria-label={`Play ${g.name}`}
                  disabled={busy}
                  onClick={() => void run(async () => setCurrent(await api.webOpen(g.id)))}
                >
                  {current?.id === g.id ? "Open again" : "Play"}
                </button>
                {confirmRemove === g.id ? (
                  <>
                    <span className="text-xs text-zinc-400">Remove it and its saved cheats?</span>
                    <button
                      className={button}
                      onClick={() =>
                        void run(async () => {
                          if (current?.id === g.id) await api.webClose();
                          await api.webGameRemove(g.id);
                          setConfirmRemove(null);
                          load();
                        })
                      }
                    >
                      Remove
                    </button>
                    <button className={button} onClick={() => setConfirmRemove(null)}>
                      Keep
                    </button>
                  </>
                ) : (
                  <button className={button} aria-label={`Remove ${g.name}`} onClick={() => setConfirmRemove(g.id)}>
                    Remove
                  </button>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="flex flex-col gap-2 border-t border-zinc-900 pt-3" aria-label="Add a web game">
        <h3 className="font-medium">Add a web game</h3>
        <div className="flex flex-wrap items-center gap-2">
          <input aria-label="Game name" className={`${field} w-48`} placeholder="Name" maxLength={80} value={name} onChange={(e) => setName(e.target.value)} />
          <input aria-label="Web address" className={`${field} min-w-0 flex-1`} placeholder="https://…" value={url} onChange={(e) => setUrl(e.target.value)} />
          <button
            className={button}
            disabled={busy || !name.trim() || !url.trim()}
            onClick={() =>
              void run(async () => {
                await api.webGameAdd(name, url);
                setName("");
                setUrl("");
                load();
              })
            }
          >
            Add
          </button>
        </div>
        <p className="text-xs text-zinc-500">
          Copy the address from your browser while the game is open. Games built with WebAssembly (also console emulators) and most
          HTML5 games work. Games inside another site&apos;s frame, or running in a background worker, may not show their numbers yet.
        </p>
      </section>
    </main>
  );
}
