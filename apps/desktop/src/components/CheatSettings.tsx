import { useEffect, useRef, useState } from "react";
import { chordFromButtons, heldButtons, keyFromEvent } from "../lib/keys";
import { api, type Cheat, type TrainerView } from "../lib/ipc";

interface Props {
  cheat: Cheat;
  view: TrainerView;
  act: (fn: () => Promise<TrainerView | void>) => Promise<void>;
}

const PAD_WAIT_MS = 10_000;

/** Per cheat: the player's own key, a controller combo and a note. */
export function CheatSettings({ cheat, view, act }: Props) {
  const keyable = cheat.ui === "toggle" || cheat.ui === "button";
  const own = view.settings.keys[cheat.id];
  const [note, setNote] = useState(view.settings.notes[cheat.id] ?? "");
  const [listening, setListening] = useState(false);
  const [padHint, setPadHint] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => () => {
    if (timer.current) clearInterval(timer.current);
  }, []);

  function capturePad() {
    if (timer.current) clearInterval(timer.current);
    setPadHint("Press and hold two or more buttons on your controller, then let go…");
    const seen = new Set<number>();
    const started = Date.now();
    timer.current = setInterval(() => {
      const held = heldButtons();
      held.forEach((b) => seen.add(b));
      const done = held.length === 0 && seen.size > 0;
      if (done || Date.now() - started > PAD_WAIT_MS) {
        if (timer.current) clearInterval(timer.current);
        timer.current = null;
        if (!done) {
          setPadHint("No controller buttons seen. Is the controller connected? You can also type the combo.");
          return;
        }
        const chord = chordFromButtons(seen);
        setPadHint(null);
        void act(() => api.cheatPad(cheat.id, chord));
      }
    }, 50);
  }

  return (
    <div className="mt-1 grid gap-2 text-xs text-zinc-400">
      {keyable && (
        <div className="flex flex-wrap items-center gap-2">
          <span className="w-24">Keyboard key</span>
          <input
            readOnly
            aria-label={`Key for ${cheat.name}`}
            className="w-36 rounded bg-zinc-950 px-2 py-1 font-mono outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
            value={listening ? "Press a key…" : own === "" ? "none" : (own ?? cheat.hotkey ?? "none")}
            onFocus={() => setListening(true)}
            onBlur={() => setListening(false)}
            onKeyDown={(e) => {
              if (e.key === "Tab") return;
              e.preventDefault();
              if (e.key === "Escape") return (e.target as HTMLInputElement).blur();
              if (e.key === "Backspace" || e.key === "Delete") {
                (e.target as HTMLInputElement).blur();
                return void act(() => api.cheatKey(cheat.id, ""));
              }
              const k = keyFromEvent(e);
              if (!k) return;
              (e.target as HTMLInputElement).blur();
              void act(() => api.cheatKey(cheat.id, k));
            }}
          />
          {own !== undefined && (
            <button className="rounded bg-zinc-800 px-2 py-1 hover:bg-zinc-700" onClick={() => void act(() => api.cheatKey(cheat.id, null))}>
              Use the pack&apos;s key
            </button>
          )}
          <span className="text-zinc-600">Click, then press the key. Backspace: no key.</span>
        </div>
      )}
      {keyable && (
        <div className="flex flex-wrap items-center gap-2">
          <span className="w-24">Controller</span>
          <PadInput
            key={view.pads[cheat.id] ?? ""}
            name={cheat.name}
            value={view.pads[cheat.id] ?? ""}
            onSave={(chord) => void act(() => api.cheatPad(cheat.id, chord))}
          />
          <button className="rounded bg-zinc-800 px-2 py-1 hover:bg-zinc-700" onClick={capturePad}>
            Press combo
          </button>
          {padHint && <span className="text-sky-300">{padHint}</span>}
        </div>
      )}
      <div className="flex items-start gap-2">
        <span className="w-24 pt-1">Note</span>
        <textarea
          aria-label={`Note for ${cheat.name}`}
          className="h-12 flex-1 rounded bg-zinc-950 px-2 py-1 outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
          maxLength={500}
          value={note}
          onChange={(e) => setNote(e.target.value)}
          onBlur={() => {
            if (note !== (view.settings.notes[cheat.id] ?? "")) void act(() => api.cheatNote(cheat.id, note));
          }}
        />
      </div>
    </div>
  );
}

function PadInput({ name, value, onSave }: { name: string; value: string; onSave: (chord: string) => void }) {
  const [text, setText] = useState(value);
  return (
    <input
      aria-label={`Controller combo for ${name}`}
      className="w-36 rounded bg-zinc-950 px-2 py-1 font-mono outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
      placeholder="e.g. LB+RB+DPadUp"
      value={text}
      onChange={(e) => setText(e.target.value)}
      onKeyDown={(e) => e.key === "Enter" && onSave(text.trim())}
      onBlur={() => text.trim() !== value && onSave(text.trim())}
    />
  );
}
