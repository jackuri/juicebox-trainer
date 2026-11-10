import { useEffect, useRef, useState } from "react";
import { hex, inspect } from "../lib/format";
import {
  api,
  type Arch,
  type CommandError,
  isSessionLost,
  type MemoryView,
  toCommandError,
  VALUE_TYPES,
  type ValueType,
} from "../lib/ipc";

/** Bytes per row and rows shown. */
const COLS = 16;
const ROWS = 16;
const SHOWN = COLS * ROWS;
const REFRESH_MS = 1000;

const button = "rounded-lg bg-zinc-800 px-2.5 py-1 text-xs hover:bg-zinc-700 disabled:opacity-40";

function byteHex(b: number): string {
  return b.toString(16).toUpperCase().padStart(2, "0");
}

function ascii(b: number): string {
  return b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : ".";
}

function regionText(view: MemoryView): string {
  const r = view.region;
  if (!r) return "Nothing is in memory at this address.";
  const can = [r.readable && "readable", r.writable && "writable", r.executable && "code"].filter(Boolean);
  const where = view.module ? ` · ${view.module}` : r.image ? " · in a module" : "";
  return `${can.length ? can.join(", ") : "can't be read"}${where} · region ${hex(r.base)}, ${(r.size / 1024).toFixed(0)} KB`;
}

interface Props {
  /** The tab is visible: refresh live. */
  active: boolean;
  arch: Arch;
  /** An address to open (from the address list); `n` changes for each request. */
  request: { address: string; n: number } | null;
  onAddToList: (address: number, ty: ValueType) => void;
  onSessionLost: (e: CommandError) => void;
}

/**
 * A live view of the game's memory, 16 bytes a row, with a value inspector.
 * Changing bytes goes through the backend: data is written directly, code is
 * journaled and put back on detach, read-only memory is refused.
 */
