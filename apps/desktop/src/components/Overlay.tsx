import { useCallback, useEffect, useState } from "react";
import { api, onTrainerChanged, type TrainerView } from "../lib/ipc";

const POLL_MS = 500;

/**
 * The overlay window's content: the loaded pack's cheats, their keys and
 * which are on. The window is see-through and clicks pass through it.
 */
export function Overlay() {
  const [view, setView] = useState<TrainerView | null>(null);

  const poll = useCallback(() => {
    api.trainerState().then(setView, () => setView(null));
  }, []);

  useEffect(() => {
    poll();
    const t = setInterval(poll, POLL_MS);
    const stop = onTrainerChanged(poll);
    return () => {
      clearInterval(t);
      stop();
    };
  }, [poll]);

  if (!view) return null;
  const stateOf = (id: string) => view.cheats.find((c) => c.id === id);

  return (
    <div className="m-1 select-none rounded-lg bg-black/70 px-3 py-2 text-[13px] text-zinc-100 shadow-lg" data-testid="overlay">
      <p className="mb-1 truncate text-xs font-semibold text-emerald-300">{view.pack.game.name}</p>
      <ul className="flex flex-col gap-0.5">
        {view.pack.cheats.map((c) => {
          const st = stateOf(c.id);
          const keys = [st?.hotkey, view.pads[c.id]].filter(Boolean).join(" · ");
          const on = Boolean(st?.active);
          const failed = st?.status === "failed";
          return (
            <li key={c.id} className="flex items-center gap-2">
              <span
                aria-hidden
                className={`h-2 w-2 shrink-0 rounded-full ${failed ? "bg-red-500" : on ? "bg-emerald-400" : "bg-zinc-600"}`}
              />
              <span className={`flex-1 truncate ${failed ? "text-zinc-500" : on ? "" : "text-zinc-300"}`}>
                {c.name}
                {on && st?.value && c.ui !== "toggle" ? ` = ${st.value}` : ""}
              </span>
              {keys && <span className="font-mono text-[11px] text-zinc-400">{keys}</span>}
            </li>
          );
        })}
      </ul>
      {view.pending.length > 0 && <p className="mt-1 text-[11px] text-sky-300">Waiting for the game…</p>}
    </div>
  );
}
