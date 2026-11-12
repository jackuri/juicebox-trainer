import { useEffect, useMemo, useState } from "react";
import { friendlyError } from "../lib/explain";
import { type AiFix, describeCheat, kindOf, toEdit } from "../lib/cheatBook";
import { api, type BookView, type Cheat, PACK_SOURCE_LABEL } from "../lib/ipc";
import { groupByCategory } from "../lib/trainer";

const button = "rounded-lg bg-zinc-800 px-3 py-1.5 text-xs hover:bg-zinc-700 disabled:cursor-not-allowed disabled:opacity-40";
const field = "rounded-lg bg-zinc-950 px-2 py-1 text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600";

interface Props {
  packKey: string;
  onClose: () => void;
  /** Open the AI helper to fix a cheat (the game must run for it to work). */
  onAskAi: (fix: AiFix) => void;
  /** The pack was saved under this key (a built-in pack becomes the player's copy). */
  onSaved?: (key: string) => void;
}

/** Text boxes for a cheat's numbers (typed as text, checked on save). */
type Numbers = Record<string, string>;

const txt = (n: number | undefined) => (n === undefined ? "" : String(n));

function numbersOf(c: Cheat): Numbers {
  return {
    keepAt: txt(c.toggle?.value),
    min: txt(c.slider?.min ?? c.number?.min),
    max: txt(c.slider?.max ?? c.number?.max),
    step: txt(c.slider?.step),
    start: txt(c.slider?.default),
    set: txt(c.button?.set),
    add: txt(c.button?.add),
  };
}

/**
 * The cheat book (v1.28.0): every cheat in a pack, to look through and change without starting the
 * game. Where a cheat looks in the game is never changed here.
 */
