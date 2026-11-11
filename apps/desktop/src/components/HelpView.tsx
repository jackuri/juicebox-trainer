import { useCallback, useEffect, useState } from "react";
import { api, onProblem, type Problem } from "../lib/ipc";
import { ProblemCard } from "./ProblemCard";

/** Common problems and what to do, in plain words. */
const TIPS: { q: string; a: string }[] = [
  {
    q: "A game closed or crashed",
    a: "JuiceBox Trainer says why under Recent problems: whether it closed normally or crashed, where it crashed, and which cheats were on. If it crashed right after you turned a cheat on, try the game without that cheat.",
  },
  {
    q: "A web game froze or went blank",
    a: "Click Reload game on its cheats. Searching a big game's memory can make the page pause for a few seconds. Some web games also freeze when your internet drops.",
  },
  {
    q: "JuiceBox Trainer's screen went blank",
    a: "It restarts its screen by itself and notes it here. If it stays blank, close JuiceBox Trainer and open it again. Only one JuiceBox Trainer can be open at a time.",
  },
  {
    q: "A cheat stopped working after a game update",
    a: "Click Repair on the Cheats screen. If that doesn't fix it, ask the AI helper (Make a trainer) to make the cheat again.",
  },
  {
    q: "JuiceBox Trainer can't connect to my game",
    a: "Start the game first, then click Play. If Windows refuses, close JuiceBox Trainer and open it with right-click, \"Run as administrator\". Games with anti-cheat (online protection) are blocked on purpose.",
  },
  {
    q: "My internet stopped working",
    a: "JuiceBox Trainer never changes your network settings, and cheats only change numbers inside the game. Restart your router, or ask your internet provider if it stays down after restarting your PC.",
  },
];

/** Help: recent problems with what to try, and common questions (v1.22.0). */
export function HelpView({ onSeen }: { onSeen: () => void }) {
  const [problems, setProblems] = useState<Problem[] | null>(null);

  const load = useCallback(() => {
    api.problemsList().then(
      (list) => setProblems(Array.isArray(list) ? list : []),
      () => setProblems([]),
    );
  }, []);

  useEffect(() => {
    load();
    api.problemsSeen().then(onSeen, () => undefined);
    return onProblem(() => {
      load();
      api.problemsSeen().then(onSeen, () => undefined);
    });
  }, [load, onSeen]);

  async function clear() {
    await api.problemsClear().catch(() => undefined);
    load();
  }

  return (
    <main className="flex min-h-0 flex-col gap-5 overflow-y-auto p-6">
      <div>
        <h2 className="text-2xl font-semibold tracking-tight">Help</h2>
        <p className="mt-1 text-sm text-zinc-400">
          When something goes wrong, JuiceBox Trainer explains it here: what happened, why, and what to try. Nothing is sent
          anywhere.
        </p>
      </div>

      <section className="flex flex-col gap-2" aria-label="Recent problems">
        <div className="flex items-center gap-2">
          <h3 className="text-base font-medium">Recent problems</h3>
          {problems && problems.length > 0 && (
            <button className="ml-auto rounded-lg bg-zinc-800 px-2.5 py-1 text-xs ring-1 ring-zinc-700 hover:bg-zinc-700" onClick={() => void clear()}>
              Clear the list
            </button>
          )}
        </div>
        {problems === null ? (
          <p className="text-sm text-zinc-400">Loading…</p>
        ) : problems.length === 0 ? (
          <p className="text-sm text-zinc-400">No problems so far.</p>
        ) : (
          problems.map((p) => <ProblemCard key={p.id} problem={p} showTime />)
        )}
      </section>

      <section className="flex flex-col gap-2" aria-label="Common questions">
        <h3 className="text-base font-medium">Common questions</h3>
        {TIPS.map((t) => (
          <details key={t.q} className="rounded-lg bg-zinc-900/80 px-3 py-2 text-sm ring-1 ring-zinc-800">
            <summary className="cursor-pointer select-none font-medium text-zinc-100">{t.q}</summary>
            <p className="mt-1 text-zinc-300">{t.a}</p>
          </details>
        ))}
      </section>
    </main>
  );
}
