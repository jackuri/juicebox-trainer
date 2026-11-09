import { useEffect, useState } from "react";
import { hex } from "../lib/format";
import {
  api,
  type CommandError,
  isSessionLost,
  type ListEntry,
  toCommandError,
  VALUE_TYPES,
  type ValueType,
} from "../lib/ipc";

const POLL_MS = 500;

interface Props {
  entries: ListEntry[];
  onChange: (entries: ListEntry[]) => void;
  onSessionLost: (e: CommandError) => void;
  /** Opens an address in the memory viewer. */
  onViewMemory?: (address: string) => void;
}

/** Address list with live values and freeze toggles. Polls the backend. */
export function AddressList({ entries, onChange, onSessionLost, onViewMemory }: Props) {
  const [address, setAddress] = useState("");
  const [ty, setTy] = useState<ValueType>("i32");
  const [label, setLabel] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<Record<number, string>>({});

  useEffect(() => {
    const load = () => {
      api.addressList().then(onChange, (e: unknown) => {
        const err = toCommandError(e);
        if (isSessionLost(err)) onSessionLost(err);
      });
    };
    load();
    const timer = setInterval(load, POLL_MS);
    return () => clearInterval(timer);
  }, [onChange, onSessionLost]);

  async function act(fn: () => Promise<unknown>) {
    setError(null);
    try {
      await fn();
      onChange(await api.addressList());
    } catch (e) {
      const err = toCommandError(e);
      if (isSessionLost(err)) onSessionLost(err);
      else setError(err.message);
    }
  }

  function add() {
    void act(async () => {
      await api.addressAdd(address, ty, label);
      setAddress("");
      setLabel("");
    });
  }

  function write(entry: ListEntry) {
    const value = editing[entry.id];
    if (value === undefined || !value.trim()) return;
    void act(async () => {
      await api.addressWrite(entry.id, value);
      setEditing((m) => {
        const next = { ...m };
        delete next[entry.id];
        return next;
      });
    });
  }

  return (
    <section className="flex flex-col gap-2" aria-label="Address list">
      <h2 className="text-sm font-medium text-zinc-300">Address list</h2>
      <div className="max-h-72 overflow-y-auto rounded-xl bg-zinc-900/70 ring-1 ring-zinc-800">
        <table className="w-full text-xs">
          <thead className="sticky top-0 bg-zinc-900 text-left text-zinc-400">
            <tr>
              <th className="w-14 px-2 py-1 font-medium">Freeze</th>
              <th className="px-2 py-1 font-medium">Label</th>
              <th className="px-2 py-1 font-medium">Address</th>
              <th className="px-2 py-1 font-medium">Type</th>
              <th className="px-2 py-1 font-medium">Value</th>
              <th className="w-8 px-2 py-1" />
            </tr>
          </thead>
          <tbody>
            {entries.map((e) => (
              <tr key={e.id} className="border-t border-zinc-900">
                <td className="px-2 py-1 text-center">
                  <input
                    type="checkbox"
                    aria-label={`Freeze ${e.label || hex(e.address)}`}
                    checked={e.frozen}
                    onChange={(ev) => void act(() => api.addressSetFrozen(e.id, ev.target.checked))}
                  />
                </td>
                <td className="px-2 py-1">
                  <input
                    className="w-full bg-transparent outline-none focus:ring-1 focus:ring-emerald-700"
                    aria-label={`Label for ${hex(e.address)}`}
                    defaultValue={e.label}
                    placeholder="—"
                    onBlur={(ev) => {
                      if (ev.target.value !== e.label) void act(() => api.addressRename(e.id, ev.target.value));
                    }}
                  />
                </td>
                <td className="px-2 py-1 font-mono text-zinc-400" title={e.addressExpr}>
                  {hex(e.address)}
                </td>
                <td className="px-2 py-1 text-zinc-500">{e.ty}</td>
                <td className="px-2 py-1 font-mono">
                  <input
                    className={`w-28 bg-transparent outline-none focus:ring-1 focus:ring-emerald-700 ${
                      e.frozen ? "text-sky-300" : ""
                    }`}
                    aria-label={`Value of ${e.label || hex(e.address)}`}
                    value={editing[e.id] ?? e.value ?? "??"}
                    title={e.error ?? undefined}
                    onFocus={() => setEditing((m) => ({ ...m, [e.id]: e.value ?? "" }))}
                    onChange={(ev) => setEditing((m) => ({ ...m, [e.id]: ev.target.value }))}
                    onBlur={() =>
                      setEditing((m) => {
                        const next = { ...m };
                        delete next[e.id];
                        return next;
                      })
                    }
                    onKeyDown={(ev) => ev.key === "Enter" && write(e)}
                  />
                  {e.error && <span className="ml-1 text-red-400" title={e.error}>!</span>}
                </td>
                <td className="whitespace-nowrap px-2 py-1 text-right">
                  {onViewMemory && (
                    <button
                      className="mr-2 text-zinc-500 hover:text-zinc-200"
                      aria-label={`View ${e.label || hex(e.address)} in memory`}
                      title="View in the memory viewer"
                      onClick={() => onViewMemory(hex(e.address))}
                    >
                      ⌗
                    </button>
                  )}
                  <button
                    className="text-zinc-500 hover:text-red-400"
                    aria-label={`Remove ${e.label || hex(e.address)}`}
                    onClick={() => void act(() => api.addressRemove(e.id))}
                  >
                    ✕
                  </button>
                </td>
              </tr>
            ))}
            {entries.length === 0 && (
              <tr>
                <td colSpan={6} className="px-2 py-4 text-center text-zinc-500">
                  Add addresses from scan results or below.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      <div className="flex gap-2">
        <input
          className="min-w-0 flex-1 rounded-lg bg-zinc-900 px-3 py-1.5 font-mono text-xs outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
          placeholder="0x… or module.exe+0x…"
          aria-label="New address"
          value={address}
          onChange={(e) => setAddress(e.target.value)}
        />
        <select
          className="rounded-lg bg-zinc-900 px-2 py-1.5 text-xs ring-1 ring-zinc-800"
          aria-label="New address type"
          value={ty}
          onChange={(e) => setTy(e.target.value as ValueType)}
        >
          {VALUE_TYPES.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </select>
        <input
          className="w-32 rounded-lg bg-zinc-900 px-3 py-1.5 text-xs outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
          placeholder="Label"
          aria-label="New address label"
          value={label}
          onChange={(e) => setLabel(e.target.value)}
        />
        <button
          className="rounded-lg bg-zinc-800 px-3 py-1.5 text-xs hover:bg-zinc-700 disabled:opacity-40"
          disabled={!address.trim()}
          onClick={add}
        >
          Add address
        </button>
      </div>
      {error && (
        <p className="text-xs text-red-400" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
