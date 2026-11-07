import { useState } from "react";
import { hex } from "../lib/format";
import { api, type CommandError, toCommandError, VALUE_TYPES, type ValueType } from "../lib/ipc";

interface LogEntry {
  id: number;
  op: "read" | "write";
  address: string;
  ty: ValueType;
  value: string;
  bytes: string;
}

interface Props {
  /** Called when the backend dropped the session (guard blocked or game exited). */
  onSessionLost: (e: CommandError) => void;
}

let nextId = 1;

export function MemoryPanel({ onSessionLost }: Props) {
  const [address, setAddress] = useState("");
  const [ty, setTy] = useState<ValueType>("i32");
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [log, setLog] = useState<LogEntry[]>([]);

  async function run(op: "read" | "write") {
    setBusy(true);
    setError(null);
    try {
      const r =
        op === "read"
          ? await api.readValue(address, ty)
          : await api.writeValue(address, ty, value);
      setValue(r.value);
      setLog((l) =>
        [{ id: nextId++, op, address: hex(r.address), ty, value: r.value, bytes: r.bytes }, ...l].slice(
          0,
          50,
        ),
      );
    } catch (e) {
      const err = toCommandError(e);
      if (err.kind === "blocked" || err.kind === "processExited") {
        onSessionLost(err);
      } else {
        setError(err.message);
      }
    } finally {
      setBusy(false);
    }
  }

  const canRun = address.trim().length > 0 && !busy;

  return (
    <section className="flex flex-col gap-3" aria-label="Memory">
      <h2 className="text-sm font-medium text-zinc-300">Read / write a value</h2>
      <div className="grid grid-cols-[1fr_auto] gap-2">
        <input
          className="rounded-lg bg-zinc-900 px-3 py-1.5 font-mono text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
          placeholder="0x1234ABCD or testbed_native.exe+0x1F0"
          aria-label="Address"
          value={address}
          onChange={(e) => setAddress(e.target.value)}
        />
        <select
          className="rounded-lg bg-zinc-900 px-2 py-1.5 text-sm ring-1 ring-zinc-800"
          aria-label="Value type"
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
          className="rounded-lg bg-zinc-900 px-3 py-1.5 font-mono text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
          placeholder="Value"
          aria-label="Value"
          value={value}
          onChange={(e) => setValue(e.target.value)}
        />
        <div className="flex gap-2">
          <button
            className="rounded-lg bg-zinc-800 px-3 py-1.5 text-sm hover:bg-zinc-700 disabled:opacity-50"
            disabled={!canRun}
            onClick={() => void run("read")}
          >
            Read
          </button>
          <button
            className="rounded-lg btn-accent px-3 py-1.5 text-sm disabled:opacity-50"
            disabled={!canRun || value.trim().length === 0}
            onClick={() => void run("write")}
          >
            Write
          </button>
        </div>
      </div>
      {error && (
        <p className="text-xs text-red-400" role="alert">
          {error}
        </p>
      )}
      {log.length > 0 && (
        <ol className="max-h-40 overflow-y-auto rounded-xl bg-zinc-900/70 text-xs ring-1 ring-zinc-800" aria-label="Activity">
          {log.map((e) => (
            <li key={e.id} className="flex gap-3 border-t border-zinc-900 px-2 py-1 font-mono first:border-0">
              <span className={e.op === "write" ? "text-emerald-400" : "text-sky-400"}>{e.op}</span>
              <span className="text-zinc-400">{e.address}</span>
              <span className="text-zinc-500">{e.ty}</span>
              <span>{e.value}</span>
              <span className="ml-auto text-zinc-600">{e.bytes}</span>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}
