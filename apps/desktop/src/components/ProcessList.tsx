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
    <section className="flex min-h-0 flex-col border-r border-zinc-800" aria-label="Processes">
      <div className="flex items-center gap-2 border-b border-zinc-800 p-3">
        <input
          className="min-w-0 flex-1 rounded-md bg-zinc-900 px-3 py-1.5 text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
          placeholder="Search name or PID"
          aria-label="Search processes"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button
          className="rounded-md bg-zinc-800 px-3 py-1.5 text-sm hover:bg-zinc-700 disabled:opacity-50"
          onClick={onRefresh}
          disabled={loading}
        >
          {loading ? "…" : "Refresh"}
        </button>
      </div>
      <label className="flex items-center gap-2 border-b border-zinc-800 px-3 py-2 text-xs text-zinc-400">
        <input
          type="checkbox"
          checked={hideSystem}
          onChange={(e) => setHideSystem(e.target.checked)}
        />
        Hide Windows processes
      </label>
      <ul className="min-h-0 flex-1 overflow-y-auto" role="listbox" aria-label="Process list">
        {shown.map((p) => (
          <li key={p.pid}>
            <button
              role="option"
              aria-selected={p.pid === selectedPid}
              onClick={() => onSelect(p)}
              className={`flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-zinc-900 ${
                p.pid === selectedPid ? "bg-zinc-900 text-emerald-400" : ""
              }`}
            >
              <span className="min-w-0 flex-1 truncate">{p.name}</span>
              <span className="rounded bg-zinc-800 px-1.5 text-[10px] uppercase text-zinc-400">
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
