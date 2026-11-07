import { useCallback, useEffect, useState } from "react";
import { AiView } from "./components/AiView";
import { GuardVerdict } from "./components/GuardVerdict";
import { LibraryView } from "./components/LibraryView";
import { ProcessList } from "./components/ProcessList";
import { SavesView } from "./components/SavesView";
import { SessionTools } from "./components/SessionTools";
import {
  api,
  type AppInfo,
  type CommandError,
  type ProcessInfo,
  type Recovery,
  type SessionInfo,
  toCommandError,
  type Verdict,
} from "./lib/ipc";

export default function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [processes, setProcesses] = useState<ProcessInfo[]>([]);
  const [loadingList, setLoadingList] = useState(false);
  const [selected, setSelected] = useState<ProcessInfo | null>(null);
  const [verdict, setVerdict] = useState<Verdict | null>(null);
  const [checking, setChecking] = useState(false);
  const [session, setSession] = useState<SessionInfo | null>(null);
  const [error, setError] = useState<CommandError | null>(null);
  const [recovered, setRecovered] = useState<Recovery[]>([]);
  const [view, setView] = useState<"games" | "cheats" | "saves" | "ai">("cheats");

  const refresh = useCallback(async () => {
    setLoadingList(true);
    try {
      setProcesses(await api.listProcesses());
    } catch (e) {
      setError(toCommandError(e));
    } finally {
      setLoadingList(false);
    }
  }, []);

  useEffect(() => {
    api.appInfo().then(setInfo, (e: unknown) => setError(toCommandError(e)));
    api.currentSession().then(setSession, () => undefined);
    api.recoveryReport().then(setRecovered, () => undefined);
    api.listProcesses().then(setProcesses, (e: unknown) => setError(toCommandError(e)));
  }, []);

  async function select(p: ProcessInfo) {
    setSelected(p);
    setVerdict(null);
    setError(null);
    setChecking(true);
    try {
      setVerdict(await api.checkGuard(p.pid));
    } catch (e) {
      setError(toCommandError(e));
    } finally {
      setChecking(false);
    }
  }

  async function attach() {
    if (!selected) return;
    setError(null);
    try {
      setSession(await api.attach(selected.pid));
    } catch (e) {
      const err = toCommandError(e);
      if (err.verdict) setVerdict(err.verdict);
      setError(err);
    }
  }

  async function detach() {
    await api.detach().catch(() => undefined);
    setSession(null);
  }

  const sessionLost = useCallback((err: CommandError) => {
    setSession(null);
    if (err.verdict) setVerdict(err.verdict);
    setError(err);
  }, []);

  async function refreshModules() {
    try {
      const modules = await api.refreshModules();
      setSession((s) => (s ? { ...s, modules } : s));
    } catch (e) {
      const err = toCommandError(e);
      if (err.kind === "blocked" || err.kind === "processExited") sessionLost(err);
      else setError(err);
    }
  }

  return (
    <div className="grid h-full grid-rows-[auto_1fr]">
      <header className="flex items-center gap-3 border-b border-zinc-800 px-4 py-2">
        <h1 className="text-lg font-semibold tracking-tight">OpenTrainer</h1>
        {info && (
          <span className="font-mono text-xs text-zinc-500" data-testid="version">
            v{info.version}
          </span>
        )}
        <nav className="ml-4 flex gap-1" aria-label="Views">
          {(
            [
              ["games", "Games"],
              ["cheats", "Cheats"],
              ["saves", "Save backups"],
              ["ai", "Make a trainer (AI)"],
            ] as const
          ).map(([v, label]) => (
            <button
              key={v}
              aria-pressed={view === v}
              onClick={() => setView(v)}
              className={`rounded px-3 py-1 text-sm ${
                view === v ? "bg-zinc-800 text-zinc-100" : "text-zinc-400 hover:text-zinc-200"
              }`}
            >
              {label}
            </button>
          ))}
        </nav>
        <span className="ml-auto text-xs text-zinc-400">
          {session ? (
            <span className="text-emerald-400">
              Attached to {session.process.name} ({session.process.pid})
            </span>
          ) : (
            "Not attached"
          )}
        </span>
      </header>

      {view === "games" ? (
        <LibraryView
          onOpenAi={() => setView("ai")}
          onPlayed={(r) => {
            setError(null);
            setVerdict(null);
            setSelected(r.session.process);
            setSession(r.session);
            setView("cheats");
          }}
        />
      ) : view === "ai" ? (
        <AiView processes={processes} onRefreshProcesses={() => void refresh()} />
      ) : view === "saves" ? (
        <SavesView
          processes={processes}
          onRefreshProcesses={() => void refresh()}
          onOpenAi={() => setView("ai")}
        />
      ) : (
        <div className="grid min-h-0 grid-cols-[320px_1fr]">
          <ProcessList
            processes={processes}
            selectedPid={session?.process.pid ?? selected?.pid ?? null}
            loading={loadingList}
            onSelect={(p) => void select(p)}
            onRefresh={() => void refresh()}
          />

          <main className="flex min-h-0 flex-col gap-4 overflow-y-auto p-4">
            {recovered.length > 0 && (
              <div className="flex items-start gap-3 rounded-md bg-amber-950/40 px-3 py-2 text-sm text-amber-200" role="status">
                <div className="flex-1">
                  <p>OpenTrainer closed unexpectedly last time. It put the games' original code back:</p>
                  <ul className="mt-1 list-disc pl-5 text-xs">
                    {recovered.map((r) => (
                      <li key={`${r.pid}-${r.exe}`}>
                        {r.exe} (PID {r.pid}): {r.restored} change{r.restored === 1 ? "" : "s"} undone
                        {r.problems.length > 0 && ` — not undone: ${r.problems.join("; ")}`}
                      </li>
                    ))}
                  </ul>
                </div>
                <button
                  className="rounded bg-amber-900/60 px-2 py-0.5 text-xs hover:bg-amber-800/60"
                  onClick={() => setRecovered([])}
                >
                  OK
                </button>
              </div>
            )}
            {error && !error.verdict && (
              <p className="rounded-md bg-red-950/40 px-3 py-2 text-sm text-red-300" role="alert">
                {error.message}
              </p>
            )}

            {session ? (
              <>
                <div className="flex items-center gap-3">
                  <div className="min-w-0">
                    <p className="font-medium">{session.process.name}</p>
                    <p className="truncate text-xs text-zinc-500">
                      PID {session.process.pid} · {session.process.arch} ·{" "}
                      {session.process.exePath ?? "path unknown"}
                    </p>
                  </div>
                  <button
                    className="ml-auto rounded-md bg-zinc-800 px-3 py-1.5 text-sm hover:bg-zinc-700"
                    onClick={() => void detach()}
                  >
                    Detach
                  </button>
                </div>
                <SessionTools
                  session={session}
                  onSessionLost={sessionLost}
                  onRefreshModules={() => void refreshModules()}
                />
              </>
            ) : selected ? (
              <div className="flex max-w-xl flex-col gap-3">
                <div>
                  <p className="font-medium">{selected.name}</p>
                  <p className="truncate text-xs text-zinc-500">
                    PID {selected.pid} · {selected.arch} · {selected.exePath ?? "path unknown"}
                  </p>
                </div>
                {checking && <p className="text-sm text-zinc-400">Checking for anti-cheat…</p>}
                {verdict && verdict.pid === selected.pid && <GuardVerdict verdict={verdict} />}
                <button
                  className="self-start rounded-md bg-emerald-700 px-4 py-2 text-sm font-medium hover:bg-emerald-600 disabled:cursor-not-allowed disabled:opacity-40"
                  disabled={!verdict?.allowed || verdict.pid !== selected.pid || checking}
                  onClick={() => void attach()}
                >
                  Attach
                </button>
              </div>
            ) : (
              <div className="m-auto max-w-sm text-center text-sm text-zinc-400">
                <p>Pick a single-player game from the list to check it and attach.</p>
                <p className="mt-2 text-xs text-zinc-500">
                  Your game&apos;s saves are backed up when you attach (see Save backups).
                </p>
              </div>
            )}
          </main>
        </div>
      )}
    </div>
  );
}
