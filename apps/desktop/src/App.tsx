import { useCallback, useEffect, useState } from "react";
import { AboutView } from "./components/AboutView";
import { ThemeView } from "./components/ThemeView";
import { WebGamesView } from "./components/WebGamesView";
import { AiView } from "./components/AiView";
import { GuardVerdict } from "./components/GuardVerdict";
import { HelpView } from "./components/HelpView";
import { friendlyError } from "./lib/explain";
import { ProblemCard } from "./components/ProblemCard";
import { Icon, Logo } from "./components/Icon";
import { useSeason } from "./lib/mascot";
import { LibraryView } from "./components/LibraryView";
import { ErrorBoundary } from "./components/ErrorBoundary";
import type { AiFix } from "./lib/cheatBook";
import { type OpenFile, PacksView } from "./components/PacksView";
import { ProcessList } from "./components/ProcessList";
import { SavesView } from "./components/SavesView";
import { SessionTools } from "./components/SessionTools";
import { PackerBadge } from "./components/PackerLevel";
import type { LiveAsk } from "./components/TrainerPanel";
import {
  api,
  type AppInfo,
  type Credit,
  type CommandError,
  isPackFile,
  onFileDrop,
  onProblem,
  onOtpackOpen,
  onUpdateAvailable,
  type Problem,
  type ProcessInfo,
  type Recovery,
  type RunningGame,
  type SessionInfo,
  toCommandError,
  type UpdateInfo,
  type Verdict,
} from "./lib/ipc";

type View = "games" | "web" | "cheats" | "packs" | "saves" | "ai" | "themes" | "help" | "about";

