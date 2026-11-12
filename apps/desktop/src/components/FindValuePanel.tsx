import { useEffect, useState } from "react";
import { hex } from "../lib/format";
import {
  cheatName,
  loadPending,
  type PendingCheat,
  savePending,
  staticLocation,
  TEST_ANYWAY_LIMIT,
  TEST_LIMIT,
  testValue,
} from "../lib/finder";
import {
  api,
  type CommandError,
  type FinderHit,
  type FinderStep,
  type FinderSummary,
  isSessionLost,
  type NewCheat,
  type ScanProgress,
  type SessionInfo,
  toCommandError,
} from "../lib/ipc";
import { friendlyError } from "../lib/explain";

const QUICK_NAMES = ["Gold", "Health", "Ammo", "Lives"];

type Phase =
  | { k: "start" }
  | { k: "busy"; label: string }
  | { k: "narrow" }
  | { k: "test"; list: FinderHit[]; i: number; original: string; trying: string }
  | { k: "found"; hit: FinderHit; original: string; rest: FinderHit[] };

interface Props {
  session: SessionInfo;
  /** The game's name, for the "my cheats" pack. */
  gameName: string;
  onSessionLost: (e: CommandError) => void;
  /** Something was added to the address list. */
  onListChanged: () => void;
  /** Opens the Trainer tab. */
  onShowTrainer: () => void;
}

const input =
  "rounded-lg bg-zinc-900 px-3 py-2 text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600";
const primary = "btn-accent rounded-lg px-4 py-2 text-sm font-semibold disabled:opacity-40";
const secondary = "rounded-lg bg-zinc-800 px-4 py-2 text-sm hover:bg-zinc-700 disabled:opacity-40";

/**
 * "Find a value": finds a number in the game by asking simple questions (no
 * value types or addresses), tries each place it found to see which one
 * changes the number on screen, then keeps it, sets it or saves it as a cheat.
 */
