import { useEffect, useState } from "react";
import { hex } from "../lib/format";
import {
  api,
  type CommandError,
  type FirstScanKind,
  isSessionLost,
  type NextScanKind,
  type ScanHit,
  type ScanProgress,
  type ScanSummary,
  scanMode,
  toCommandError,
  VALUE_TYPES,
  type ValueType,
} from "../lib/ipc";

const FIRST_KINDS: { kind: FirstScanKind; label: string }[] = [
  { kind: "exact", label: "Exact value" },
  { kind: "between", label: "Value between" },
  { kind: "unknown", label: "Unknown initial value" },
];

const NEXT_KINDS: { kind: NextScanKind; label: string }[] = [
  { kind: "exact", label: "Exact value" },
  { kind: "between", label: "Value between" },
  { kind: "changed", label: "Changed" },
  { kind: "unchanged", label: "Unchanged" },
  { kind: "increased", label: "Increased" },
  { kind: "decreased", label: "Decreased" },
];

const RESULT_LIMIT = 200;

interface Props {
  onAddToList: (address: number, ty: ValueType) => void;
  /** Look for pointer paths to this address (Pointers tab). */
  onFindPointers?: (address: number, ty: ValueType) => void;
  onSessionLost: (e: CommandError) => void;
}

function needsValue(kind: string) {
  return kind === "exact" || kind === "between";
}