export function CheatBook({ packKey, onClose, onAskAi, onSaved }: Props) {
  const [book, setBook] = useState<BookView | null>(null);
  const [cheats, setCheats] = useState<Cheat[]>([]);
  const [numbers, setNumbers] = useState<Record<string, Numbers>>({});
  const [notes, setNotes] = useState<Record<string, string>>({});
  const [favourites, setFavourites] = useState<string[]>([]);
  const [keys, setKeys] = useState<Record<string, string>>({});
  const [open, setOpen] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [asking, setAsking] = useState<{ id: string; problem: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [key, setKey] = useState(packKey);

  function show(b: BookView) {
    setBook(b);
    setCheats(b.pack.cheats);
    setNumbers(Object.fromEntries(b.pack.cheats.map((c) => [c.id, numbersOf(c)])));
    setNotes(b.notes ?? {});
    setFavourites(b.favourites ?? []);
    setKeys(b.keys ?? {});
  }

  useEffect(() => {
    let live = true;
    api.bookOpen(key).then(
      (b) => live && show(b),
      (e: unknown) => live && setError(friendlyError(e)),
    );
    return () => {
      live = false;
    };
  }, [key]);

  const edit = useMemo(() => (book ? toEdit(cheats, numbers, notes, favourites, keys) : null), [book, cheats, numbers, notes, favourites, keys]);
  const dirty = useMemo(() => {
    if (!book || !edit) return false;
    const before = toEdit(book.pack.cheats, Object.fromEntries(book.pack.cheats.map((c) => [c.id, numbersOf(c)])), book.notes ?? {}, book.favourites ?? [], book.keys ?? {});
    return JSON.stringify(before) !== JSON.stringify(edit);
  }, [book, edit]);

  if (!book) {
    return (
      <section className="flex flex-col gap-2 rounded-2xl bg-zinc-900/90 p-4 ring-1 ring-emerald-800/70" aria-label="Cheat book">
        {error ? (
          <p className="text-sm text-red-300" role="alert">
            {error}
          </p>
        ) : (
          <p className="text-sm text-zinc-400">Opening…</p>
        )}
        <button className={`${button} self-start`} onClick={onClose}>
          Close
        </button>
      </section>
    );
  }

  const change = (id: string, patch: Partial<Cheat>) => setCheats((list) => list.map((c) => (c.id === id ? { ...c, ...patch } : c)));
  const setNum = (id: string, name: string, v: string) => setNumbers((all) => ({ ...all, [id]: { ...all[id], [name]: v } }));
  const q = filter.trim().toLowerCase();
  const shown = q ? cheats.filter((c) => `${c.name} ${c.category ?? ""} ${c.description ?? ""}`.toLowerCase().includes(q)) : cheats;

  async function save() {
    if (!edit) return;
    if (typeof edit === "string") {
      setError(edit);
      return;
    }
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      const r = await api.bookSave(key, edit);
      const parts = ["Saved."];
      if (r.copied) parts.push("It's your own copy of the pack now (in Packs).");
      if (r.reloaded) parts.push("The pack was in use, so it was loaded again: turn your cheats back on in Cheats.");
      setMessage(parts.join(" "));
      onSaved?.(r.key);
      if (r.key !== key) setKey(r.key);
      else show(await api.bookOpen(r.key));
    } catch (e) {
      setError(friendlyError(e));
    } finally {
      setBusy(false);
    }
  }

  function undo() {
    show(book!);
    setError(null);
    setMessage(null);
  }

  return (
    <section className="flex flex-col gap-3 rounded-2xl bg-zinc-900/90 p-4 ring-1 ring-emerald-800/70" aria-label="Cheat book">
      <div className="flex flex-wrap items-center gap-2">
        <div className="min-w-0 flex-1">
          <h3 className="text-lg font-semibold tracking-tight">{book.pack.game.name}</h3>
          <p className="text-xs text-zinc-400">
            {cheats.length} cheat{cheats.length === 1 ? "" : "s"} · {PACK_SOURCE_LABEL[book.source]} · {book.pack.game.exe}
          </p>
        </div>
        <input aria-label="Find a cheat" className={`${field} w-48`} placeholder="Find a cheat…" value={filter} onChange={(e) => setFilter(e.target.value)} />
        <button className={button} onClick={onClose}>
          Close
        </button>
      </div>
      <p className="text-xs text-zinc-400">
        Look through your cheats and change their names, groups, values, keys and notes, no game needed. Where each cheat looks in the
        game stays as it is.
        {book.source === "bundled" && " This pack is built into JuiceBox Trainer: your changes are saved as your own copy."}
        {book.loaded && " It's in use in Cheats right now: saving loads it again, which turns its cheats off."}
      </p>

      {groupByCategory(shown).map(([group, list]) => (
        <div key={group} className="flex flex-col gap-1.5">
          <p className="text-xs font-medium uppercase tracking-wider text-zinc-500">{group}</p>
          <ul className="flex flex-col gap-1.5">
            {list.map((c) => {
              const fav = favourites.includes(c.id);
              const keyable = c.ui === "toggle" || c.ui === "button";
              const own = keys[c.id];
              const n = numbers[c.id] ?? numbersOf(c);
              const expanded = open === c.id;
              return (
                <li key={c.id} className="rounded-xl bg-zinc-950/60 ring-1 ring-zinc-800">
                  <div className="flex flex-wrap items-center gap-2 px-3 py-2 text-sm">
                    <button
                      className={`text-base ${fav ? "text-amber-300" : "text-zinc-600 hover:text-zinc-300"}`}
                      aria-label={fav ? `Unfavourite ${c.name}` : `Favourite ${c.name}`}
                      aria-pressed={fav}
                      onClick={() => setFavourites((f) => (fav ? f.filter((x) => x !== c.id) : [...f, c.id]))}
                    >
                      {fav ? "★" : "☆"}
                    </button>
                    <span className="min-w-0 flex-1">
                      <span className="font-medium text-zinc-100">{c.name}</span>
                      <span className="text-xs text-zinc-400"> · {describeCheat(c)}</span>
                      {keyable && (own !== undefined ? own : c.hotkey) && (
                        <span className="ml-2 rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-[11px] text-zinc-300">{own !== undefined ? own || "no key" : c.hotkey}</span>
                      )}
                      {notes[c.id] && <span className="block text-xs text-zinc-500">{notes[c.id]}</span>}
                    </span>
                    <button className={button} aria-expanded={expanded} aria-label={`Edit ${c.name}`} onClick={() => setOpen(expanded ? null : c.id)}>
                      {expanded ? "Done" : "Edit"}
                    </button>
                  </div>
                  {expanded && (
                    <div className="flex flex-col gap-2 border-t border-zinc-800 px-3 py-2 text-xs" role="group" aria-label={`Change ${c.name}`}>
                      <label className="flex flex-col gap-1">
                        Name
                        <input className={field} maxLength={60} value={c.name} onChange={(e) => change(c.id, { name: e.target.value })} />
                      </label>
                      <label className="flex flex-col gap-1">
                        Group
                        <input className={field} maxLength={40} placeholder="General" value={c.category ?? ""} onChange={(e) => change(c.id, { category: e.target.value })} />
                      </label>
                      <label className="flex flex-col gap-1">
                        What it does
                        <textarea className={field} rows={2} maxLength={400} value={c.description ?? ""} onChange={(e) => change(c.id, { description: e.target.value })} />
                      </label>
                      {kindOf(c) === "keep" && (
                        <label className="flex flex-col gap-1">
                          Keep it at (empty: wherever it is when you turn it on)
                          <input className={`${field} w-32`} inputMode="decimal" value={n.keepAt} onChange={(e) => setNum(c.id, "keepAt", e.target.value)} />
                        </label>
                      )}
                      {c.ui === "slider" && (
                        <div className="flex flex-wrap gap-2">
                          {(
                            [
                              ["min", "Lowest"],
                              ["max", "Highest"],
                              ["step", "Step"],
                              ["start", "Starts at"],
                            ] as const
                          ).map(([name, label]) => (
                            <label key={name} className="flex flex-col gap-1">
                              {label}
                              <input className={`${field} w-24`} inputMode="decimal" value={n[name]} onChange={(e) => setNum(c.id, name, e.target.value)} />
                            </label>
                          ))}
                          <label className="flex items-end gap-1.5 pb-1">
                            <input type="checkbox" checked={c.slider?.hold ?? false} onChange={(e) => change(c.id, { slider: { ...c.slider!, hold: e.target.checked } })} />
                            Keep it there
                          </label>
                        </div>
                      )}
                      {c.ui === "number" && (
                        <div className="flex flex-wrap gap-2">
                          <label className="flex flex-col gap-1">
                            Lowest
                            <input className={`${field} w-24`} inputMode="decimal" value={n.min} onChange={(e) => setNum(c.id, "min", e.target.value)} />
                          </label>
                          <label className="flex flex-col gap-1">
                            Highest
                            <input className={`${field} w-24`} inputMode="decimal" value={n.max} onChange={(e) => setNum(c.id, "max", e.target.value)} />
                          </label>
                          <label className="flex items-end gap-1.5 pb-1">
                            <input type="checkbox" checked={c.number?.hold ?? false} onChange={(e) => change(c.id, { number: { ...c.number, hold: e.target.checked } })} />
                            Keep it there
                          </label>
                        </div>
                      )}
                      {c.ui === "button" && (
                        <label className="flex flex-col gap-1">
                          {c.button?.set !== undefined ? "Sets it to" : "Adds"}
                          <input
                            className={`${field} w-32`}
                            inputMode="decimal"
                            value={c.button?.set !== undefined ? n.set : n.add}
                            onChange={(e) => setNum(c.id, c.button?.set !== undefined ? "set" : "add", e.target.value)}
                          />
                        </label>
                      )}
                      {keyable && (
                        <div className="flex flex-wrap items-end gap-2">
                          <label className="flex flex-col gap-1">
                            Key (e.g. F5, Ctrl+1)
                            <input
                              className={`${field} w-32`}
                              placeholder={c.hotkey ?? "none"}
                              value={own ?? ""}
                              onChange={(e) => setKeys((k) => ({ ...k, [c.id]: e.target.value }))}
                            />
                          </label>
                          <button
                            className={button}
                            onClick={() =>
                              setKeys((k) => {
                                const next = { ...k };
                                delete next[c.id];
                                return next;
                              })
                            }
                          >
                            Use the pack&apos;s key{c.hotkey ? ` (${c.hotkey})` : ""}
                          </button>
                          <button className={button} onClick={() => setKeys((k) => ({ ...k, [c.id]: "" }))}>
                            No key
                          </button>
                        </div>
                      )}
                      <label className="flex flex-col gap-1">
                        Your note
                        <input
                          className={field}
                          maxLength={200}
                          placeholder="e.g. works after level 2"
                          value={notes[c.id] ?? ""}
                          onChange={(e) => setNotes((all) => ({ ...all, [c.id]: e.target.value }))}
                        />
                      </label>
                      <div className="flex flex-wrap gap-2 pt-1">
                        <button className={button} onClick={() => setAsking({ id: c.id, problem: "" })}>
                          It doesn&apos;t work: ask the AI to fix it
                        </button>
                        <button
                          className="rounded-lg px-3 py-1.5 text-xs text-red-300 hover:bg-red-950/50"
                          onClick={() => {
                            setCheats((list) => list.filter((x) => x.id !== c.id));
                            setOpen(null);
                          }}
                        >
                          Remove this cheat
                        </button>
                      </div>
                      {asking?.id === c.id && (
                        <div className="flex flex-col gap-2 rounded-lg bg-zinc-900 p-2" role="group" aria-label={`Ask the AI about ${c.name}`}>
                          <label className="flex flex-col gap-1">
                            What&apos;s wrong with it?
                            <textarea
                              className={field}
                              rows={2}
                              maxLength={300}
                              placeholder="e.g. my health still goes down, or it stopped working after the game updated"
                              value={asking.problem}
                              onChange={(e) => setAsking({ id: c.id, problem: e.target.value })}
                            />
                          </label>
                          <p className="text-zinc-400">
                            The AI helper opens with this filled in. Start the game first: it fixes cheats while the game runs.
                          </p>
                          <div className="flex gap-2">
                            <button
                              className="btn-accent rounded-lg px-3 py-1.5 text-xs font-semibold disabled:opacity-40"
                              disabled={!asking.problem.trim()}
                              onClick={() =>
                                onAskAi({
                                  game: book.pack.game.name,
                                  exe: book.pack.game.exe,
                                  cheat: c.name,
                                  problem: asking.problem.trim(),
                                  describe: describeCheat(c),
                                  key: book.key,
                                  id: c.id,
                                })
                              }
                            >
                              Open the AI helper
                            </button>
                            <button className={button} onClick={() => setAsking(null)}>
                              Cancel
                            </button>
                          </div>
                        </div>
                      )}
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        </div>
      ))}
      {shown.length === 0 && <p className="text-sm text-zinc-500">{q ? "No cheat with that name." : "This pack has no cheats left."}</p>}

      {error && (
        <p className="rounded-lg bg-red-950/40 px-3 py-2 text-sm text-red-300" role="alert">
          {error}
        </p>
      )}
      {message && (
        <p className="text-sm text-emerald-300" role="status">
          {message}
        </p>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <button className="btn-accent rounded-lg px-4 py-2 text-sm font-semibold disabled:opacity-40" disabled={!dirty || busy} onClick={() => void save()}>
          {busy ? "Saving…" : "Save changes"}
        </button>
        <button className={button} disabled={!dirty || busy} onClick={undo}>
          Undo changes
        </button>
        {dirty && <span className="text-xs text-amber-300">Not saved yet</span>}
      </div>
    </section>
  );
}