export function MemoryViewer({ active, arch, request, onAddToList, onSessionLost }: Props) {
  const [input, setInput] = useState(request?.address ?? "");
  const [view, setView] = useState<MemoryView | null>(null);
  const [changed, setChanged] = useState<Set<number>>(new Set());
  const [selected, setSelected] = useState<number | null>(null);
  const [editHex, setEditHex] = useState("");
  const [ty, setTy] = useState<ValueType>("i32");
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const last = useRef<MemoryView | null>(null);

  function fail(e: unknown) {
    const err = toCommandError(e);
    if (isSessionLost(err)) onSessionLost(err);
    else setError(err.message);
  }

  function show(v: MemoryView, keepSelection: boolean) {
    const prev = last.current;
    const diff = new Set<number>();
    if (prev && prev.address === v.address) {
      v.bytes.forEach((b, i) => {
        if (v.readable[i] && prev.readable[i] && prev.bytes[i] !== b) diff.add(i);
      });
    }
    last.current = v;
    setView(v);
    setChanged(diff);
    if (!keepSelection) setSelected(null);
  }

  async function go(address: string) {
    setError(null);
    setMessage(null);
    try {
      const v = await api.memoryView(address, SHOWN);
      last.current = null;
      show(v, false);
      setInput(hex(v.address));
    } catch (e) {
      fail(e);
    }
  }

  // A request from the address list opens that address.
  useEffect(() => {
    if (!request) return;
    let live = true;
    api.memoryView(request.address, SHOWN).then(
      (v) => {
        if (!live) return;
        last.current = null;
        show(v, false);
        setInput(hex(v.address));
      },
      (e: unknown) => live && fail(e),
    );
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [request]);

  const base = view?.address ?? null;
  useEffect(() => {
    if (!active || base === null) return;
    const timer = setInterval(() => {
      api.memoryView(hex(base), SHOWN).then(
        (v) => show(v, true),
        (e: unknown) => {
          const err = toCommandError(e);
          if (isSessionLost(err)) onSessionLost(err);
        },
      );
    }, REFRESH_MS);
    return () => clearInterval(timer);
  }, [active, base, onSessionLost]);

  const move = (delta: number) => {
    if (base === null) return;
    void go(hex(Math.max(0, base + delta)));
  };

  const select = (i: number) => {
    setSelected(i);
    setMessage(null);
    if (view?.readable[i]) {
      const n = Math.min(4, SHOWN - i);
      setEditHex(
        view.bytes
          .slice(i, i + n)
          .map(byteHex)
          .join(" "),
      );
    }
  };

  async function write() {
    if (base === null || selected === null) return;
    setError(null);
    setMessage(null);
    try {
      const r = await api.memoryEdit(hex(base + selected), editHex);
      setMessage(
        r === "code"
          ? "Changed the game's code. It is put back when you detach or close OpenTrainer."
          : r === "data"
            ? "Changed."
            : "Those bytes already had that value.",
      );
      show(await api.memoryView(hex(base), SHOWN), true);
    } catch (e) {
      fail(e);
    }
  }

  const selAddress = base !== null && selected !== null ? base + selected : null;
  const selBytes: number[] = [];
  if (view && selected !== null) {
    for (let i = selected; i < Math.min(selected + 8, SHOWN) && view.readable[i]; i++) {
      selBytes.push(view.bytes[i]!);
    }
  }
  const ptrSize = arch === "x86" ? 4 : 8;
  const pointer =
    selBytes.length >= ptrSize
      ? selBytes
          .slice(0, ptrSize)
          .reduceRight((acc, b) => acc * 256 + b, 0)
      : null;

  return (
    <section className="flex flex-col gap-3" aria-label="Memory viewer">
      <h2 className="text-sm font-medium text-zinc-300">Memory viewer</h2>
      <p className="text-xs text-zinc-500">
        The game&apos;s memory, 16 bytes a row, updated every second. Changed bytes light up. Click a
        byte to see the number it starts and to change it.
      </p>
      <div className="flex flex-wrap gap-2">
        <input
          className="min-w-0 flex-1 rounded-lg bg-zinc-900 px-3 py-1.5 font-mono text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
          placeholder="0x1234ABCD, game.exe+0x1F0 or [[game.exe+0x10]+0x8]"
          aria-label="Memory address"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && input.trim() && void go(input)}
        />
        <button className={button} disabled={!input.trim()} onClick={() => void go(input)}>
          Go
        </button>
        <button className={button} disabled={base === null} onClick={() => move(-SHOWN)} aria-label="Previous page">
          ▲ Page
        </button>
        <button className={button} disabled={base === null} onClick={() => move(SHOWN)} aria-label="Next page">
          ▼ Page
        </button>
      </div>
      {error && (
        <p className="text-xs text-red-400" role="alert">
          {error}
        </p>
      )}

      {view && (
        <>
          <p className="text-[11px] text-zinc-500">{regionText(view)}</p>
          <div className="overflow-x-auto rounded-xl bg-zinc-900/70 p-2 ring-1 ring-zinc-800">
            <table className="font-mono text-xs" aria-label="Memory bytes">
              <tbody>
                {Array.from({ length: ROWS }, (_, row) => {
                  const start = row * COLS;
                  return (
                    <tr key={row}>
                      <td className="pr-3 text-zinc-500">{hex(view.address + start)}</td>
                      {Array.from({ length: COLS }, (_, col) => {
                        const i = start + col;
                        const ok = view.readable[i];
                        const cls =
                          i === selected
                            ? "bg-emerald-700 text-bright"
                            : changed.has(i)
                              ? "text-amber-300"
                              : ok
                                ? "text-zinc-200"
                                : "text-zinc-700";
                        return (
                          <td key={col} className={col === 8 ? "pl-2" : ""}>
                            <button
                              className={`rounded px-0.5 hover:bg-zinc-700 ${cls}`}
                              aria-label={`Byte at ${hex(view.address + i)}`}
                              onClick={() => select(i)}
                            >
                              {ok ? byteHex(view.bytes[i]!) : "??"}
                            </button>
                          </td>
                        );
                      })}
                      <td className="pl-3 whitespace-pre text-zinc-500">
                        {Array.from({ length: COLS }, (_, col) => {
                          const i = start + col;
                          return view.readable[i] ? ascii(view.bytes[i]!) : " ";
                        }).join("")}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </>
      )}

      {view && selAddress !== null && (
        <div className="flex flex-col gap-2 rounded-xl bg-zinc-900/70 p-3 text-xs ring-1 ring-zinc-800" aria-label="Selected byte">
          <p className="font-mono text-zinc-300">{hex(selAddress)}</p>
          {selBytes.length === 0 ? (
            <p className="text-zinc-500">This byte can&apos;t be read.</p>
          ) : (
            <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5">
              {inspect(selBytes).map((r) => (
                <div key={r.label} className="contents">
                  <dt className="text-zinc-500">{r.label}</dt>
                  <dd className="font-mono">{r.value}</dd>
                </div>
              ))}
              {pointer !== null && (
                <div className="contents">
                  <dt className="text-zinc-500">pointer</dt>
                  <dd className="font-mono">
                    {hex(pointer)}{" "}
                    <button className="ml-1 text-sky-300 hover:underline" onClick={() => void go(hex(pointer))}>
                      Go there
                    </button>
                  </dd>
                </div>
              )}
            </dl>
          )}
          <div className="flex flex-wrap items-center gap-2">
            <input
              className="w-48 rounded-lg bg-zinc-900 px-2 py-1 font-mono outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
              aria-label="New bytes"
              placeholder="E8 03 00 00"
              value={editHex}
              onChange={(e) => setEditHex(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && editHex.trim() && void write()}
            />
            <button className="rounded-lg btn-accent px-2.5 py-1 disabled:opacity-40" disabled={!editHex.trim()} onClick={() => void write()}>
              Write bytes
            </button>
            <span className="text-zinc-600">|</span>
            <select
              className="rounded-lg bg-zinc-900 px-2 py-1 ring-1 ring-zinc-800"
              aria-label="Type for the address list"
              value={ty}
              onChange={(e) => setTy(e.target.value as ValueType)}
            >
              {VALUE_TYPES.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
            <button className={button} onClick={() => onAddToList(selAddress, ty)}>
              Add to address list
            </button>
          </div>
          {message && (
            <p className="text-emerald-300" role="status">
              {message}
            </p>
          )}
        </div>
      )}
    </section>
  );
}