export function ScannerPanel({ onAddToList, onFindPointers, onSessionLost }: Props) {
  const [ty, setTy] = useState<ValueType>("i32");
  const [firstKind, setFirstKind] = useState<FirstScanKind>("exact");
  const [nextKind, setNextKind] = useState<NextScanKind>("exact");
  const [value, setValue] = useState("");
  const [max, setMax] = useState("");
  const [writableOnly, setWritableOnly] = useState(true);
  const [aligned, setAligned] = useState(true);
  const [tolerance, setTolerance] = useState("");
  const [summary, setSummary] = useState<ScanSummary | null>(null);
  const [hits, setHits] = useState<ScanHit[]>([]);
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const kind = summary ? nextKind : firstKind;
  const isFloat = ty === "f32" || ty === "f64";

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

  async function loadResults() {
    try {
      const r = await api.scanResults(0, RESULT_LIMIT);
      setHits(r.hits);
    } catch (e) {
      fail(e);
    }
  }

  async function runScan() {
    setError(null);
    setRunning(true);
    setProgress(null);
    try {
      const mode = scanMode(kind, value, max);
      const s = summary
        ? await api.scanNext(mode)
        : await api.scanFirst(ty, mode, {
            writableOnly,
            aligned,
            floatTolerance: isFloat && tolerance.trim() ? Number(tolerance) : null,
          });
      setSummary(s);
      await loadResults();
    } catch (e) {
      fail(e);
    } finally {
      setRunning(false);
    }
  }

  async function newScan() {
    setError(null);
    await api.scanReset().catch(fail);
    setSummary(null);
    setHits([]);
    setNextKind("exact");
  }

  const canScan = !running && (!needsValue(kind) || (value.trim() && (kind !== "between" || max.trim())));
  const pct = progress && progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0;

  return (
    <section className="flex flex-col gap-3" aria-label="Scanner">
      <div className="flex flex-wrap items-end gap-2">
        <label className="flex flex-col gap-1 text-xs text-zinc-400">
          Type
          <select
            className="rounded-lg bg-zinc-900 px-2 py-1.5 text-sm text-zinc-100 ring-1 ring-zinc-800 disabled:opacity-50"
            aria-label="Scan value type"
            value={ty}
            disabled={summary !== null || running}
            onChange={(e) => setTy(e.target.value as ValueType)}
          >
            {VALUE_TYPES.map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 text-xs text-zinc-400">
          Scan
          <select
            className="rounded-lg bg-zinc-900 px-2 py-1.5 text-sm text-zinc-100 ring-1 ring-zinc-800"
            aria-label="Scan kind"
            value={kind}
            disabled={running}
            onChange={(e) =>
              summary
                ? setNextKind(e.target.value as NextScanKind)
                : setFirstKind(e.target.value as FirstScanKind)
            }
          >
            {(summary ? NEXT_KINDS : FIRST_KINDS).map((k) => (
              <option key={k.kind} value={k.kind}>
                {k.label}
              </option>
            ))}
          </select>
        </label>
        {needsValue(kind) && (
          <input
            className="w-32 rounded-lg bg-zinc-900 px-3 py-1.5 font-mono text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
            placeholder={kind === "between" ? "Min" : "Value"}
            aria-label={kind === "between" ? "Minimum" : "Scan value"}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && canScan && void runScan()}
          />
        )}
        {kind === "between" && (
          <input
            className="w-32 rounded-lg bg-zinc-900 px-3 py-1.5 font-mono text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
            placeholder="Max"
            aria-label="Maximum"
            value={max}
            onChange={(e) => setMax(e.target.value)}
          />
        )}
        <button
          className="rounded-lg btn-accent px-4 py-1.5 text-sm font-semibold disabled:opacity-40"
          disabled={!canScan}
          onClick={() => void runScan()}
        >
          {summary ? "Next scan" : "First scan"}
        </button>
        {summary && (
          <button
            className="rounded-lg bg-zinc-800 px-3 py-1.5 text-sm hover:bg-zinc-700 disabled:opacity-40"
            disabled={running}
            onClick={() => void newScan()}
          >
            New scan
          </button>
        )}
      </div>

      {!summary && (
        <div className="flex flex-wrap gap-4 text-xs text-zinc-400">
          <label className="flex items-center gap-1.5">
            <input type="checkbox" checked={writableOnly} onChange={(e) => setWritableOnly(e.target.checked)} />
            Writable memory only
          </label>
          <label className="flex items-center gap-1.5">
            <input type="checkbox" checked={aligned} onChange={(e) => setAligned(e.target.checked)} />
            Aligned (fast)
          </label>
          {isFloat && (
            <label className="flex items-center gap-1.5">
              Tolerance
              <input
                className="w-20 rounded bg-zinc-900 px-2 py-0.5 font-mono ring-1 ring-zinc-800"
                placeholder={ty === "f32" ? "0.001" : "0.000001"}
                aria-label="Float tolerance"
                value={tolerance}
                onChange={(e) => setTolerance(e.target.value)}
              />
            </label>
          )}
        </div>
      )}

      {running && (
        <div className="flex items-center gap-3" role="progressbar" aria-valuenow={pct}>
          <div className="h-1.5 flex-1 overflow-hidden rounded bg-zinc-800">
            <div className="h-full bg-emerald-600 transition-all" style={{ width: `${pct}%` }} />
          </div>
          <span className="w-10 text-right text-xs text-zinc-400">{pct}%</span>
          <button
            className="rounded-lg bg-zinc-800 px-2 py-1 text-xs hover:bg-zinc-700"
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

      {summary && (
        <div className="flex flex-col gap-2">
          <div className="flex items-center gap-3 text-xs text-zinc-400">
            <span data-testid="scan-count">
              {summary.count.toLocaleString()} result{summary.count === 1 ? "" : "s"}
            </span>
            <span>scan {summary.rounds}</span>
            <span>{summary.ty}</span>
            {summary.count > RESULT_LIMIT && <span>showing first {RESULT_LIMIT}</span>}
            <button className="ml-auto rounded bg-zinc-800 px-2 py-0.5 hover:bg-zinc-700" onClick={() => void loadResults()}>
              Refresh values
            </button>
          </div>
          {hits.length > 0 && (
            <div className="max-h-64 overflow-y-auto rounded-xl bg-zinc-900/70 ring-1 ring-zinc-800">
              <table className="w-full text-xs" aria-label="Scan results">
                <thead className="sticky top-0 bg-zinc-900 text-left text-zinc-400">
                  <tr>
                    <th className="px-2 py-1 font-medium">Address</th>
                    <th className="px-2 py-1 font-medium">Value</th>
                    <th className="px-2 py-1 font-medium">Previous</th>
                    <th className="px-2 py-1" />
                  </tr>
                </thead>
                <tbody className="font-mono">
                  {hits.map((h) => (
                    <tr key={h.address} className="border-t border-zinc-900">
                      <td className="px-2 py-1 text-zinc-300">{hex(h.address)}</td>
                      <td className={`px-2 py-1 ${h.current !== h.previous ? "text-amber-300" : ""}`}>
                        {h.current ?? "??"}
                      </td>
                      <td className="px-2 py-1 text-zinc-500">{h.previous}</td>
                      <td className="whitespace-nowrap px-2 py-1 text-right">
                        {onFindPointers && (
                          <button
                            className="mr-1 rounded bg-zinc-800 px-2 py-0.5 font-sans hover:bg-zinc-700"
                            aria-label={`Find pointers to ${hex(h.address)}`}
                            title="Find a path to this value that still works after the game restarts"
                            onClick={() => onFindPointers(h.address, summary.ty)}
                          >
                            Find pointers
                          </button>
                        )}
                        <button
                          className="rounded bg-zinc-800 px-2 py-0.5 font-sans hover:bg-zinc-700"
                          aria-label={`Add ${hex(h.address)} to address list`}
                          onClick={() => onAddToList(h.address, summary.ty)}
                        >
                          Add
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
