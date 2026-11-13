import { useCallback, useEffect, useMemo, useState } from "react";
import { findQuestions, type GuideView, LESSONS, parts, type Topic, TOPICS } from "../lib/guide";
import { api, onProblem, type Problem } from "../lib/ipc";
import { ProblemCard } from "./ProblemCard";

export type HelpTab = "tutorial" | "questions" | "problems";

/** Lessons the player marked done (v1.34.0), kept on this PC. */
const DONE_KEY = "juicebox.tutorial";

function loadDone(): string[] {
  try {
    const list: unknown = JSON.parse(localStorage.getItem(DONE_KEY) ?? "[]");
    return Array.isArray(list) ? list.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

function saveDone(list: string[]) {
  try {
    localStorage.setItem(DONE_KEY, JSON.stringify(list));
  } catch {
    // Not remembered; nothing else depends on it.
  }
}

/** Guide text with its **buttons** in bold and `keys` in code style. */
function Rich({ text }: { text: string }) {
  return (
    <>
      {parts(text).map((p, i) =>
        p.kind === "bold" ? (
          <b key={i} className="font-semibold text-zinc-100">
            {p.text}
          </b>
        ) : p.kind === "code" ? (
          <code key={i} className="rounded bg-zinc-800 px-1 py-px text-[0.9em] text-zinc-200">
            {p.text}
          </code>
        ) : (
          <span key={i}>{p.text}</span>
        ),
      )}
    </>
  );
}

/** The tutorial: lessons to open one at a time, with "Show me" and done ticks. */
function Tutorial({ onGo }: { onGo?: (view: GuideView) => void }) {
  const [done, setDone] = useState(loadDone);
  const [open, setOpen] = useState<string | null>(() => LESSONS.find((l) => !loadDone().includes(l.id))?.id ?? null);
  const count = LESSONS.filter((l) => done.includes(l.id)).length;

  function mark(id: string, on: boolean) {
    const list = on ? [...new Set([...done, id])] : done.filter((x) => x !== id);
    setDone(list);
    saveDone(list);
  }

  return (
    <section className="flex flex-col gap-3" aria-label="Tutorial">
      <div className="flex flex-col gap-1.5">
        <p className="text-sm text-zinc-300">
          New here? These short lessons show how to use cheats and how to make your own, by hand or with AI. Do them in any
          order: <b className="font-semibold text-zinc-100">Show me</b> opens the screen a lesson is about.
        </p>
        <div className="flex items-center gap-2 text-xs text-zinc-400">
          <div
            className="h-2 w-48 overflow-hidden rounded-full bg-zinc-800"
            role="progressbar"
            aria-label="Lessons done"
            aria-valuemin={0}
            aria-valuemax={LESSONS.length}
            aria-valuenow={count}
          >
            <div className="h-full rounded-full bg-emerald-400 transition-all" style={{ width: `${(count / LESSONS.length) * 100}%` }} />
          </div>
          {count === LESSONS.length ? "All done. Nice!" : `${count} of ${LESSONS.length} lessons done`}
        </div>
      </div>

      <ol className="flex flex-col gap-2">
        {LESSONS.map((l, i) => {
          const isOpen = open === l.id;
          const isDone = done.includes(l.id);
          const next = LESSONS[i + 1];
          return (
            <li key={l.id} className={`rounded-xl ring-1 ${isOpen ? "bg-zinc-900 ring-emerald-800/70" : "bg-zinc-900/70 ring-zinc-800"}`}>
              <button
                className="flex w-full items-center gap-3 px-3 py-2.5 text-left"
                aria-expanded={isOpen}
                aria-label={`${i + 1}. ${l.title}${isDone ? " (done)" : ""}: ${l.summary}`}
                onClick={() => setOpen(isOpen ? null : l.id)}
              >
                <span
                  className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-xs font-bold ${
                    isDone ? "bg-emerald-500 text-zinc-950" : "bg-zinc-800 text-zinc-300 ring-1 ring-zinc-700"
                  }`}
                  aria-hidden="true"
                >
                  {isDone ? "✓" : i + 1}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block text-sm font-medium text-zinc-100">
                    {l.title}
                  </span>
                  <span className="block text-xs text-zinc-400">{l.summary}</span>
                </span>
                <span className="shrink-0 text-xs text-zinc-500">{l.minutes} min</span>
              </button>
              {isOpen && (
                <div className="flex flex-col gap-3 border-t border-zinc-800 px-4 pb-4 pt-3 text-sm" role="region" aria-label={l.title}>
                  <ol className="flex list-decimal flex-col gap-1.5 pl-5 text-zinc-300 marker:text-emerald-400">
                    {l.steps.map((s) => (
                      <li key={s}>
                        <Rich text={s} />
                      </li>
                    ))}
                  </ol>
                  {l.tip && (
                    <p className="rounded-lg bg-emerald-950/40 px-3 py-2 text-xs text-emerald-100 ring-1 ring-emerald-900/60">
                      <b className="font-semibold">Tip: </b>
                      <Rich text={l.tip} />
                    </p>
                  )}
                  <div className="flex flex-wrap gap-2">
                    {onGo && (
                      <button className="btn-accent rounded-lg px-3 py-1.5 text-sm font-medium" onClick={() => onGo(l.go.view)}>
                        Show me: {l.go.label}
                      </button>
                    )}
                    <button
                      className="rounded-lg bg-zinc-800 px-3 py-1.5 text-sm ring-1 ring-zinc-700 hover:bg-zinc-700"
                      onClick={() => {
                        mark(l.id, !isDone);
                        if (!isDone) setOpen(next?.id ?? null);
                      }}
                    >
                      {isDone ? "Not done yet" : next ? "Done: next lesson" : "Done"}
                    </button>
                  </div>
                </div>
              )}
            </li>
          );
        })}
      </ol>
    </section>
  );
}

/** The common questions, with a search box and topics. */
function Questions() {
  const [search, setSearch] = useState("");
  const [topic, setTopic] = useState<Topic | null>(null);
  const found = useMemo(() => findQuestions(search, topic), [search, topic]);

  return (
    <section className="flex flex-col gap-3" aria-label="Common questions">
      <input
        type="search"
        aria-label="Search the questions"
        placeholder="Search the questions, e.g. antivirus, flashlight, controller"
        className="w-full max-w-lg rounded-lg border border-zinc-700 bg-zinc-900 px-3 py-1.5 text-sm"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />
      <div className="flex flex-wrap gap-1.5" role="group" aria-label="Topics">
        {[null, ...TOPICS].map((t) => (
          <button
            key={t ?? "all"}
            aria-pressed={topic === t}
            className={`rounded-full px-2.5 py-0.5 text-xs ring-1 ${
              topic === t ? "bg-emerald-900/70 text-emerald-100 ring-emerald-700" : "bg-zinc-900 text-zinc-300 ring-zinc-700 hover:bg-zinc-800"
            }`}
            onClick={() => setTopic(t)}
          >
            {t ?? "All"}
          </button>
        ))}
      </div>
      {found.length === 0 ? (
        <p className="text-sm text-zinc-400">No question matches that. Try other words, or look through the tutorial.</p>
      ) : (
        TOPICS.filter((t) => found.some((x) => x.topic === t)).map((t) => (
          <div key={t} className="flex flex-col gap-1.5">
            <h4 className="text-xs font-semibold uppercase tracking-wider text-zinc-500">{t}</h4>
            {found
              .filter((x) => x.topic === t)
              .map((x) => (
                <details key={x.q} className="rounded-lg bg-zinc-900/80 px-3 py-2 text-sm ring-1 ring-zinc-800" open={search.trim() !== ""}>
                  <summary className="cursor-pointer select-none font-medium text-zinc-100">{x.q}</summary>
                  <p className="mt-1 text-zinc-300">
                    <Rich text={x.a} />
                  </p>
                </details>
              ))}
          </div>
        ))
      )}
    </section>
  );
}

/** Help: a tutorial, common questions, and recent problems with what to try (v1.22.0, v1.34.0). */
export function HelpView({
  onSeen,
  start = "tutorial",
  onGo,
}: {
  onSeen: () => void;
  /** The part to show first. */
  start?: HelpTab;
  /** Opens a screen of the app ("Show me" in a lesson). */
  onGo?: (view: GuideView) => void;
}) {
  const [tab, setTab] = useState<HelpTab>(start);
  const [problems, setProblems] = useState<Problem[] | null>(null);

  const load = useCallback(() => {
    api.problemsList().then(
      (list) => setProblems(Array.isArray(list) ? list : []),
      () => setProblems([]),
    );
  }, []);

  useEffect(() => {
    load();
    return onProblem(load);
  }, [load]);

  // Problems count as seen once the player looks at them.
  useEffect(() => {
    if (tab !== "problems" || problems === null) return;
    api.problemsSeen().then(onSeen, () => undefined);
  }, [tab, problems, onSeen]);

  async function clear() {
    await api.problemsClear().catch(() => undefined);
    load();
  }

  const unseen = problems?.filter((p) => !p.seen).length ?? 0;
  const tabs: [HelpTab, string][] = [
    ["tutorial", "Tutorial"],
    ["questions", "Common questions"],
    ["problems", "Recent problems"],
  ];

  return (
    <main className="flex min-h-0 flex-col gap-5 overflow-y-auto p-6">
      <div>
        <h2 className="text-2xl font-semibold tracking-tight">Help</h2>
        <p className="mt-1 text-sm text-zinc-400">
          Learn JuiceBox Trainer step by step, find answers, and see what went wrong lately and what to try. Nothing here is sent
          anywhere.
        </p>
      </div>

      <div className="flex gap-1 self-start rounded-xl bg-zinc-900/80 p-1 ring-1 ring-zinc-800" role="tablist" aria-label="Help">
        {tabs.map(([t, label]) => (
          <button
            key={t}
            role="tab"
            aria-selected={tab === t}
            className={`flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-medium ${
              tab === t ? "bg-zinc-800 text-bright ring-1 ring-zinc-700" : "text-zinc-400 hover:text-zinc-100"
            }`}
            onClick={() => setTab(t)}
          >
            {label}
            {t === "problems" && unseen > 0 && tab !== "problems" && (
              <span className="h-2 w-2 rounded-full bg-red-400" aria-label="New problems" />
            )}
          </button>
        ))}
      </div>

      {tab === "tutorial" ? (
        <Tutorial onGo={onGo} />
      ) : tab === "questions" ? (
        <Questions />
      ) : (
        <section className="flex flex-col gap-2" aria-label="Recent problems">
          <div className="flex items-center gap-2">
            <p className="text-sm text-zinc-400">When something goes wrong, JuiceBox Trainer explains it here: what happened, why, and what to try.</p>
            {problems && problems.length > 0 && (
              <button
                className="ml-auto shrink-0 rounded-lg bg-zinc-800 px-2.5 py-1 text-xs ring-1 ring-zinc-700 hover:bg-zinc-700"
                onClick={() => void clear()}
              >
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
      )}
    </main>
  );
}
