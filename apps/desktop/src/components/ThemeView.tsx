import { useState } from "react";
import { seasonalOn, setSeasonalOn } from "../lib/mascot";
import { DEFAULT_THEME, loadTheme, saveTheme, type Theme, THEMES } from "../lib/theme";

/** A small live picture of the app in a theme (the card sets its own data-theme). */
function Preview({ theme }: { theme: Theme }) {
  return (
    <div
      data-theme={theme.id}
      className="pointer-events-none relative flex h-28 overflow-hidden rounded-xl bg-zinc-950 text-zinc-100 ring-1 ring-zinc-800"
      aria-hidden="true"
    >
      <div className="flex w-14 shrink-0 flex-col gap-1.5 bg-ink/70 p-2">
        <span className="h-3 w-3 rounded-md" style={{ backgroundImage: "linear-gradient(135deg, var(--accent-from), var(--accent-to))" }} />
        <span className="mt-1 h-2 rounded-sm bg-zinc-800" />
        <span className="h-2 w-8 rounded-sm bg-zinc-800/60" />
        <span className="h-2 w-7 rounded-sm bg-zinc-800/60" />
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-2 p-2.5">
        <span className="truncate text-[11px] font-semibold">Infinite health</span>
        <div className="flex items-center gap-2 rounded-lg bg-zinc-900 px-2 py-1.5 ring-1 ring-zinc-800">
          <span className="flex-1 truncate text-[10px] text-zinc-400">God mode</span>
          <span className="relative h-3 w-6 rounded-full bg-emerald-500">
            <span className="absolute right-0.5 top-0.5 h-2 w-2 rounded-full bg-white" />
          </span>
        </div>
        <div className="flex items-center gap-2">
          <span className="btn-accent rounded-lg px-2 py-0.5 text-[10px] font-semibold">Attach</span>
          <span className="text-[10px] text-emerald-300">Ready</span>
        </div>
      </div>
      {/* The theme's scanlines, if it has them. */}
      <span
        className="absolute inset-0"
        style={{ background: "repeating-linear-gradient(to bottom, rgb(0 0 0 / var(--scanlines)) 0 1px, transparent 1px 3px)" }}
      />
    </div>
  );
}

/** Pick how JuiceBox Trainer looks. The choice is shown at once and remembered. */
export function ThemeView() {
  const [current, setCurrent] = useState(loadTheme);
  const [holidays, setHolidays] = useState(seasonalOn);

  function pick(id: string) {
    saveTheme(id);
    setCurrent(id);
  }

  return (
    <main className="flex min-h-0 flex-col gap-5 overflow-y-auto p-6 text-sm" aria-label="Themes">
      <section>
        <h2 className="text-2xl font-semibold tracking-tight">Themes</h2>
        <p className="text-zinc-400">Pick a look for JuiceBox Trainer. It changes straight away, and the in-game overlay follows.</p>
      </section>
      <label className="flex items-start gap-3 rounded-xl bg-zinc-900/70 px-3 py-2.5 ring-1 ring-zinc-800">
        <input
          type="checkbox"
          className="mt-0.5"
          checked={holidays}
          onChange={(e) => {
            setSeasonalOn(e.target.checked);
            setHolidays(e.target.checked);
          }}
        />
        <span>
          <span className="font-medium">Holiday juice boxes</span>
          <span className="block text-xs text-zinc-400">
            The juice box in the corner and on the taskbar dresses up: Valentine&apos;s in February, Halloween in
            October, Thanksgiving in November and Christmas in December.
          </span>
        </span>
      </label>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-4">
        {THEMES.map((t) => {
          const on = t.id === current;
          return (
            <button
              key={t.id}
              aria-pressed={on}
              onClick={() => pick(t.id)}
              className={`flex flex-col gap-2.5 rounded-2xl p-2.5 text-left transition-colors ${
                on ? "bg-emerald-950/40 ring-2 ring-emerald-500" : "bg-zinc-900/70 ring-1 ring-zinc-800 hover:bg-zinc-900"
              }`}
            >
              <Preview theme={t} />
              <span className="flex flex-col gap-0.5 px-1 pb-1">
                <span className="flex items-center gap-2 font-medium">
                  {t.name}
                  {t.id === DEFAULT_THEME && <span className="text-[11px] font-normal text-zinc-500">default</span>}
                  {on && <span className="ml-auto rounded-full bg-emerald-800 px-2 py-0.5 text-[10px] font-semibold text-bright">In use</span>}
                </span>
                <span className="text-xs text-zinc-400">{t.description}</span>
              </span>
            </button>
          );
        })}
      </div>
    </main>
  );
}
