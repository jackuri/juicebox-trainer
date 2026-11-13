import { useCallback, useEffect, useRef, useState } from "react";
import { api, explain, isProblem, type Action, type Cheat, type Pc, type View } from "../lib/ipc";
import { CheatRow } from "./CheatRow";

/** How often the PC is asked what's on (while the screen shows). */
export const POLL_MS = 1000;

type Link = "connecting" | "connected" | "unreachable" | "unpaired";

/** The remote for one PC: its game's cheats by group, presets, and what happened. */
export function Remote({ pc, onBack, onForgotten }: { pc: Pc; onBack: () => void; onForgotten: () => void }) {
  const [view, setView] = useState<View | null>(null);
  const [link, setLink] = useState<Link>("connecting");
  const [problem, setProblem] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [closedGroups, setClosedGroups] = useState<Set<string>>(new Set());
  const [confirmForget, setConfirmForget] = useState(false);
  // A change in flight: polls don't overwrite what the player just did.
  const acting = useRef(0);

  const poll = useCallback(async () => {
    if (acting.current > 0) return;
    try {
      const v = await api.state(pc.id);
      if (acting.current > 0) return;
      setView(v);
      setLink("connected");
      setProblem(null);
    } catch (e) {
      setLink(isProblem(e) && e.kind === "unpaired" ? "unpaired" : "unreachable");
      setProblem(explain(e));
    }
  }, [pc.id]);

  useEffect(() => {
    const first = setTimeout(() => void poll(), 0);
    const t = setInterval(() => {
      if (document.visibilityState === "visible") void poll();
    }, POLL_MS);
    return () => {
      clearTimeout(first);
      clearInterval(t);
    };
  }, [poll]);

  useEffect(() => {
    if (!note) return;
    const t = setTimeout(() => setNote(null), 3000);
    return () => clearTimeout(t);
  }, [note]);

  async function change(key: string, run: () => Promise<View>, done?: string) {
    acting.current++;
    setBusy(key);
    try {
      setView(await run());
      setLink("connected");
      setProblem(null);
      if (done) setNote(done);
    } catch (e) {
      setNote(explain(e));
    } finally {
      acting.current--;
      setBusy(null);
    }
  }

  const act = (c: Cheat, action: Omit<Action, "id">) =>
    void change(c.id, () => api.act(pc.id, { id: c.id, ...action }));

  const cheats = view?.cheats ?? [];
  const pending = new Set(view?.pending ?? []);
  const groups: { name: string; cheats: Cheat[] }[] = [];
  for (const c of cheats) {
    const name = c.category || "Cheats";
    const g = groups.find((x) => x.name === name);
    if (g) g.cheats.push(c);
    else groups.push({ name, cheats: [c] });
  }
  const on = cheats.filter((c) => c.active).length;

  return (
    <main className="flex flex-col gap-3 p-4 pb-10">
      <header className="sticky top-0 z-10 -mx-4 flex items-center gap-2 bg-zinc-950/90 px-4 py-2 backdrop-blur">
        <button className="rounded-full px-2 py-1 text-2xl text-zinc-300" aria-label="Your PCs" onClick={onBack}>
          ‹
        </button>
        <div className="min-w-0 flex-1">
          <h1 className="truncate text-lg font-semibold">{view?.pack ? view.pack.game : pc.name}</h1>
          <p className="flex items-center gap-1.5 text-xs text-zinc-400" aria-live="polite">
            <span
              aria-hidden
              className={`inline-block h-2 w-2 rounded-full ${
                link === "connected" ? "bg-emerald-400" : link === "connecting" ? "bg-zinc-500" : "bg-red-400"
              }`}
            />
            {link === "connected"
              ? `Connected to ${pc.name}${view?.pack ? ` · ${on} on` : ""}`
              : link === "connecting"
                ? `Connecting to ${pc.name}…`
                : link === "unpaired"
                  ? "Not paired any more"
                  : "Can't reach the PC"}
          </p>
        </div>
      </header>

      {problem && link !== "connected" && (
        <div role="alert" className="rounded-2xl bg-red-950/50 p-3 text-sm text-red-100 ring-1 ring-red-900">
          <p>{problem}</p>
          {link === "unreachable" && (
            <p className="mt-1 text-red-200/80">The remote keeps trying by itself.</p>
          )}
        </div>
      )}

      {view && !view.pack && (
        <section
          aria-label={view.closed ? "Game closed" : "No game yet"}
          className="flex flex-col items-center gap-3 rounded-2xl bg-zinc-900 p-6 text-center ring-1 ring-zinc-800"
        >
          <img src={view.closed ? "/error.png" : "/wink.png"} alt="" className="h-20 w-20" />
          {view.closed ? (
            <>
              <h2 className="text-lg font-semibold">{view.closed} closed</h2>
              <p className="text-sm text-zinc-400">
                Start it again, then attach to it in JuiceBox Trainer on your PC. Its cheats come back here by
                themselves.
              </p>
            </>
          ) : (
            <>
              <h2 className="text-lg font-semibold">No game yet</h2>
              <p className="text-sm text-zinc-400">
                On your PC, attach JuiceBox Trainer to a game and load its cheats. They show here by themselves.
              </p>
            </>
          )}
        </section>
      )}

      {view?.pack && (view.presets?.length ?? 0) > 0 && (
        <section aria-label="Presets" className="flex flex-col gap-2">
          <h2 className="text-xs font-semibold uppercase tracking-wider text-zinc-500">Presets</h2>
          <div className="flex flex-wrap gap-2">
            {view.presets?.map((name) => (
              <button
                key={name}
                className="rounded-full bg-zinc-800 px-4 py-2 text-sm ring-1 ring-zinc-700 active:bg-zinc-700 disabled:opacity-50"
                disabled={busy !== null}
                onClick={() => void change(`preset:${name}`, () => api.preset(pc.id, name), `Turned on "${name}"`)}
              >
                {name}
              </button>
            ))}
          </div>
        </section>
      )}

      {groups.map((g) => {
        const shut = closedGroups.has(g.name);
        return (
          <section key={g.name} aria-label={g.name} className="flex flex-col gap-2">
            <button
              className="flex items-center gap-2 pt-2 text-left text-xs font-semibold uppercase tracking-wider text-zinc-500"
              aria-expanded={!shut}
              onClick={() =>
                setClosedGroups((s) => {
                  const next = new Set(s);
                  if (shut) next.delete(g.name);
                  else next.add(g.name);
                  return next;
                })
              }
            >
              <span aria-hidden>{shut ? "▸" : "▾"}</span>
              {g.name}
              <span className="font-normal normal-case tracking-normal text-zinc-600">({g.cheats.length})</span>
            </button>
            {!shut &&
              g.cheats.map((c) => (
                <CheatRow
                  key={c.id}
                  cheat={c}
                  waiting={pending.has(c.id)}
                  busy={busy === c.id}
                  disabled={link !== "connected"}
                  onAct={(a) => act(c, a)}
                />
              ))}
          </section>
        );
      })}

      {note && (
        <p
          role="status"
          className="fixed inset-x-4 bottom-6 z-20 mx-auto max-w-md rounded-2xl bg-zinc-800 px-4 py-3 text-center text-sm shadow-lg ring-1 ring-zinc-700"
        >
          {note}
        </p>
      )}

      <section className="mt-6 flex flex-col items-center gap-2 text-sm">
        {confirmForget ? (
          <div className="flex flex-col items-center gap-2 rounded-2xl bg-zinc-900 p-4 text-center ring-1 ring-zinc-800">
            <p>Forget {pc.name}? To use it again, you&apos;ll scan its QR code again.</p>
            <div className="flex gap-2">
              <button className="rounded-xl bg-zinc-800 px-4 py-2" onClick={() => setConfirmForget(false)}>
                Keep it
              </button>
              <button
                className="rounded-xl bg-red-900 px-4 py-2 text-red-50"
                onClick={() => void api.forget(pc.id).then(onForgotten, onForgotten)}
              >
                Forget it
              </button>
            </div>
          </div>
        ) : (
          <button className="text-zinc-500 underline" onClick={() => setConfirmForget(true)}>
            Forget this PC
          </button>
        )}
      </section>
    </main>
  );
}
