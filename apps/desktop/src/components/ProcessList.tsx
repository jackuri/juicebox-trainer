import { useMemo, useState } from "react";
import { splitGames } from "../lib/format";
import type { ProcessInfo, RunningGame } from "../lib/ipc";

interface Props {
  processes: ProcessInfo[];
  /** Which of `processes` are the player's games. */
  games: RunningGame[];
  selectedPid: number | null;
  loading: boolean;
  onSelect: (p: ProcessInfo) => void;
  onRefresh: () => void;
  /** Opens the Games tab (to start a game). */
  onShowGames?: () => void;
}

/**
 * The player's running games, by name. Other programs are listed only after
 * "Show all running programs" (for a game JuiceBox Trainer didn't recognise).
 */
export function ProcessList({ processes, games, selectedPid, loading, onSelect, onRefresh, onShowGames }: Props) {
  const [query, setQuery] = useState("");
  const [showAll, setShowAll] = useState(false);
  const split = useMemo(() => splitGames(processes, games, query), [processes, games, query]);
  const selectedIsOther = split.others.some((p) => p.pid === selectedPid);
  const all = showAll || selectedIsOther;

  const row = (p: ProcessInfo, title: string, sub: string | null) => (
    <li key={p.pid}>
      <button
        role="option"
        aria-selected={p.pid === selectedPid}
        onClick={() => onSelect(p)}
        className={`flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-sm transition-colors ${
          p.pid === selectedPid
            ? "bg-emerald-950/60 text-bright ring-1 ring-emerald-800/80"
            : "text-zinc-300 hover:bg-zinc-900"
        }`}
      >
        <span className="min-w-0 flex-1">
          <span className="block truncate">{title}</span>
          {sub && <span className="block truncate text-xs text-zinc-500">{sub}</span>}
        </span>
        <span className="rounded-full bg-zinc-800 px-1.5 text-[10px] font-semibold uppercase text-zinc-400">
          {p.arch}
        </span>
        <span className="w-12 text-right font-mono text-xs text-zinc-500">{p.pid}</span>
      </button>
    </li>
  );

  const heading = "px-2.5 pb-1 pt-2 text-[11px] font-semibold uppercase tracking-[0.14em] text-zinc-400";

  return (
    <section className="flex min-h-0 flex-col border-r border-zinc-800/70 bg-zinc-950/40" aria-label="Processes">
      <div className="flex items-center gap-2 px-3 pb-2 pt-4">
        <input
          className="min-w-0 flex-1 rounded-lg bg-zinc-900 px-3 py-1.5 text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
          placeholder="Search"
          aria-label="Search processes"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button
          className="rounded-lg bg-zinc-800 px-3 py-1.5 text-sm hover:bg-zinc-700 disabled:opacity-50"
          onClick={onRefresh}
          disabled={loading}
        >
          {loading ? "…" : "Refresh"}
        </button>
      </div>
      <label className="flex items-center gap-2 border-b border-zinc-800/70 px-4 pb-3 text-xs text-zinc-400">
        <input
          type="checkbox"
          className="accent-emerald-500"
          checked={all}
          disabled={selectedIsOther}
          onChange={(e) => setShowAll(e.target.checked)}
        />
        Show all running programs
      </label>
      <ul className="min-h-0 flex-1 overflow-y-auto px-2 py-2" role="listbox" aria-label="Process list">
        <li className={heading} role="presentation">
          Your running games
        </li>
        {split.games.map(({ game, process }) => row(process, game.name, process.name))}
        {split.games.length === 0 && (
          <li className="px-2.5 py-3 text-sm text-zinc-500" role="presentation">
            {query ? (
              "No running game matches."
            ) : (
              <>
                None of your games is running. Start one
                {onShowGames ? (
                  <>
                    {" "}
                    from{" "}
                    <button className="text-emerald-300 underline hover:text-emerald-200" onClick={onShowGames}>
                      Games
                    </button>
                  </>
                ) : null}{" "}
                and click Refresh. Not found? Tick “Show all running programs”.
              </>
            )}
          </li>
        )}
        {all && (
          <>
            <li className={`${heading} mt-2 border-t border-zinc-800/70 pt-3`} role="presentation">
              Other programs
            </li>
            {split.others.map((p) => row(p, p.name, null))}
            {split.others.length === 0 && (
              <li className="px-2.5 py-3 text-sm text-zinc-500" role="presentation">
                No matching programs
              </li>
            )}
          </>
        )}
      </ul>
    </section>
  );
}
