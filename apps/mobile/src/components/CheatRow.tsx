import { useEffect, useRef, useState } from "react";
import type { Action, Cheat } from "../lib/ipc";

/** How long a slider waits after the last move before sending the value. */
export const SLIDER_WAIT_MS = 350;

/** One cheat: a switch, a button, a slider or a number box, like on the PC. */
export function CheatRow({
  cheat: c,
  waiting,
  busy,
  disabled,
  onAct,
}: {
  cheat: Cheat;
  waiting: boolean;
  busy: boolean;
  disabled: boolean;
  onAct: (a: Omit<Action, "id">) => void;
}) {
  const failed = c.status === "failed";
  const off = disabled || failed;
  const note = failed ? c.message || "Not available in this game version" : waiting ? "Waiting for the game…" : null;

  return (
    <div
      className={`flex flex-col gap-2 rounded-2xl bg-zinc-900 px-4 py-3 ring-1 ${
        c.active ? "ring-emerald-600/60" : "ring-zinc-800"
      }`}
    >
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <p className="text-[15px] leading-snug">{c.name}</p>
          {note && <p className={`text-xs ${failed ? "text-red-300" : "text-amber-300"}`}>{note}</p>}
          {!note && c.ui !== "toggle" && c.ui !== "button" && c.value !== null && (
            <p className="text-xs text-zinc-500">Now: {c.value}</p>
          )}
        </div>
        {c.ui === "toggle" && (
          <button
            role="switch"
            aria-checked={c.active}
            aria-label={c.name}
            disabled={off || busy}
            onClick={() => onAct({ op: "toggle", on: !c.active })}
            className={`relative h-8 w-14 shrink-0 rounded-full transition-colors disabled:opacity-40 ${
              c.active ? "btn-accent" : "bg-zinc-700"
            }`}
          >
            <span
              aria-hidden
              className={`absolute top-1 h-6 w-6 rounded-full bg-white shadow transition-all ${
                c.active ? "left-7" : "left-1"
              }`}
            />
          </button>
        )}
        {c.ui === "button" && (
          <button
            className="btn-accent shrink-0 rounded-xl px-4 py-2 text-sm font-medium disabled:opacity-40"
            disabled={off || busy}
            aria-label={`${c.name}: ${c.label || "Go"}`}
            onClick={() => onAct({ op: "press" })}
          >
            {c.label || "Go"}
          </button>
        )}
      </div>
      {c.ui === "slider" && c.min !== null && c.max !== null ? (
        <Slider cheat={c} disabled={off} onSet={(v) => onAct({ op: "set", value: v })} />
      ) : (
        (c.ui === "slider" || c.ui === "number") && (
          <NumberBox cheat={c} disabled={off || busy} onSet={(v) => onAct({ op: "set", value: v })} />
        )
      )}
    </div>
  );
}

function Slider({ cheat: c, disabled, onSet }: { cheat: Cheat; disabled: boolean; onSet: (v: string) => void }) {
  const min = Number(c.min);
  const max = Number(c.max);
  const now = c.value !== null && !Number.isNaN(Number(c.value)) ? Number(c.value) : min;
  const [value, setValue] = useState<number | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => () => {
    if (timer.current) clearTimeout(timer.current);
  }, []);
  const shown = value ?? now;
  return (
    <div className="flex items-center gap-3">
      <input
        type="range"
        className="h-8 flex-1"
        min={min}
        max={max}
        step={c.step ?? "any"}
        value={shown}
        disabled={disabled}
        aria-label={c.name}
        onChange={(e) => {
          const v = Number(e.target.value);
          setValue(v);
          if (timer.current) clearTimeout(timer.current);
          timer.current = setTimeout(() => {
            onSet(String(v));
            setValue(null);
          }, SLIDER_WAIT_MS);
        }}
      />
      <span className="w-14 text-right text-sm tabular-nums text-zinc-300">{shown}</span>
    </div>
  );
}

function NumberBox({ cheat: c, disabled, onSet }: { cheat: Cheat; disabled: boolean; onSet: (v: string) => void }) {
  const [text, setText] = useState("");
  return (
    <form
      className="flex gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (text.trim() !== "") {
          onSet(text.trim());
          setText("");
        }
      }}
    >
      <input
        type="number"
        inputMode="decimal"
        className="min-w-0 flex-1 rounded-xl bg-zinc-950 px-3 py-2 text-base ring-1 ring-zinc-700"
        placeholder={c.value ?? ""}
        min={c.min ?? undefined}
        max={c.max ?? undefined}
        value={text}
        disabled={disabled}
        aria-label={`${c.name}: new value`}
        onChange={(e) => setText(e.target.value)}
      />
      <button
        className="rounded-xl bg-zinc-800 px-4 py-2 text-sm disabled:opacity-40"
        disabled={disabled || text.trim() === ""}
      >
        Set
      </button>
    </form>
  );
}