export function FindValuePanel({ session, gameName, onSessionLost, onListChanged, onShowTrainer }: Props) {
  const exe = session.process.name;
  const [phase, setPhase] = useState<Phase>({ k: "start" });
  const [name, setName] = useState("");
  const [number, setNumber] = useState("");
  const [noNumber, setNoNumber] = useState(false);
  const [summary, setSummary] = useState<FinderSummary | null>(null);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [cheatValue, setCheatValue] = useState("");
  const [keep, setKeep] = useState(true);
  const [pending, setPending] = useState<PendingCheat | null>(() => loadPending(exe));

  const busy = phase.k === "busy";
  const label = cheatName(name);

  useEffect(() => {
    if (!busy) return;
    const timer = setInterval(() => {
      api.scanProgress().then(setProgress, () => undefined);
    }, 150);
    return () => clearInterval(timer);
  }, [busy]);

  function fail(e: unknown, back: Phase) {
    const err = toCommandError(e);
    if (isSessionLost(err)) {
      onSessionLost(err);
      return;
    }
    if (err.kind === "cancelled") setNotice("Stopped. Your earlier results are kept.");
    else setError(friendlyError(err));
    setPhase(back);
  }

  function reset() {
    void api.finderReset().catch(() => undefined);
    setSummary(null);
    setNumber("");
    setError(null);
    setNotice(null);
    setPhase({ k: "start" });
  }

  async function afterSearch(s: FinderSummary) {
    setSummary(s);
    setNumber("");
    if (s.count === 0) {
      setError(
        noNumber
          ? "Nothing matches. Start over and make sure you answer right after each change."
          : "Nothing matches that number. Check it's typed exactly as the game shows it, or start over and choose \"There's no number on screen\".",
      );
      setPhase({ k: "narrow" });
    } else if (noNumber && s.rounds === 1) {
      setPhase({ k: "narrow" });
    } else if (s.count <= TEST_LIMIT) {
      await beginTest();
    } else {
      setPhase({ k: "narrow" });
    }
  }

  async function start() {
    setError(null);
    setNotice(null);
    setProgress(null);
    setPhase({ k: "busy", label: noNumber ? "Remembering the game's memory…" : `Looking for ${number.trim()}…` });
    try {
      await afterSearch(await api.finderStart(noNumber ? null : number.trim()));
    } catch (e) {
      fail(e, { k: "start" });
    }
  }

  async function next(step: FinderStep) {
    setError(null);
    setNotice(null);
    setProgress(null);
    setPhase({ k: "busy", label: "Checking which places changed…" });
    try {
      await afterSearch(await api.finderNext(step));
    } catch (e) {
      fail(e, { k: "narrow" });
    }
  }

  async function beginTest() {
    try {
      await tryAt(await api.finderResults(TEST_ANYWAY_LIMIT), 0);
    } catch (e) {
      fail(e, { k: "narrow" });
    }
  }

  /** Writes a test number to place `i` and asks whether the game shows it. */
  async function tryAt(list: FinderHit[], i: number) {
    for (let j = i; j < list.length; j++) {
      const h = list[j];
      if (!h) continue;
      try {
        const now = await api.readValue(hex(h.address), h.ty);
        const trying = testValue(now.value, h.ty);
        if (trying === null) continue;
        await api.writeValue(hex(h.address), h.ty, trying);
        setPhase({ k: "test", list, i: j, original: now.value, trying });
        return;
      } catch (e) {
        const err = toCommandError(e);
        if (isSessionLost(err)) {
          onSessionLost(err);
          return;
        }
        // That place can't be read or written any more: try the next.
      }
    }
    setNotice(
      `None of those was your ${label}. Change it in the game once more and search again, or start over.`,
    );
    setPhase({ k: "narrow" });
  }

  async function answer(yes: boolean) {
    if (phase.k !== "test") return;
    const h = phase.list[phase.i];
    if (!h) return;
    // Put the real number back either way.
    await api.writeValue(hex(h.address), h.ty, phase.original).catch(() => undefined);
    if (yes) {
      // Finishing a cheat saved before the restart: show what it will do.
      const finishing = pending !== null && pending.pid !== session.process.pid;
      setCheatValue(finishing ? pending.value : phase.original);
      if (finishing) setKeep(pending.keep);
      setNotice(null);
      setPhase({ k: "found", hit: h, original: phase.original, rest: phase.list.slice(phase.i + 1) });
    } else {
      await tryAt(phase.list, phase.i + 1);
    }
  }

  async function applyNow() {
    if (phase.k !== "found") return;
    setError(null);
    try {
      const entry = await api.addressAdd(hex(phase.hit.address), phase.hit.ty, label);
      await api.addressWrite(entry.id, cheatValue.trim());
      if (keep) await api.addressSetFrozen(entry.id, true);
      onListChanged();
      setNotice(
        keep
          ? `${label} is kept at ${cheatValue.trim()} while you play. It's in the address list below: untick "Freeze" to stop.`
          : `${label} is now ${cheatValue.trim()}. It's in the address list below.`,
      );
    } catch (e) {
      fail(e, phase);
    }
  }

  async function saveWith(
    found: { address: number },
    where: Pick<NewCheat, "module" | "moduleOffset" | "offsets">,
    what: Pick<NewCheat, "name" | "ty" | "value" | "keep">,
    game: string,
  ) {
    const saved = await api.myCheatSave(game, { ...what, ...where }, found.address);
    setNotice(
      `Saved "${what.name}" in "${saved.packName}"${saved.hotkey ? ` (hotkey ${saved.hotkey})` : ""}. ` +
        (saved.loaded
          ? "It's ready in the Trainer tab, and next time you play too."
          : "Load that pack in the Trainer tab to use it."),
    );
  }

  async function saveCheat() {
    if (phase.k !== "found") return;
    const back = phase;
    const what = { name: label, ty: phase.hit.ty, value: cheatValue.trim(), keep };
    setError(null);
    setNotice(null);
    try {
      const modules = await api.refreshModules().catch(() => session.modules);
      const fixed = staticLocation(phase.hit.address, modules);
      if (fixed) {
        await saveWith(phase.hit, { module: fixed.module, moduleOffset: fixed.offset, offsets: [] }, what, gameName);
        return;
      }
      // The value moves each time the game starts: look for a path to it
      // now, and keep the paths that still work after a restart.
      setProgress(null);
      setPhase({ k: "busy", label: "Looking for a way to find it again next time…" });
      const r = await api.pointerScan(hex(phase.hit.address), { maxDepth: 4, maxOffset: 0x1000, maxResults: 5000 });
      if (r.paths.length === 0) {
        setError(
          `${label} can't be saved as a cheat: JuiceBox Trainer found no way to find it again after a restart. You can still use it now.`,
        );
        setPhase(back);
        return;
      }
      const p: PendingCheat = { exe, pid: session.process.pid, gameName, ...what };
      savePending(p);
      setPending(p);
      setPhase(back);
    } catch (e) {
      fail(e, back);
    }
  }

  async function finishPending() {
    if (phase.k !== "found" || !pending) return;
    setError(null);
    setNotice(null);
    try {
      const r = await api.pointerFilter(hex(phase.hit.address));
      const path = r.paths.find((p) => p.address === phase.hit.address) ?? r.paths[0];
      if (!path) {
        setError(
          `No way to find ${pending.name} survived the restart, so it can't be saved as a cheat. You can still use it now.`,
        );
      } else {
        await saveWith(
          phase.hit,
          { module: path.module, moduleOffset: path.moduleOffset, offsets: path.offsets },
          { name: pending.name, ty: pending.ty, value: pending.value, keep: pending.keep },
          pending.gameName,
        );
      }
      savePending(null);
      setPending(null);
    } catch (e) {
      const err = toCommandError(e);
      if (err.kind === "noScan") {
        setError(`JuiceBox Trainer was closed since, so it has to start again: click "Save as a cheat" below.`);
        savePending(null);
        setPending(null);
      } else {
        fail(e, phase);
      }
    }
  }

  function forgetPending() {
    savePending(null);
    setPending(null);
  }

  const restarted = pending !== null && pending.pid !== session.process.pid;
  const pct = progress && progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0;

  return (
    <section className="flex max-w-2xl flex-col gap-4" aria-label="Find a value">
      {pending && (
        <div className="rounded-xl bg-violet-950/40 px-4 py-3 text-sm ring-1 ring-violet-800/60" role="status">
          {restarted ? (
            <p>
              Almost done saving <b>{pending.name}</b>: find it once more below. When it's found, click{" "}
              <b>Finish saving</b>.
            </p>
          ) : (
            <p>
              To save <b>{pending.name}</b> for next time: close the game, start it again, attach, then find{" "}
              {pending.name} here once more. Keep JuiceBox Trainer open meanwhile.
            </p>
          )}
          <button className="mt-1 text-xs text-zinc-400 underline hover:text-zinc-200" onClick={forgetPending}>
            Don't save it
          </button>
        </div>
      )}

      {phase.k === "start" && (
        <div className="flex flex-col gap-4">
          <div>
            <h2 className="text-lg font-semibold">Find a value</h2>
            <p className="text-sm text-zinc-400">
              Find a number in the game (gold, health, ammo) by answering a few questions. Then keep it, change it,
              or save it as a cheat.
            </p>
          </div>
          <div className="flex flex-col gap-1.5 text-sm" role="group" aria-label="What do you want to change?">
            <p>What do you want to change?</p>
            <div className="flex flex-wrap items-center gap-2">
              {QUICK_NAMES.map((n) => (
                <button
                  key={n}
                  className={`rounded-full px-3 py-1 text-xs ring-1 ${
                    name === n ? "bg-emerald-700/40 ring-emerald-600" : "bg-zinc-900 ring-zinc-800 hover:bg-zinc-800"
                  }`}
                  onClick={() => setName(n)}
                >
                  {n}
                </button>
              ))}
              <input
                className={`${input} w-44`}
                placeholder="or type a name"
                aria-label="Value name"
                value={name}
                onChange={(e) => setName(e.target.value)}
              />
            </div>
          </div>
          {!noNumber ? (
            <div className="flex flex-col gap-1.5 text-sm">
              <p>What number does the game show for it right now?</p>
              <div className="flex items-center gap-2">
                <input
                  className={`${input} w-40 font-mono`}
                  placeholder="e.g. 1250"
                  aria-label="Number shown in the game"
                  inputMode="decimal"
                  value={number}
                  onChange={(e) => setNumber(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && number.trim() && void start()}
                />
                <button className={primary} disabled={!number.trim()} onClick={() => void start()}>
                  Search
                </button>
              </div>
              <button className={`${secondary} self-start`} onClick={() => setNoNumber(true)}>
                There's no number on screen (a bar, a battery, a meter)
              </button>
            </div>
          ) : (
            <div className="flex flex-col gap-2 text-sm">
              <p>
                OK. JuiceBox Trainer remembers the game's memory now. Then you change {label} in the game and say whether
                it went up or down.
              </p>
              <p className="text-xs text-zinc-400">
                Tip: for something that drains by itself (like a flashlight battery), let it drain a little and say
                &ldquo;It went down&rdquo;, then turn it off (or don&apos;t use it) for a moment and say &ldquo;It stayed
                the same&rdquo;. Do that a few times. Big games take a minute or two the first time.
              </p>
              <div className="flex gap-2">
                <button className={primary} onClick={() => void start()}>
                  Start
                </button>
                <button className={secondary} onClick={() => setNoNumber(false)}>
                  Back
                </button>
              </div>
            </div>
          )}
        </div>
      )}

      {phase.k === "busy" && (
        <div className="flex flex-col gap-2">
          <p className="text-sm">{phase.label}</p>
          <div className="flex items-center gap-3" role="progressbar" aria-valuenow={pct}>
            <div className="h-1.5 flex-1 overflow-hidden rounded bg-zinc-800">
              <div className="h-full bg-emerald-600 transition-all" style={{ width: `${pct}%` }} />
            </div>
            <span className="w-10 text-right text-xs text-zinc-400">{pct}%</span>
            <button className="rounded-lg bg-zinc-800 px-2 py-1 text-xs hover:bg-zinc-700" onClick={() => void api.scanCancel()}>
              Cancel
            </button>
          </div>
        </div>
      )}

      {phase.k === "narrow" && summary && (
        <div className="flex flex-col gap-3">
          <p className="text-sm text-zinc-400" data-testid="finder-count">
            {noNumber && summary.rounds === 1
              ? "Memory remembered."
              : `${summary.count.toLocaleString()} possible place${summary.count === 1 ? "" : "s"} for ${label}.`}
          </p>
          {summary.count > 0 &&
            (noNumber ? (
              <div className="flex flex-col gap-2 text-sm">
                <p>
                  Now make {label} change in the game (take a hit, use some). Then come back and say what happened:
                </p>
                <div className="flex flex-wrap gap-2">
                  <button className={primary} onClick={() => void next({ kind: "decreased" })}>
                    It went down
                  </button>
                  <button className={primary} onClick={() => void next({ kind: "increased" })}>
                    It went up
                  </button>
                  {summary.rounds > 1 && (
                    <button className={secondary} onClick={() => void next({ kind: "unchanged" })}>
                      It stayed the same
                    </button>
                  )}
                </div>
              </div>
            ) : (
              <div className="flex flex-col gap-1.5 text-sm">
                <p>Now make {label} change in the game (spend some, pick some up). What number does it show now?</p>
                <div className="flex items-center gap-2">
                  <input
                    className={`${input} w-40 font-mono`}
                    placeholder="new number"
                    aria-label="New number shown in the game"
                    inputMode="decimal"
                    value={number}
                    onChange={(e) => setNumber(e.target.value)}
                    onKeyDown={(e) =>
                      e.key === "Enter" && number.trim() && void next({ kind: "number", value: number.trim() })
                    }
                  />
                  <button
                    className={primary}
                    disabled={!number.trim()}
                    onClick={() => void next({ kind: "number", value: number.trim() })}
                  >
                    Search
                  </button>
                </div>
              </div>
            ))}
          <div className="flex flex-wrap gap-2">
            {summary.count > 0 && summary.count <= TEST_ANYWAY_LIMIT && !(noNumber && summary.rounds === 1) && (
              <button className={secondary} onClick={() => void beginTest()}>
                Try these {summary.count} now
              </button>
            )}
            <button className={secondary} onClick={reset}>
              Start over
            </button>
          </div>
        </div>
      )}

      {phase.k === "test" && (
        <div className="flex flex-col gap-3 rounded-xl bg-zinc-900/80 p-4 ring-1 ring-zinc-800">
          <p className="text-xs text-zinc-500">
            Trying place {phase.i + 1} of {phase.list.length}
          </p>
          <p className="text-base">
            Look at your {label} in the game. Does it show <b className="font-mono">{phase.trying}</b> now?
          </p>
          <p className="text-xs text-zinc-400">
            Some games only update the number when something happens: open a menu or move a little, then look again.
            The real number is put back after you answer.
          </p>
          <div className="flex flex-wrap gap-2">
            <button className={primary} onClick={() => void answer(true)}>
              Yes
            </button>
            <button className={secondary} onClick={() => void answer(false)}>
              No
            </button>
            <button className={secondary} onClick={() => void answer(false)}>
              Not sure
            </button>
          </div>
        </div>
      )}

      {phase.k === "found" && (
        <div className="flex flex-col gap-3 rounded-xl bg-zinc-900/80 p-4 ring-1 ring-emerald-800/60">
          <p className="text-base font-semibold text-emerald-300">Found your {label}!</p>
          {restarted && (
            <button className={`${primary} self-start`} onClick={() => void finishPending()}>
              Finish saving {pending?.name}
            </button>
          )}
          <div className="flex flex-wrap items-center gap-2 text-sm">
            <select
              className={input}
              aria-label="What the cheat does"
              value={keep ? "keep" : "set"}
              onChange={(e) => setKeep(e.target.value === "keep")}
            >
              <option value="keep">Keep it at</option>
              <option value="set">Set it once to</option>
            </select>
            <input
              className={`${input} w-36 font-mono`}
              aria-label="Cheat value"
              inputMode="decimal"
              value={cheatValue}
              onChange={(e) => setCheatValue(e.target.value)}
            />
          </div>
          <div className="flex flex-wrap gap-2">
            <button className={primary} disabled={!cheatValue.trim()} onClick={() => void applyNow()}>
              Use it now
            </button>
            <button className={secondary} disabled={!cheatValue.trim()} onClick={() => void saveCheat()}>
              Save as a cheat (with a hotkey)
            </button>
            {notice?.startsWith("Saved") && (
              <button className={secondary} onClick={onShowTrainer}>
                Open the Trainer tab
              </button>
            )}
          </div>
          <div className="flex flex-wrap gap-3 text-xs text-zinc-400">
            {phase.rest.length > 0 && (
              <button className="underline hover:text-zinc-200" onClick={() => void tryAt(phase.rest, 0)}>
                That wasn't it (the number came back): try the next place
              </button>
            )}
            <button className="underline hover:text-zinc-200" onClick={reset}>
              Find something else
            </button>
          </div>
        </div>
      )}

      {notice && (
        <p className="rounded-lg bg-zinc-900 px-3 py-2 text-sm text-zinc-300 ring-1 ring-zinc-800" role="status">
          {notice}
        </p>
      )}
      {error && (
        <p className="text-sm text-red-400" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