export default function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [processes, setProcesses] = useState<ProcessInfo[]>([]);
  const [games, setGames] = useState<RunningGame[]>([]);
  const [loadingList, setLoadingList] = useState(false);
  const [selected, setSelected] = useState<ProcessInfo | null>(null);
  const [verdict, setVerdict] = useState<Verdict | null>(null);
  const [checking, setChecking] = useState(false);
  const [session, setSession] = useState<SessionInfo | null>(null);
  const [error, setError] = useState<CommandError | null>(null);
  const [recovered, setRecovered] = useState<Recovery[]>([]);
  const [view, setView] = useState<View>("cheats");
  /** The AI helper opens on "a web game" (from a web game's cheats). */
  const [aiWeb, setAiWeb] = useState(false);
  /** A pack file to show in Packs (opened from Explorer, dropped, or a downloaded table). */
  const [openPath, setOpenPath] = useState<OpenFile | null>(null);
  /** A pack to open in the cheat book, and a cheat to fix with AI (v1.28.0). */
  const [openBook, setOpenBook] = useState<string | null>(null);
  const [aiFix, setAiFix] = useState<AiFix | null>(null);
  const showBook = useCallback((key: string) => {
    setOpenBook(key);
    setView("packs");
  }, []);
  const bookOpened = useCallback(() => setOpenBook(null), []);
  /** A cheat to fix on the spot, in Cheats, while the game runs with its pack loaded (v1.30.0). */
  const [liveAsk, setLiveAsk] = useState<LiveAsk | null>(null);
  const liveAskUsed = useCallback(() => setLiveAsk(null), []);
  /** "Fix it with AI" from the cheat book: on the spot when that pack is in use, else the AI page. */
  async function askAi(f: AiFix) {
    const loaded = await api.trainerState().catch(() => null);
    if (f.id && f.key && loaded?.key === f.key) {
      setLiveAsk({ fix: f.id, problem: f.problem });
      setView("cheats");
      return;
    }
    setAiFix(f);
    setView("ai");
  }
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  // The taskbar's juice box follows the season too (v1.24.0).
  const season = useSeason();
  useEffect(() => {
    api.appIconSet(season).catch(() => undefined);
  }, [season]);
  /** The one-time "OpenTrainer is now JuiceBox Trainer" note is hidden. */
  const [renameSeen, setRenameSeen] = useState(false);
  const [dropError, setDropError] = useState<string | null>(null);
  /** Why the attached game closed (shown on the Cheats screen, v1.22.0). */
  const [closed, setClosed] = useState<Problem | null>(null);
  /** The newest problem not looked at yet (the banner), and whether any are unseen. */
  const [notice, setNotice] = useState<Problem | null>(null);
  const [unseen, setUnseen] = useState(false);
  const problemsSeen = useCallback(() => {
    setUnseen(false);
    setNotice(null);
  }, []);

  const openOtpack = useCallback((path: string, exe?: string, credit?: Credit) => {
    setOpenPath({ path, exe, credit });
    setView("packs");
  }, []);
  const opened = useCallback(() => setOpenPath(null), []);

  useEffect(() => {
    api.packsPendingOpen().then((p) => p && openOtpack(p), () => undefined);
    const stopOpen = onOtpackOpen(openOtpack);
    const stopUpdate = onUpdateAvailable(setUpdate);
    const stopDrop = onFileDrop((paths) => {
      const pack = paths.find(isPackFile);
      if (pack) {
        setDropError(null);
        openOtpack(pack);
      } else if (paths.length > 0) {
        setDropError("Only pack files (.juice or pack.json) and Cheat Engine tables (.CT) can be dropped here.");
      }
    });
    // Problems: a game closing, a page crashing, JuiceBox Trainer's screen restarting.
    const stopProblem = onProblem((p) => {
      if (p.kind === "gameClosed") {
        setSession(null);
        setClosed(p);
      }
      if (!p.seen) {
        setUnseen(true);
        setNotice(p);
      }
    });
    // Problems from last time (JuiceBox Trainer closed after a bug, its screen restarted).
    api.problemsList().then(
      (list) => {
        const fresh = (Array.isArray(list) ? list : []).filter((p) => !p.seen);
        if (fresh.length > 0) {
          setUnseen(true);
          setNotice(fresh[0] ?? null);
        }
      },
      () => undefined,
    );
    return () => {
      stopOpen();
      stopUpdate();
      stopDrop();
      stopProblem();
    };
  }, [openOtpack]);

  const refresh = useCallback(async () => {
    setLoadingList(true);
    try {
      const [list, running] = await Promise.all([api.listProcesses(), api.runningGames().catch(() => [])]);
      setProcesses(list);
      setGames(running);
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
    api.runningGames().then(setGames, () => undefined);
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
    setClosed(null);
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
    if (err.closed) {
      setClosed(err.closed);
      setError(null);
    } else {
      setError(err);
    }
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
    <div className="grid h-full grid-cols-[216px_1fr]">
      <aside className="flex min-h-0 flex-col border-r border-zinc-800/70 bg-ink/70 px-3 pb-3 pt-4">
        <div className="flex items-center gap-2.5 px-2">
          <Logo className="h-8 w-8 shrink-0" />
          <div className="min-w-0">
            <h1 className="text-[15px] font-semibold leading-tight tracking-tight">JuiceBox Trainer</h1>
            {info && (
              <span className="text-[11px] text-zinc-500" data-testid="version">
                v{info.version}
              </span>
            )}
          </div>
        </div>
        <nav className="mt-6 flex flex-col gap-0.5" aria-label="Views">
          {(
            [
              ["games", "Games", "games"],
              ["web", "Web games", "web"],
              ["cheats", "Cheats", "cheats"],
              ["packs", "Packs", "packs"],
              ["saves", "Save backups", "saves"],
              ["ai", "Make a trainer (AI)", "ai"],
              ["themes", "Themes", "themes"],
              ["help", "Help", "help"],
              ["about", "About", "about"],
            ] as const
          ).map(([v, label, icon]) => (
            <button
              key={v}
              aria-pressed={view === v}
              onClick={() => {
                setView(v);
                setAiWeb(false);
                setAiFix(null);
              }}
              className={`group relative flex items-center gap-3 rounded-lg px-3 py-2 text-left text-sm font-medium transition-colors ${
                view === v
                  ? "bg-zinc-800/80 text-bright"
                  : "text-zinc-400 hover:bg-zinc-900 hover:text-zinc-100"
              }`}
            >
              {view === v && <span className="absolute inset-y-2 left-0 w-[3px] rounded-full bg-emerald-400" />}
              <Icon
                name={icon}
                className={`h-[18px] w-[18px] shrink-0 ${view === v ? "text-emerald-300" : "text-zinc-500 group-hover:text-zinc-300"}`}
              />
              {label}
              {v === "help" && unseen && (
                <span className="ml-auto h-2 w-2 rounded-full bg-red-400" aria-label="New problems" />
              )}
            </button>
          ))}
        </nav>
        <div className="mt-auto">
          <PackerBadge />
        </div>
        <div className="rounded-xl bg-zinc-900/80 px-3 py-2.5 text-xs ring-1 ring-zinc-800">
          {session ? (
            <div className="flex items-start gap-2">
              <span className="mt-1 h-2 w-2 shrink-0 rounded-full bg-green-400 shadow-[0_0_8px] shadow-green-400/70" />
              <span className="min-w-0 break-words text-zinc-200">
                Attached to {session.process.name} ({session.process.pid})
              </span>
            </div>
          ) : (
            <div className="flex items-center gap-2 text-zinc-500">
              <span className="h-2 w-2 rounded-full bg-zinc-600" />
              Not attached
            </div>
          )}
        </div>
      </aside>

      <div className="grid min-h-0 grid-rows-[auto_1fr]">
      <div>
        {info?.movedFromOpenTrainer && !renameSeen && (
          <div className="flex items-center gap-3 bg-emerald-950/60 px-4 py-1.5 text-xs text-emerald-100" role="status">
            <span className="flex-1">
              OpenTrainer is now JuiceBox Trainer. Your packs, cheats, save backups and settings came along.
            </span>
            <button className="text-emerald-300 hover:text-emerald-100" aria-label="Hide" onClick={() => setRenameSeen(true)}>
              ✕
            </button>
          </div>
        )}
        {update && view !== "about" && (
          <div className="flex items-center gap-3 bg-sky-950/60 px-4 py-1.5 text-xs text-sky-100" role="status">
            <span className="flex-1">JuiceBox Trainer {update.version} is available.</span>
            <button className="rounded bg-sky-900/70 px-2 py-0.5 hover:bg-sky-800" onClick={() => setView("about")}>
              See the update
            </button>
            <button className="text-sky-300 hover:text-sky-100" aria-label="Hide" onClick={() => setUpdate(null)}>
              ✕
            </button>
          </div>
        )}
        {notice && view !== "help" && !(notice.kind === "gameClosed" && view === "cheats") && (
          <div className="flex items-center gap-3 bg-red-950/60 px-4 py-1.5 text-xs text-red-100" role="alert">
            <span className="flex-1">{notice.title}.</span>
            <button className="rounded bg-red-900/70 px-2 py-0.5 hover:bg-red-800" onClick={() => setView("help")}>
              See what happened
            </button>
            <button className="text-red-300 hover:text-red-100" aria-label="Hide" onClick={() => setNotice(null)}>
              ✕
            </button>
          </div>
        )}
        {dropError && (
          <div className="flex items-center gap-3 bg-amber-950/60 px-4 py-1.5 text-xs text-amber-100" role="alert">
            <span className="flex-1">{dropError}</span>
            <button className="text-amber-300 hover:text-amber-100" aria-label="Hide" onClick={() => setDropError(null)}>
              ✕
            </button>
          </div>
        )}
      </div>
      <ErrorBoundary key={view}>
      {view === "games" ? (
        <LibraryView
          onOpenAi={() => setView("ai")}
          onOpenTable={openOtpack}
          onOpenBook={showBook}
          onPlayed={(r) => {
            setError(null);
            setClosed(null);
            setVerdict(null);
            setSelected(r.session.process);
            setSession(r.session);
            setView("cheats");
            void refresh();
          }}
        />
      ) : view === "packs" ? (
        <PacksView
          openPath={openPath}
          onOpened={opened}
          onShowCheats={() => setView("cheats")}
          onOpenAi={() => setView("ai")}
          openBook={openBook}
          onBookOpened={bookOpened}
          onAskAi={(f) => void askAi(f)}
        />
      ) : view === "about" ? (
        <AboutView />
      ) : view === "themes" ? (
        <ThemeView />
      ) : view === "help" ? (
        <HelpView onSeen={problemsSeen} />
      ) : view === "web" ? (
        <WebGamesView
          onOpenAi={() => {
            setAiWeb(true);
            setView("ai");
          }}
        />
      ) : view === "ai" ? (
        <AiView key={aiFix ? `fix:${aiFix.cheat}` : "ai"} processes={processes} games={games} onRefreshProcesses={() => void refresh()} webFirst={aiWeb} fix={aiFix} />
      ) : view === "saves" ? (
        <SavesView
          processes={processes}
          games={games}
          onRefreshProcesses={() => void refresh()}
          onOpenAi={() => setView("ai")}
        />
      ) : (
        <div className="grid min-h-0 grid-cols-[272px_1fr]">
          <ProcessList
            processes={processes}
            games={games}
            onShowGames={() => setView("games")}
            selectedPid={session?.process.pid ?? selected?.pid ?? null}
            loading={loadingList}
            onSelect={(p) => void select(p)}
            onRefresh={() => void refresh()}
          />

          <main className="flex min-h-0 flex-col gap-4 overflow-y-auto p-4">
            {recovered.length > 0 && (
              <div className="flex items-start gap-3 rounded-lg bg-amber-950/40 px-3 py-2 text-sm text-amber-200" role="status">
                <div className="flex-1">
                  <p>JuiceBox Trainer closed unexpectedly last time. It put the games' original code back:</p>
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
            {closed && !session && (
              <ProblemCard problem={closed} onDismiss={() => setClosed(null)} />
            )}
            {error && !error.verdict && (
              <p className="rounded-lg bg-red-950/40 px-3 py-2 text-sm text-red-300" role="alert">
                {friendlyError(error)}
              </p>
            )}

            {session ? (
              <>
                <div className="relative flex shrink-0 items-center gap-4 overflow-hidden rounded-2xl bg-gradient-to-br from-emerald-900/60 via-zinc-900 to-zinc-900 px-5 py-4 ring-1 ring-zinc-800">
                  <div className="pointer-events-none absolute -right-16 -top-20 h-56 w-56 rounded-full bg-emerald-500/20 blur-3xl" />
                  <div className="relative min-w-0">
                    <p className="flex items-center gap-2 text-[11px] font-semibold uppercase tracking-[0.14em] text-emerald-300">
                      <span className="h-1.5 w-1.5 rounded-full bg-green-400" /> Attached
                    </p>
                    <p className="mt-0.5 truncate text-2xl font-semibold tracking-tight">{session.process.name}</p>
                    <p className="truncate text-xs text-zinc-400">
                      PID {session.process.pid} · {session.process.arch} ·{" "}
                      {session.process.exePath ?? "path unknown"}
                    </p>
                  </div>
                  <button
                    className="relative ml-auto rounded-lg bg-zinc-800/90 px-4 py-2 text-sm font-medium ring-1 ring-zinc-700 hover:bg-zinc-700"
                    onClick={() => void detach()}
                  >
                    Detach
                  </button>
                </div>
                <SessionTools
                  session={session}
                  gameName={
                    games.find((g) => g.pid === session.process.pid)?.name ??
                    session.process.name.replace(/\.exe$/i, "")
                  }
                  onShowPacks={() => setView("packs")}
                  onOpenBook={showBook}
                  liveAsk={liveAsk}
                  onLiveAskUsed={liveAskUsed}
                  onOpenAi={() => setView("ai")}
                  onSessionLost={sessionLost}
                  onRefreshModules={() => void refreshModules()}
                />
              </>
            ) : selected ? (
              <div className="flex max-w-xl flex-col gap-4 rounded-2xl bg-zinc-900/80 p-5 ring-1 ring-zinc-800">
                <div className="min-w-0">
                  <p className="truncate text-xl font-semibold tracking-tight">{selected.name}</p>
                  <p className="truncate text-xs text-zinc-500">
                    PID {selected.pid} · {selected.arch} · {selected.exePath ?? "path unknown"}
                  </p>
                </div>
                {checking && <p className="text-sm text-zinc-400">Checking for anti-cheat…</p>}
                {verdict && verdict.pid === selected.pid && <GuardVerdict verdict={verdict} />}
                <button
                  className="btn-accent self-start rounded-xl px-8 py-2.5 text-sm font-semibold disabled:cursor-not-allowed disabled:opacity-40"
                  disabled={!verdict?.allowed || verdict.pid !== selected.pid || checking}
                  onClick={() => void attach()}
                >
                  Attach
                </button>
              </div>
            ) : (
              <div className="m-auto flex max-w-sm flex-col items-center text-center text-sm text-zinc-400">
                <Logo className="mb-4 h-14 w-14 opacity-90" />
                <p className="text-base font-medium text-zinc-200">Pick a game to start</p>
                <p className="mt-1">Choose a game from the list. JuiceBox Trainer checks it, then you attach.</p>
                <p className="mt-2 text-xs text-zinc-500">
                  Your game&apos;s saves are backed up when you attach (see Save backups).
                </p>
              </div>
            )}
          </main>
        </div>
      )}
      </ErrorBoundary>
      </div>
    </div>
  );
}
