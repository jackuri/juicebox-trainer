import type { CheatChanges } from "../lib/ipc";

/** What changed since cheats were shared, in plain words (v1.38.0). */
export function ChangeList({ changes }: { changes: CheatChanges }) {
  const rows: [string, string[]][] = [
    ["New", changes.added],
    ["Changed", changes.changed],
    ["Renamed", changes.renamed.map(([a, b]) => `${a} → ${b}`)],
    ["Removed", changes.removed],
  ];
  return (
    <ul className="flex flex-col gap-1 text-xs" aria-label="What changed">
      {rows
        .filter(([, items]) => items.length > 0)
        .map(([label, items]) => (
          <li key={label}>
            <span className={label === "Removed" ? "text-amber-300" : "text-emerald-300"}>{label}:</span>{" "}
            <span className="text-zinc-300">{items.join(", ")}</span>
          </li>
        ))}
      {changes.details && <li className="text-zinc-400">Who made them or the notes changed.</li>}
    </ul>
  );
}
