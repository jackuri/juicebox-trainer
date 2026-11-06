import { useEffect, useState } from "react";
import { hex } from "../lib/format";
import {
  api,
  type CommandError,
  isSessionLost,
  type PointerResults,
  type ScanProgress,
  toCommandError,
  VALUE_TYPES,
  type ValueType,
} from "../lib/ipc";

interface Props {
  onAddToList: (expression: string, ty: ValueType) => void;
  onSessionLost: (e: CommandError) => void;
}

const DEPTHS = [1, 2, 3, 4, 5, 6, 7];

/** Parses `1000`, `0x1000` (hex, like everywhere else in the app). */
function parseHex(s: string): number | null {
  const t = s.trim().replace(/^0x/i, "");
  if (!/^[0-9a-f]+$/i.test(t)) return null;
  return parseInt(t, 16);
}

/**
 * Pointer scanner: finds paths from the game's exe or DLLs to a value, so a
 * cheat can find it again after the game restarts.
 */
export function PointerPanel({ onAddToList, onSessionLost }: Props) {
  const [target, setTarget] = useState("");
  const [depth, setDepth] = useState(4);
  const [maxOffset, setMaxOffset] = useState("1000");
  const [ty, setTy] = useState<ValueType>("i32");
  const [newTarget, setNewTarget] = useState("");
  const [results, setResults] = useState<PointerResults | null>(null);
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.pointerResults().then(setResults, () => undefined);
  }, []);

  useEffect(() => {
    if (!running) return;
    const timer = setInterval(() => {
      api.scanProgress().then(setProgress, () => undefined);
    }, 150);
    return () => clearInterval(timer);
  }, [running]);

  function fail(e: unknown) {
    const err = toCommandError(e);
    if (isSessionLost(err)) onSessionLost(err);
    else if (err.kind !== "cancelled") setError(err.message);
  }

  const offset = parseHex(maxOffset);

  async function scan() {
    if (offset === null) return;
    setError(null);
    setRunning(true);
    setProgress(null);
    try {
      setResults(
        await api.pointerScan(target.trim(), { maxDepth: depth, maxOffset: offset, maxResults: 5000 }),
      );
    } catch (e) {
      fail(e);
    } finally {
      setRunning(false);
    }
  }

  async function check() {
    setError(null);
    try {
      setResults(await api.pointerFilter(newTarget.trim()));
      setNewTarget("");
    } catch (e) {
      fail(e);
    }
  }

  async function clear() {
    await api.pointerClear().catch(() => undefined);
    setResults(null);
  }

  const pct = progress && progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0;
  const input =
    "rounded-md bg-zinc-900 px-3 py-1.5 font-mono text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600";

  return (
    <section className="flex flex-col gap-3" aria-label="Pointer scanner">
      <p className="text-xs text-zinc-400">
        Find a value with the Scanner first, then look for pointer paths to its address. Paths start in the
        game's exe or a DLL, so they still work after the game restarts.
      </p>
      <div className="flex flex-wrap items-end gap-2">
        <label className="flex flex-col gap-1 text-xs text-zinc-400">
          Address of the value
          <input
            className={`w-52 ${input}`}
            placeholder="0x1A2B3C40"
            aria-label="Target address"
            value={target}
            onChange={(e) => setTarget(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && target.trim() && !running && void scan()}
          />
        </label>
        <label className="flex flex-col gap-1 text-xs text-zinc-400">
          Levels
          <select
            className="rounded-md bg-zinc-900 px-2 py-1.5 text-sm text-zinc-100 ring-1 ring-zinc-800"
            aria-label="Pointer levels"
            value={depth}
            onChange={(e) => setDepth(Number(e.target.value))}
          >
            {DEPTHS.map((d) => (
              <option key={d} value={d}>
                {d}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 text-xs text-zinc-400">
          Largest offset (hex)
          <input
            className={`w-24 ${input}`}
            aria-label="Largest offset"
            value={maxOffset}
            onChange={(e) => setMaxOffset(e.target.value)}
          />
        </label>
        <button
          className="rounded-md bg-emerald-700 px-4 py-1.5 text-sm font-medium hover:bg-emerald-600 disabled:opacity-40"
          disabled={running || !target.trim() || offset === null || offset === 0}
          onClick={() => void scan()}
        >
          Find pointers
        </button>
      </div>

      {running && (
        <div className="flex items-center gap-3" role="progressbar" aria-valuenow={pct}>
          <div className="h-1.5 flex-1 overflow-hidden rounded bg-zinc-800">
            <div className="h-full bg-emerald-600 transition-all" style={{ width: `${pct}%` }} />
          </div>
          <span className="w-10 text-right text-xs text-zinc-400">{pct}%</span>
          <button
            className="rounded-md bg-zinc-800 px-2 py-1 text-xs hover:bg-zinc-700"
            onClick={() => void api.scanCancel()}
          >
            Cancel
          </button>
        </div>
      )}

      {error && (
        <p className="text-xs text-red-400" role="alert">
          {error}
        </p>
      )}

      {results && (
        <div className="flex flex-col gap-2">
          <div className="flex flex-wrap items-center gap-3 text-xs text-zinc-400">
            <span data-testid="pointer-count">
              {results.count.toLocaleString()} path{results.count === 1 ? "" : "s"}
            </span>
            <span>
              {results.rounds === 1 ? "first scan" : `checked ${results.rounds - 1} time(s) after a restart`}
            </span>
            <span>
              in {results.exe}, to {hex(results.target)}
            </span>
            {results.count > results.paths.length && <span>showing first {results.paths.length}</span>}
            <button className="ml-auto rounded bg-zinc-800 px-2 py-0.5 hover:bg-zinc-700" onClick={() => void clear()}>
              Clear
            </button>
          </div>

          <div className="flex flex-wrap items-end gap-2 rounded-md bg-zinc-900/60 p-2 text-xs text-zinc-400">
            <label className="flex flex-col gap-1">
              Restarted the game? Find the value again and enter its new address to keep only paths that still
              work.
              <input
                className={`w-52 ${input}`}
                placeholder="new address"
                aria-label="New address"
                value={newTarget}
                onChange={(e) => setNewTarget(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && newTarget.trim() && void check()}
              />
            </label>
            <button
              className="rounded-md bg-zinc-800 px-3 py-1.5 text-sm text-zinc-100 hover:bg-zinc-700 disabled:opacity-40"
              disabled={!newTarget.trim()}
              onClick={() => void check()}
            >
              Check paths
            </button>
            <label className="ml-auto flex flex-col gap-1">
              Add as
              <select
                className="rounded-md bg-zinc-900 px-2 py-1.5 text-sm text-zinc-100 ring-1 ring-zinc-800"
                aria-label="Value type to add"
                value={ty}
                onChange={(e) => setTy(e.target.value as ValueType)}
              >
                {VALUE_TYPES.map((t) => (
                  <option key={t} value={t}>
                    {t}
                  </option>
                ))}
              </select>
            </label>
          </div>

          {results.paths.length > 0 ? (
            <div className="max-h-72 overflow-y-auto rounded-md ring-1 ring-zinc-800">
              <table className="w-full text-xs" aria-label="Pointer paths">
                <thead className="sticky top-0 bg-zinc-900 text-left text-zinc-400">
                  <tr>
                    <th className="px-2 py-1 font-medium">Path</th>
                    <th className="px-2 py-1 font-medium">Leads to</th>
                    <th className="px-2 py-1" />
                  </tr>
                </thead>
                <tbody className="font-mono">
                  {results.paths.map((p) => (
                    <tr key={p.expression} className="border-t border-zinc-900">
                      <td className="px-2 py-1 text-zinc-300">{p.expression}</td>
                      <td className="px-2 py-1 text-zinc-500">{p.address === null ? "—" : hex(p.address)}</td>
                      <td className="px-2 py-1 text-right">
                        <button
                          className="rounded bg-zinc-800 px-2 py-0.5 font-sans hover:bg-zinc-700"
                          aria-label={`Add ${p.expression} to address list`}
                          onClick={() => onAddToList(p.expression, ty)}
                        >
                          Add
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <p className="text-xs text-zinc-400">
              No paths left. Try more levels or a larger offset, or check the address.
            </p>
          )}
        </div>
      )}
    </section>
  );
}
