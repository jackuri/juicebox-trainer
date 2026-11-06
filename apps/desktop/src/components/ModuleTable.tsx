import { useMemo, useState } from "react";
import { hex } from "../lib/format";
import type { Module } from "../lib/ipc";

interface Props {
  modules: Module[];
  onRefresh: () => void;
}

export function ModuleTable({ modules, onRefresh }: Props) {
  const [query, setQuery] = useState("");
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return q ? modules.filter((m) => m.name.toLowerCase().includes(q)) : modules;
  }, [modules, query]);

  return (
    <section className="flex min-h-0 flex-col" aria-label="Modules">
      <div className="flex items-center gap-2 pb-2">
        <h2 className="text-sm font-medium text-zinc-300">Modules ({modules.length})</h2>
        <input
          className="ml-auto w-48 rounded-md bg-zinc-900 px-2 py-1 text-xs outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
          placeholder="Filter modules"
          aria-label="Filter modules"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button
          className="rounded-md bg-zinc-800 px-2 py-1 text-xs hover:bg-zinc-700"
          onClick={onRefresh}
        >
          Refresh
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto rounded-md ring-1 ring-zinc-800">
        <table className="w-full text-xs">
          <thead className="sticky top-0 bg-zinc-900 text-left text-zinc-400">
            <tr>
              <th className="px-2 py-1 font-medium">Name</th>
              <th className="px-2 py-1 font-medium">Base</th>
              <th className="px-2 py-1 font-medium">Size</th>
            </tr>
          </thead>
          <tbody className="font-mono">
            {shown.map((m) => (
              <tr key={`${m.base}`} className="border-t border-zinc-900" title={m.path}>
                <td className="px-2 py-1 font-sans">{m.name}</td>
                <td className="px-2 py-1 text-zinc-400">{hex(m.base)}</td>
                <td className="px-2 py-1 text-zinc-500">{hex(m.size)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
