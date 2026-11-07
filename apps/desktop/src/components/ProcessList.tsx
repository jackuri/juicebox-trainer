import { useMemo, useState } from "react";
import { filterProcesses } from "../lib/format";
import type { ProcessInfo } from "../lib/ipc";

interface Props {
  processes: ProcessInfo[];
  selectedPid: number | null;
  loading: boolean;
  onSelect: (p: ProcessInfo) => void;
  onRefresh: () => void;
}

export function ProcessList({ processes, selectedPid, loading, onSelect, onRefresh }: Props) {
  const [query, setQuery] = useState("");
  const [hideSystem, setHideSystem] = useState(true);
  const shown = useMemo(
    () => filterProcesses(processes, query, hideSystem),
    [processes, query, hideSystem],
  );

  return (
    <section className="flex min-h-0 flex-col border-r border-zinc-800/70 bg-zinc-950/40" aria-label="Processes">
      <p className="px-4 pt-4 text-[11px] font-semibold uppercase tracking-[0.14em] text-zinc-400">Running programs</p>
      <div className="flex items-center gap-2 px-3 pb-2 pt-2">
        <input
          className="min-w-0 flex-1 rounded-lg bg-zinc-900 px-3 py-1.5 text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
          placeholder="Search name or PID"
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
          checked={hideSystem}
          onChange={(e) => setHideSystem(e.target.checked)}
        />
        Hide Windows processes
      </label>
      <ul className="min-h-0 flex-1 overflow-y-auto px-2 py-2" role="listbox" aria-label="Process list">
        {shown.map((p) => (
          <li key={p.pid}>
            <button
              role="option"
              aria-selected={p.pid === selectedPid}
              onClick={() => onSelect(p)}
              className={`flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-sm transition-colors ${
                p.pid === selectedPid
                  ? "bg-emerald-950/60 text-white ring-1 ring-emerald-800/80"
                  : "text-zinc-300 hover:bg-zinc-900"
              }`}
            >
              <span className="min-w-0 flex-1 truncate">{p.name}</span>
              <span className="rounded-full bg-zinc-800 px-1.5 text-[10px] font-semibold uppercase text-zinc-400">
                {p.arch}
              </span>
              <span className="w-14 text-right font-mono text-xs text-zinc-500">{p.pid}</span>
            </button>
          </li>
        ))}
        {shown.length === 0 && (
          <li className="px-3 py-6 text-center text-sm text-zinc-500">No matching processes</li>
        )}
      </ul>
    </section>
  );
}
