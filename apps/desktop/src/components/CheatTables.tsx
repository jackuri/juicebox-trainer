import { useCallback, useEffect, useState } from "react";
import { api, type LibraryGame, type RecentTable, type TableLink } from "../lib/ipc";
import { friendlyError } from "../lib/explain";
import { dismiss, dismissed, exeFor, lastSearch, linksFor, matchGame, rememberSearch, siteOf } from "../lib/tables";

const small = "rounded-lg bg-zinc-800 px-2.5 py-1.5 text-xs ring-1 ring-zinc-700 hover:bg-zinc-700";

/** Look for a free Cheat Engine table for one game, on its maker's site. */
export function FindTable({ game, onClose, onError }: { game: LibraryGame; onClose: () => void; onError: (m: string) => void }) {
  const [opened, setOpened] = useState<"page" | "search" | null>(null);
  /** Tables other players linked on the community list (null while loading). */
  const [links, setLinks] = useState<TableLink[] | null>(null);

  useEffect(() => {
    let live = true;
    api.communityList(false).then(
      (list) => live && setLinks(linksFor(game, list?.tables ?? [])),
      () => live && setLinks([]),
    );
    return () => {
      live = false;
    };
  }, [game]);

  async function openLink(url: string) {
    try {
      await api.communityOpenTable(url);
      rememberSearch(game.id);
      setOpened("page");
    } catch (e) {
      onError(friendlyError(e));
    }
  }

  async function search(site: "fearless" | "web") {
    try {
      await api.tablesSearch(site, game.name);
      rememberSearch(game.id);
      setOpened("search");
    } catch (e) {
      onError(friendlyError(e));
    }
  }
  return (
    <div className="flex w-full flex-col gap-2 rounded-lg bg-zinc-950/60 p-2.5 text-xs ring-1 ring-zinc-800" role="group" aria-label={`Find a cheat table for ${game.name}`}>
      <p className="text-zinc-300">
        Players share free cheat tables (<code>.CT</code> files) for many games. Open one on its maker&apos;s page and
        download the <code>.CT</code> file: OpenTrainer spots it in your Downloads folder and turns it into cheats.
      </p>
      {links && links.length > 0 && links[0] && (
        <button
          className="btn-accent self-start rounded-lg px-3 py-1.5 text-xs font-semibold"
          onClick={() => void openLink(links[0]!.url)}
        >
          Open the {game.name} table page ({siteOf(links[0].url)})
        </button>
      )}
      {links && links.length > 0 && (
        <div className="flex flex-col gap-1" aria-label="Tables other players found">
          <p className="font-medium text-zinc-200">Tables other players found</p>
          <ul className="flex flex-col gap-1">
            {links.map((l) => (
              <li key={l.id} className="flex flex-col items-start gap-1 rounded bg-zinc-900/80 px-2 py-1.5">
                <span className="min-w-0 break-words">
                  <span className="text-zinc-100">{l.title}</span>
                  <span className="text-zinc-400">
                    {" "}
                    by {l.by} · {siteOf(l.url)}
                    {l.cheats != null && ` · ${l.cheats} cheat${l.cheats === 1 ? "" : "s"} OpenTrainer can use`}
                    {l.added && ` · added ${l.added}`}
                  </span>
                </span>
                <button className={small} aria-label={`Open the page of ${l.title}`} onClick={() => void openLink(l.url)}>
                  Open its page
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
      <div className="flex flex-wrap gap-1.5">
        <button className={small} onClick={() => void search("fearless")}>
          {links && links.length > 0 ? "Search FearLess Revolution for others" : "Search FearLess Revolution"}
        </button>
        <button className={small} onClick={() => void search("web")}>
          Search the web
        </button>
        <button className="ml-auto text-zinc-500 hover:text-zinc-300" onClick={onClose}>
          Close
        </button>
      </div>
      {opened && (
        <p className="text-emerald-300" role="status">
          Opened in your browser.{" "}
          {opened === "search"
            ? "Click your game in the list, then the .CT file under the first post."
            : "Click the .CT file under the first post."}{" "}
          FearLess Revolution asks you to sign in before downloading. Check the table is for your game version, and
          please thank its maker on their page.
        </p>
      )}
    </div>
  );
}

const POLL_MS = 5000;

/** Tables the player downloaded lately, each with "Turn into cheats". */
export function DownloadedTables({
  games,
  onOpenTable,
}: {
  games: LibraryGame[];
  onOpenTable: (path: string, exe?: string) => void;
}) {
  const [tables, setTables] = useState<RecentTable[]>([]);
  const [hidden, setHidden] = useState(dismissed);

  const load = useCallback(() => {
    // Only while OpenTrainer is on screen.
    if (document.hidden) return;
    api.tablesRecent().then((list) => setTables(Array.isArray(list) ? list : []), () => undefined);
  }, []);

  useEffect(() => {
    load();
    const timer = window.setInterval(load, POLL_MS);
    window.addEventListener("focus", load);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", load);
    };
  }, [load]);

  const search = lastSearch();
  const shown = tables
    .filter((t) => !t.error && !hidden.has(`${t.path}|${t.modified}`))
    .slice(0, 3)
    .map((t) => ({ t, game: matchGame(t, games, search) }));
  if (shown.length === 0) return null;

  function notNow(t: RecentTable) {
    dismiss(t);
    setHidden(dismissed());
  }

  return (
    <section className="flex flex-col gap-1.5 rounded-xl bg-emerald-950/40 p-3 ring-1 ring-emerald-800/70" aria-label="Downloaded cheat tables">
      <h3 className="text-sm font-medium text-emerald-200">Cheat tables in your Downloads</h3>
      {shown.map(({ t, game }) => (
        <div key={`${t.path}|${t.modified}`} className="flex flex-wrap items-center gap-2 text-xs">
          <span className="min-w-0 flex-1 break-words">
            <span className="font-medium">{t.file}</span>
            <span className="text-zinc-400">
              {" "}
              · {game ? `for ${game.name}` : `for "${t.game}"`} ·{" "}
              {t.cheats === 0 ? "only scripts (nothing OpenTrainer can use)" : `${t.cheats} cheat${t.cheats === 1 ? "" : "s"} OpenTrainer can use`}
            </span>
          </span>
          <button
            className="btn-accent rounded-lg px-3 py-1.5 text-xs font-semibold"
            onClick={() => {
              notNow(t);
              onOpenTable(t.path, exeFor(t, game));
            }}
          >
            Turn into cheats
          </button>
          <button className={small} onClick={() => notNow(t)}>
            Not now
          </button>
        </div>
      ))}
    </section>
  );
}
