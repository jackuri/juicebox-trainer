import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, onWebCheatsChanged, onWebGameProblem, type Problem, type WebCheat, type WebGame, type WebInfo, type WebSearch, type WebTarget } from "../lib/ipc";
import { friendlyError } from "../lib/explain";
import { ProblemCard } from "./ProblemCard";
import { keyFromEvent } from "../lib/keys";
import { keysFor, lockId, where } from "../lib/webCheats";

const button = "rounded-lg bg-zinc-800 px-3 py-1.5 text-xs hover:bg-zinc-700 disabled:cursor-not-allowed disabled:opacity-40";
const field = "rounded-lg bg-zinc-950 px-2 py-1 text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600";

const num = (s: string): number | null => {
  const t = s.trim();
  if (!t) return null;
  const n = Number(t);
  return Number.isFinite(n) ? n : null;
};
/** A value for the screen. The game can hand back anything (a gone value comes back empty), so only real numbers are shown. */
const show = (v: unknown) => (typeof v !== "number" || !Number.isFinite(v) ? "?" : Number.isInteger(v) ? String(v) : v.toFixed(3));
/** Find, change and lock numbers in the open web game; the player's saved cheats for it. */
export function WebCheatsPanel({ game, onClose, onOpenAi }: { game: WebGame; onClose: () => void; onOpenAi?: () => void }) {
  const [info, setInfo] = useState<WebInfo | null>(null);
  const [waiting, setWaiting] = useState(true);
  const [value, setValue] = useState("");
  const [kind, setKind] = useState<"any" | "whole" | "decimal">("any");
  const [search, setSearch] = useState<WebSearch | null>(null);
  const [cheats, setCheats] = useState<WebCheat[]>([]);
  const [values, setValues] = useState<(number | undefined)[]>([]);
  const [inputs, setInputs] = useState<Record<number, string>>({});
  const [locked, setLocked] = useState<Set<string>>(new Set());
  const [naming, setNaming] = useState<{ target: WebTarget; name: string; value: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** The game page crashed or froze (v1.22.0). */
  const [pageProblem, setPageProblem] = useState<Problem | null>(null);
  /** Bumped by Reload game, to wait for the page again. */
  const [reloads, setReloads] = useState(0);
  useEffect(() => onWebGameProblem(setPageProblem), []);
  async function reload() {
    try {
      await api.webReload();
      setPageProblem(null);
      setSearch(null);
      setLocked(new Set());
      setWaiting(true);
      setReloads((n) => n + 1);
    } catch (e) {
      setError(friendlyError(e));
    }
  }
  const keys = useMemo(() => keysFor(cheats), [cheats]);
  const cheatsRef = useRef(cheats);
  const searchRef = useRef(search);
  const keysRef = useRef(keys);
  useEffect(() => {
    cheatsRef.current = cheats;
    searchRef.current = search;
    keysRef.current = keys;
  }, [cheats, search, keys]);

  // Wait for the game page (and JuiceBox Trainer's helper in it) to be ready.
  useEffect(() => {
    let live = true;
    let tries = 0;
    const look = () => {
      api.webInfo().then(
        (i) => {
          if (!live) return;
          setInfo(i);
          setWaiting(false);
        },
        () => {
          if (!live) return;
          if (++tries < 60) window.setTimeout(look, 1000);
          else setWaiting(false);
        },
      );
    };
    look();
    api.webCheats(game.id).then(
      (c) => live && setCheats(Array.isArray(c) ? c : []),
      () => undefined,
    );
    // The AI helper saved cheats for this game: show them.
    const stop = onWebCheatsChanged((id) => {
      if (id !== game.id) return;
      api.webCheats(game.id).then(
        (c) => live && setCheats(Array.isArray(c) ? c : []),
        () => undefined,
      );
    });
    return () => {
      live = false;
      stop();
    };
  }, [game.id, reloads]);

  // Live values: saved cheats, and the search results while there are few enough to show.
  useEffect(() => {
    if (waiting) return;
    let ticks = 0;
    const timer = window.setInterval(() => {
      if (document.hidden) return;
      // Games often make their memory after the page loads (or after a level starts): look again.
      if (++ticks % 3 === 0) api.webInfo().then(setInfo, () => undefined);
      // What's locked (keys in the game lock things too), and the keys, again after the page reloads.
      api.webState().then(
        (st) => {
          if (!st || !Array.isArray(st.locks)) return;
          setLocked(new Set(st.locks));
          const k = keysRef.current;
          if (st.keysTag !== k.tag) api.webKeys(k.list, k.tag).catch((e: unknown) => setError(friendlyError(e)));
        },
        () => undefined,
      );
      const list = cheatsRef.current;
      if (list.length) {
        api.webRead(list.map((c) => c.target)).then(
          (r) => setValues(Array.isArray(r) ? r.map((x) => x.value) : []),
          () => undefined,
        );
      }
      const s = searchRef.current;
      // Only a well-formed answer replaces the list (a reloading page can answer with anything).
      if (s && s.count > 0 && s.count <= 100)
        api.webResults().then((r) => {
          if (r && typeof r.count === "number" && Array.isArray(r.results)) setSearch(r);
        }, () => undefined);
    }, 1000);
    return () => window.clearInterval(timer);
  }, [waiting]);

  const run = useCallback(async (f: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await f();
    } catch (e) {
      setError(friendlyError(e));
    } finally {
      setBusy(false);
    }
  }, []);

  async function saveCheats(list: WebCheat[]) {
    await api.webCheatsSave(game.id, list);
    setCheats(list);
  }

  const v = num(value);
  return (
    <section className="flex flex-col gap-4 rounded-2xl bg-zinc-900/70 p-4 ring-1 ring-emerald-800/60" aria-label={`Cheats for ${game.name}`}>
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="text-lg font-semibold">Cheats for {game.name}</h3>
        <span className="text-xs text-zinc-500">
          {waiting
            ? "Waiting for the game to load…"
            : !info
              ? "JuiceBox Trainer can't see into this page yet. Wait for the game to load and click Look again."
              : info.memories.length === 0
                ? "No game memory found yet (its JavaScript values can still be searched)"
                : `${info.memories.length} game memor${info.memories.length === 1 ? "y" : "ies"} found`}
        </span>
        {!waiting && (
          <button className={button} onClick={() => void run(async () => setInfo(await api.webInfo()))} disabled={busy}>
            Look again
          </button>
        )}
        {onOpenAi && (
          <button className={`${button} ml-auto bg-emerald-800 hover:bg-emerald-700`} onClick={onOpenAi}>
            Find cheats with AI
          </button>
        )}
        <button className={`${button} ${onOpenAi ? "" : "ml-auto"}`} onClick={onClose}>
          Close the game
        </button>
      </div>

      {pageProblem && (
        <ProblemCard
          problem={pageProblem}
          onDismiss={() => setPageProblem(null)}
          actions={
            <button className="btn-accent rounded-lg px-3 py-1.5 text-xs font-semibold" onClick={() => void reload()}>
              Reload game
            </button>
          }
        />
      )}
      {error && (
        <p className="rounded-lg bg-red-950/40 px-3 py-2 text-sm text-red-300" role="alert">
          {error}
        </p>
      )}

      <div className="flex flex-col gap-2" aria-label="Find a value">
        <p className="text-sm font-medium">Find a value</p>
        <p className="text-xs text-zinc-400">
          Type a number you see in the game (gold, health, ammo…) and click Find. Then change it in the game and search again until
          only a few are left.
        </p>
        <div className="flex flex-wrap items-center gap-2">
          <input aria-label="Value" className={`${field} w-32`} inputMode="decimal" value={value} onChange={(e) => setValue(e.target.value)} />
          <select aria-label="Kind of number" className={field} value={kind} onChange={(e) => setKind(e.target.value as typeof kind)}>
            <option value="any">Any number</option>
            <option value="whole">Whole number</option>
            <option value="decimal">Decimal number</option>
          </select>
          <button className="btn-accent rounded-lg px-4 py-1.5 text-sm font-semibold disabled:opacity-40" disabled={busy || waiting || v === null} onClick={() => void run(async () => setSearch(await api.webFind(v!, kind)))}>
            {search ? "New search" : "Find"}
          </button>
          {search && (
            <>
              <button className={button} disabled={busy || v === null} onClick={() => void run(async () => setSearch(await api.webNext("equals", v!)))}>
                It&apos;s now this value
              </button>
              <button className={button} disabled={busy} onClick={() => void run(async () => setSearch(await api.webNext("increased")))}>
                Went up
              </button>
              <button className={button} disabled={busy} onClick={() => void run(async () => setSearch(await api.webNext("decreased")))}>
                Went down
              </button>
              <button className={button} disabled={busy} onClick={() => void run(async () => setSearch(await api.webNext("changed")))}>
                Changed
              </button>
              <button className={button} disabled={busy} onClick={() => void run(async () => setSearch(await api.webNext("unchanged")))}>
                Didn&apos;t change
              </button>
            </>
          )}
        </div>
        {search && (
          <p className="text-xs text-zinc-400" role="status">
            {search.count === 0
              ? "Nothing found. Check the number, or try another kind of number."
              : `${search.count} found${search.tooMany ? " (too many to keep: search for a less common number)" : ""}.${search.count > 100 ? " Change it in the game and search again." : ""}`}
          </p>
        )}
        {search && search.count > 0 && search.count <= 100 && (
          <ul className="flex max-h-64 flex-col gap-1 overflow-y-auto" aria-label="Results">
            {(Array.isArray(search.results) ? search.results : []).map((h) => (
              <li key={h.id} className="flex items-center gap-2 rounded bg-zinc-950/60 px-2 py-1 text-xs">
                <span className="w-20 shrink-0 font-mono text-sm text-zinc-100">{show(h.value)}</span>
                <span className="min-w-0 flex-1 truncate text-zinc-400">{h.where}</span>
                <button
                  className={button}
                  disabled={!h.target}
                  onClick={() => setNaming({ target: h.target, name: `Cheat ${cheats.length + 1}`, value: typeof h.value === "number" ? h.value : NaN })}
                >
                  Add to my cheats
                </button>
              </li>
            ))}
          </ul>
        )}
        {naming && (
          <div className="flex flex-wrap items-center gap-2 rounded-lg bg-zinc-950/60 p-2 text-sm" role="group" aria-label="Name the cheat">
            Name:
            <input aria-label="Cheat name" className={field} maxLength={80} value={naming.name} onChange={(e) => setNaming({ ...naming, name: e.target.value })} />
            <button
              className="btn-accent rounded-lg px-3 py-1 text-xs font-semibold disabled:opacity-40"
              disabled={busy || !naming.name.trim()}
              onClick={() =>
                void run(async () => {
                  await saveCheats([...cheats, { name: naming.name.trim(), target: naming.target, value: Number.isFinite(naming.value) ? naming.value : null }]);
                  setNaming(null);
                })
              }
            >
              Save
            </button>
            <button className={button} onClick={() => setNaming(null)}>
              Cancel
            </button>
          </div>
        )}
      </div>

      <div className="flex flex-col gap-2" aria-label="My cheats">
        <p className="text-sm font-medium">My cheats for this game</p>
        {cheats.length === 0 ? (
          <p className="text-xs text-zinc-500">None yet. Find a value above and click “Add to my cheats”.</p>
        ) : (
          <ul className="flex flex-col gap-1">
            {cheats.map((c, i) => {
              const id = lockId(c);
              const on = locked.has(id);
              const typed = num(inputs[i] ?? "");
              return (
                <li key={id} className="flex flex-wrap items-center gap-2 rounded bg-zinc-950/60 px-2 py-1.5 text-sm">
                  <span className="min-w-0 flex-1">
                    <span className="font-medium">{c.name}</span>{" "}
                    <span className="text-xs text-zinc-500">{where(c.target)}</span>
                  </span>
                  <span className="w-20 text-right font-mono" aria-label={`${c.name} value`}>
                    {show(values[i])}
                  </span>
                  <input
                    aria-label={`New value for ${c.name}`}
                    className={`${field} w-24`}
                    inputMode="decimal"
                    placeholder={c.value != null ? String(c.value) : ""}
                    value={inputs[i] ?? ""}
                    onChange={(e) => setInputs({ ...inputs, [i]: e.target.value })}
                  />
                  <button
                    className={button}
                    disabled={busy || typed === null}
                    onClick={() =>
                      void run(async () => {
                        await api.webSet(c.target, typed!);
                        const next = cheats.map((x, j) => (j === i ? { ...x, value: typed } : x));
                        await saveCheats(next);
                      })
                    }
                  >
                    Set
                  </button>
                  <button
                    className={on ? "rounded-lg bg-emerald-700 px-3 py-1.5 text-xs font-medium hover:bg-emerald-600" : button}
                    aria-pressed={on}
                    disabled={busy || (!on && (typed ?? c.value ?? values[i]) == null)}
                    title="Keep it at this value"
                    onClick={() =>
                      void run(async () => {
                        const s = new Set(locked);
                        if (on) {
                          await api.webUnlock(id);
                          s.delete(id);
                        } else {
                          await api.webLock(id, c.target, (typed ?? c.value ?? values[i])!);
                          s.add(id);
                        }
                        setLocked(s);
                      })
                    }
                  >
                    {on ? "Locked" : "Lock"}
                  </button>
                  <input
                    aria-label={`Key for ${c.name}`}
                    className={`${field} w-24 cursor-pointer text-center text-xs`}
                    readOnly
                    placeholder="Key"
                    title="Click, then press a key. Backspace: no key."
                    value={c.key ?? ""}
                    onKeyDown={(e) => {
                      if (e.key === "Tab") return;
                      e.preventDefault();
                      const box = e.currentTarget;
                      if (e.key === "Escape") return box.blur();
                      const k = e.key === "Backspace" ? null : keyFromEvent(e);
                      if (k === null && e.key !== "Backspace") return;
                      box.blur();
                      void run(() => saveCheats(cheats.map((x, j) => (j === i ? { ...x, key: k, keyDoes: k ? (x.keyDoes ?? "lock") : null } : x))));
                    }}
                  />
                  {c.key && (
                    <select
                      aria-label={`What the key for ${c.name} does`}
                      className={`${field} text-xs`}
                      value={c.keyDoes ?? "lock"}
                      onChange={(e) =>
                        void run(() => saveCheats(cheats.map((x, j) => (j === i ? { ...x, keyDoes: e.target.value as "lock" | "set" } : x))))
                      }
                    >
                      <option value="lock">Lock / unlock</option>
                      <option value="set" disabled={c.value == null}>
                        Set to {c.value ?? "…"}
                      </option>
                    </select>
                  )}
                  <button
                    className="text-xs text-zinc-500 hover:text-red-300"
                    aria-label={`Remove ${c.name}`}
                    onClick={() =>
                      void run(async () => {
                        if (on) {
                          await api.webUnlock(id);
                          const s = new Set(locked);
                          s.delete(id);
                          setLocked(s);
                        }
                        await saveCheats(cheats.filter((_, j) => j !== i));
                      })
                    }
                  >
                    Remove
                  </button>
                </li>
              );
            })}
          </ul>
        )}
        <p className="text-xs text-zinc-500">
          Keys work while the game window is in front: a key locks or unlocks its cheat (or sets it), and a note in the corner of the game
          says so. Cheats are found again by where they were. If the game changes (an update, or a new level that moves things), find them again.
        </p>
      </div>
    </section>
  );
}
