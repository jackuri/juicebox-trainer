import { useState } from "react";
import { api, type TrainerView } from "../lib/ipc";

interface Props {
  view: TrainerView;
  act: (fn: () => Promise<TrainerView | void>) => Promise<void>;
}

/** Presets: save what's on as "Story mode", apply it later or when the game starts. */
export function PresetBar({ view, act }: Props) {
  const { presets, autoPreset } = view.settings;
  const [chosen, setChosen] = useState("");
  const [name, setName] = useState("");
  const current = presets.find((p) => p.name === chosen) ? chosen : (presets[0]?.name ?? "");
  const nameOf = (id: string) => view.pack.cheats.find((c) => c.id === id)?.name ?? id;

  return (
    <div className="flex flex-col gap-2 rounded-md bg-zinc-900/60 px-3 py-2" aria-label="Presets" role="group">
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <span className="font-medium text-zinc-300">Presets</span>
        {presets.length > 0 ? (
          <>
            <select
              aria-label="Preset"
              className="rounded bg-zinc-800 px-2 py-1"
              value={current}
              onChange={(e) => setChosen(e.target.value)}
            >
              {presets.map((p) => (
                <option key={p.name} value={p.name}>
                  {p.name} ({p.cheats.length})
                </option>
              ))}
            </select>
            <button
              className="rounded bg-emerald-700 px-2 py-1 hover:bg-emerald-600"
              onClick={() => void act(() => api.presetApply(current))}
            >
              Apply
            </button>
            <button
              className="rounded bg-zinc-800 px-2 py-1 hover:bg-zinc-700"
              aria-label={`Delete preset ${current}`}
              onClick={() => void act(() => api.presetDelete(current))}
            >
              Delete
            </button>
            <label className="flex items-center gap-1 text-zinc-400">
              <input
                type="checkbox"
                checked={autoPreset === current}
                onChange={(e) => void act(() => api.presetAuto(e.target.checked ? current : null))}
              />
              Use when I start the game from Games
            </label>
          </>
        ) : (
          <span className="text-zinc-500">Turn on the cheats you like, then save them as a preset.</span>
        )}
      </div>
      <div className="flex items-center gap-2 text-xs">
        <input
          aria-label="New preset name"
          className="w-48 rounded bg-zinc-950 px-2 py-1 outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
          placeholder="e.g. Story mode"
          maxLength={40}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <button
          className="rounded bg-zinc-800 px-2 py-1 hover:bg-zinc-700 disabled:opacity-40"
          disabled={!name.trim()}
          onClick={() =>
            void act(async () => {
              const v = await api.presetSave(name.trim());
              setChosen(name.trim());
              setName("");
              return v;
            })
          }
        >
          Save what&apos;s on now
        </button>
      </div>
      {view.pending.length > 0 && (
        <p className="text-xs text-sky-300" role="status">
          Waiting for the game: {view.pending.map(nameOf).join(", ")}. They turn on by themselves once they&apos;re
          available (e.g. when you&apos;re in a level).
        </p>
      )}
    </div>
  );
}
