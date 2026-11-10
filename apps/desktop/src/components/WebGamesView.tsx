import { useCallback, useEffect, useState } from "react";
import { api, toCommandError, type WebGame, type WebPack } from "../lib/ipc";
import { WebCheatsPanel } from "./WebCheatsPanel";
import { WebShareDialog } from "./WebShareDialog";

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
  const [sharing, setSharing] = useState<WebGame | null>(null);
  /** Cheats from a file or the community list, shown before they're added. */
  const [offered, setOffered] = useState<{ pack: WebPack; from: "file" | "community" } | null>(null);
  const [community, setCommunity] = useState<WebPack[] | null>(null);
  const [communityError, setCommunityError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
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
    api.communityList(false).then(
      (l) => setCommunity(Array.isArray(l?.web) ? l.web : []),
      (e: unknown) => setCommunityError(toCommandError(e).message),
    );
    return () => window.clearInterval(timer);
  }, [load]);

  async function run(f: () => Promise<void>) {
    setBusy(true);
    setError(null);
    setDone(null);
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

      {done && (
        <p className="rounded-lg bg-emerald-950/40 px-3 py-2 text-emerald-200" role="status">
          {done}
        </p>
      )}

      {offered && (
        <WebPackOffer
          offer={offered}
          busy={busy}
          onAdd={() =>
            void run(async () => {
              const r = await api.webInstall(offered.pack);
              setOffered(null);
              load();
              setDone(
                r.added === 0
                  ? `You already had these cheats for ${r.game.name}.`
                  : `Added ${r.added} cheat${r.added === 1 ? "" : "s"} to ${r.game.name}${r.skipped ? ` (${r.skipped} you already had)` : ""}. Click Play to use them.`,
              );
            })
          }
          onCancel={() => setOffered(null)}
        />
      )}

      {sharing && <WebShareDialog key={sharing.id} game={sharing} onClose={() => setSharing(null)} />}

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
                <button className={button} aria-label={`Share the cheats for ${g.name}`} onClick={() => setSharing(g)}>
                  Share
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
        <div className="flex flex-wrap items-center gap-2">
          <button
            className={button}
            disabled={busy}
            onClick={() =>
              void run(async () => {
                const path = await api.webPickFile();
                if (path) setOffered({ pack: await api.webReadFile(path), from: "file" });
              })
            }
          >
            Open a cheats file
          </button>
          <span className="text-xs text-zinc-500">A friend can give you their cheats as a file (Share, then “Save to a file”).</span>
        </div>
      </section>

      <section className="flex flex-col gap-2 border-t border-zinc-900 pt-3" aria-label="Web cheats from the community">
        <h3 className="font-medium">Web cheats from the community</h3>
        {community === null ? (
          <p className="text-xs text-zinc-500">{communityError ? `Couldn't load the community list: ${communityError}` : "Loading…"}</p>
        ) : community.length === 0 ? (
          <p className="text-xs text-zinc-500">None yet. Found some that work? Click Share next to the game.</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {community.map((p) => (
              <li key={p.id} className="flex flex-wrap items-center gap-2 rounded-xl bg-zinc-900/70 px-3 py-2 ring-1 ring-zinc-800">
                <span className="min-w-0 flex-1">
                  <span className="font-medium">{p.game}</span> <span className="text-xs text-zinc-500">{host(p.url)}</span>
                  <span className="block text-xs text-zinc-400">
                    {p.cheats.length} cheat{p.cheats.length === 1 ? "" : "s"} · found by {p.by}
                  </span>
                </span>
                <button className={button} aria-label={`Get the cheats for ${p.game}`} onClick={() => setOffered({ pack: p, from: "community" })}>
                  Get these cheats
                </button>
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

/** What's in cheats from a file or the community list, before they're added. */
function WebPackOffer({
  offer,
  busy,
  onAdd,
  onCancel,
}: {
  offer: { pack: WebPack; from: "file" | "community" };
  busy: boolean;
  onAdd: () => void;
  onCancel: () => void;
}) {
  const { pack } = offer;
  return (
    <section className="flex flex-col gap-2 rounded-2xl bg-zinc-900/90 p-4 ring-1 ring-emerald-800/70" role="dialog" aria-label={`Cheats for ${pack.game}`}>
      <p className="text-lg font-semibold tracking-tight">Cheats for {pack.game}</p>
      <p className="text-xs text-zinc-400">
        Found by {pack.by}
        {pack.added ? ` · ${pack.added}` : ""} · the game&apos;s page: <span className="text-zinc-200">{host(pack.url)}</span>
      </p>
      {pack.notes && <p className="text-sm text-zinc-300">{pack.notes}</p>}
      <ul className="flex flex-wrap gap-1 text-xs" aria-label="Cheats in it">
        {pack.cheats.map((c, i) => (
          <li key={i} className="rounded bg-zinc-950/70 px-2 py-0.5">
            {c.name}
            {c.key ? <span className="text-zinc-500"> ({c.key})</span> : null}
          </li>
        ))}
      </ul>
      <p className="text-xs text-zinc-500">
        {offer.from === "file" ? "Only add cheats from people you trust. " : "A maintainer checked these before they were listed. "}
        Adding them puts the game in your web games (if it isn&apos;t there yet) with these cheats. They may not work after the game
        changes.
      </p>
      <div className="flex gap-2">
        <button className="btn-accent rounded-lg px-4 py-1.5 text-sm font-semibold disabled:opacity-40" disabled={busy} onClick={onAdd}>
          Add to my web games
        </button>
        <button className={button} onClick={onCancel}>
          Cancel
        </button>
      </div>
    </section>
  );
}
