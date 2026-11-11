import { useEffect, useState } from "react";
import { pendingAsk } from "../lib/ai";
import {
  type AiEvent,
  type AiStatus,
  api,
  onAiEvent,
  onAiInstallProgress,
  type ProcessInfo,
  type RunningGame,
  type WebGame,
} from "../lib/ipc";
import { friendlyError } from "../lib/explain";
import { GamePicker } from "./GamePicker";

const WISHES = [
  "Infinite health",
  "Infinite ammo",
  "Lots of money / gold",
  "Infinite stamina",
  "Lots of XP",
  "Faster movement",
  "Freeze the timer",
];

const button = "rounded bg-zinc-800 px-3 py-1.5 text-sm hover:bg-zinc-700 disabled:opacity-40";
const big =
  "rounded-lg bg-emerald-700 px-5 py-2 text-base font-medium hover:bg-emerald-600 disabled:opacity-40";

interface Props {
  processes: ProcessInfo[];
  games: RunningGame[];
  onRefreshProcesses: () => void;
  /** Start on "a web game" (opened from a web game's cheats). */
  webFirst?: boolean;
}

type Backend = "subscription" | "apiKey" | "local";

interface Settings {
  backend: Backend;
  budget: string;
  localUrl: string;
  model: string;
  analysisCommand: string;
  analysisArgs: string;
  /** The helper may look at the game window (v1.21.0). */
  pictures: boolean;
  /** Claude backends: the best model, or a cheaper and faster one. */
  quality: "best" | "cheaper";
}

/** The cheaper Claude model (the best one is the helper's default). */
export const CHEAPER_MODEL = "claude-sonnet-5-5";

const DEFAULTS: Settings = {
  backend: "subscription",
  budget: "5",
  localUrl: "http://localhost:11434",
  model: "",
  analysisCommand: "",
  analysisArgs: "",
  pictures: true,
  quality: "best",
};

const SETTINGS_KEY = "opentrainer.ai.settings";

function loadSettings(): Settings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    return raw ? { ...DEFAULTS, ...(JSON.parse(raw) as Partial<Settings>) } : DEFAULTS;
  } catch {
    return DEFAULTS;
  }
}

function saveSettings(s: Settings) {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(s));
  } catch {
    // Settings are a convenience; the helper works without them.
  }
}

/** "Make a trainer": the AI helper finds cheats with the player's help. */
export function AiView({ processes, games, onRefreshProcesses, webFirst = false }: Props) {
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [events, setEvents] = useState<AiEvent[]>([]);
  const [pid, setPid] = useState("");
  const [kind, setKind] = useState<"pc" | "web">(webFirst ? "web" : "pc");
  const [webGame, setWebGame] = useState<WebGame | null>(null);
  const [webTask, setWebTask] = useState(webFirst);
  const [wishes, setWishes] = useState<string[]>(["Infinite health", "Lots of money / gold"]);
  const [other, setOther] = useState("");
  const [settings, setSettings] = useState<Settings>(loadSettings);
  const { backend, budget } = settings;
  const update = (patch: Partial<Settings>) =>
    setSettings((s) => {
      const next = { ...s, ...patch };
      saveSettings(next);
      return next;
    });
  const setBackend = (b: Backend) => update({ backend: b });
  const setBudget = (v: string) => update({ budget: v });
  const [reply, setReply] = useState("");
  const [note, setNote] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);

  useEffect(() => {
    api.aiStatus().then(
      (s) => {
        setStatus(s);
        setEvents(s.transcript);
      },
      (e: unknown) => setError(friendlyError(e)),
    );
    return onAiEvent((e) => {
      setEvents((list) => [...list, e]);
      if (e.type === "exit") {
        api.aiStatus().then(setStatus, () => undefined);
      }
    });
  }, []);

  // Which web game is open in OpenTrainer's game window (checked while the form shows).
  const formShown = !(status?.running ?? false) && events.length === 0;
  useEffect(() => {
    if (kind !== "web" || !formShown) return;
    let live = true;
    const look = () =>
      api.webCurrent().then(
        (g) => live && setWebGame(g ?? null),
        () => live && setWebGame(null),
      );
    void look();
    const timer = window.setInterval(() => void look(), 2000);
    return () => {
      live = false;
      window.clearInterval(timer);
    };
  }, [kind, formShown]);

  const exited = events.some((e) => e.type === "exit");
  const running = (status?.running ?? false) && !exited;
  const ask = pendingAsk(events);
  const cost = [...events].reverse().find((e) => e.type === "cost");
  const finished = [...events].reverse().find((e) => e.type === "finished");

  async function start() {
    setError(null);
    setStarting(true);
    try {
      const list = [...wishes, ...other.split(/[,\n]/).map((s) => s.trim()).filter(Boolean)];
      setEvents([]);
      const web = kind === "web";
      setWebTask(web);
      await api.aiStart({
        pid: web ? 0 : Number(pid),
        task: web ? "webTrainer" : undefined,
        webGame: web ? webGame?.id : undefined,
        wishes: list,
        notes: "",
        backend,
        maxBudgetUsd: Number(budget) || 5,
        localUrl: backend === "local" ? settings.localUrl : undefined,
        model: backend === "local" ? settings.model : settings.quality === "cheaper" ? CHEAPER_MODEL : undefined,
        pictures: settings.pictures ? undefined : false,
        analysisCommand: web ? undefined : settings.analysisCommand.trim() || undefined,
        analysisArgs: settings.analysisArgs.split(" ").filter(Boolean),
      });
      setStatus((s) => (s ? { ...s, running: true } : s));
    } catch (e) {
      setError(friendlyError(e));
    } finally {
      setStarting(false);
    }
  }

  async function signIn() {
    setError(null);
    try {
      await api.aiSignIn();
    } catch (e) {
      setError(friendlyError(e));
    }
  }

  async function recheck() {
    try {
      const s = await api.aiStatus();
      setStatus((old) => (old ? { ...old, signedIn: s.signedIn } : s));
    } catch (e) {
      setError(friendlyError(e));
    }
  }

  async function sendNote(text: string) {
    try {
      await api.aiNote(text);
      setEvents((list) => [...list, { type: "note", text }]);
      setNote("");
    } catch (e) {
      setError(friendlyError(e));
    }
  }

  async function answer(text: string) {
    if (!ask) return;
    try {
      await api.aiAnswer(ask.id, text);
      setEvents((list) => [...list, { type: "answered", id: ask.id, text }]);
      setReply("");
    } catch (e) {
      setError(friendlyError(e));
    }
  }

  if (status && !status.available) {
    return <AiSetup status={status} onReady={setStatus} />;
  }

  const showForm = !running && events.length === 0;

  return (
    <div className="flex min-h-0 flex-col gap-4 overflow-y-auto p-4">
      <div className="flex items-center gap-3">
        <h2 className="text-2xl font-semibold tracking-tight">Make a trainer with AI</h2>
        {cost?.type === "cost" && (
          <span className="text-xs text-zinc-500">Cost so far: about ${cost.usd.toFixed(2)}</span>
        )}
        {running && (
          <button className={`${button} ml-auto`} onClick={() => void api.aiCancel()}>
            Cancel
          </button>
        )}
        {!running && events.length > 0 && (
          <button className={`${button} ml-auto`} onClick={() => setEvents([])}>
            Start over
          </button>
        )}
      </div>
      {error && (
        <p className="rounded-lg bg-red-950/40 px-3 py-2 text-sm text-red-300" role="alert">
          {error}
        </p>
      )}

      {showForm && (
        <div className="flex max-w-2xl flex-col gap-4">
          {kind === "web" ? (
            <p className="text-sm text-zinc-400">
              Tick what you'd like. The helper looks in the open web game, asks you to do small things
              in it (like "spend some gold") when it needs to, then adds the cheats that work to the
              game's cheats in Web games.
            </p>
          ) : (
            <p className="text-sm text-zinc-400">
              Start your game, pick it here and tick what you'd like. The helper asks you to do small
              things in the game (like "spend some gold") while it searches, then saves a cheat pack.
              Single-player games only: games with anti-cheat are refused.
            </p>
          )}
          <fieldset className="flex flex-col gap-1 text-sm">
            <legend className="mb-1">What kind of game?</legend>
            <label className="flex items-center gap-2">
              <input type="radio" checked={kind === "pc"} onChange={() => setKind("pc")} />
              A game on this PC
            </label>
            <label className="flex items-center gap-2">
              <input type="radio" checked={kind === "web"} onChange={() => setKind("web")} />
              A web game (open in OpenTrainer&apos;s game window)
            </label>
          </fieldset>
          {kind === "web" ? (
            <div className="flex flex-col gap-1 rounded-lg bg-zinc-900 px-3 py-2 text-sm" role="note" aria-label="Web game">
              {webGame ? (
                <p>
                  The helper works on <b>{webGame.name}</b>, the game open in OpenTrainer&apos;s game window. Keep
                  that window open while it works.
                </p>
              ) : (
                <p className="text-amber-200">
                  Open the game first: in <b>Web games</b>, click <b>Play</b> next to it. Then come back here.
                </p>
              )}
              <p className="text-xs text-zinc-500">
                For playing alone or with friends you invited. Not for competitive games or games with
                items worth real money.
              </p>
            </div>
          ) : (
          <div className="flex flex-col gap-1 text-sm">
            Game
            <GamePicker
              label="Game"
              className="rounded border border-zinc-700 bg-zinc-900 px-2 py-1"
              processes={processes}
              games={games}
              value={pid}
              onChange={setPid}
              onRefresh={onRefreshProcesses}
            />
          </div>
          )}
          <fieldset className="flex flex-col gap-1 text-sm">
            <legend className="mb-1">Cheats you want</legend>
            {WISHES.map((w) => (
              <label key={w} className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={wishes.includes(w)}
                  onChange={(e) =>
                    setWishes((list) => (e.target.checked ? [...list, w] : list.filter((x) => x !== w)))
                  }
                />
                {w}
              </label>
            ))}
            <input
              aria-label="Other cheats"
              className="mt-1 rounded border border-zinc-700 bg-zinc-900 px-2 py-1"
              placeholder="Something else? e.g. never run out of potions"
              value={other}
              onChange={(e) => setOther(e.target.value)}
            />
          </fieldset>
          <fieldset className="flex flex-col gap-1 text-sm">
            <legend className="mb-1">AI account</legend>
            <label className="flex items-center gap-2">
              <input
                type="radio"
                checked={backend === "subscription"}
                onChange={() => setBackend("subscription")}
              />
              My Claude subscription (signed in to Claude Code on this PC)
            </label>
            <label className="flex items-center gap-2">
              <input type="radio" checked={backend === "apiKey"} onChange={() => setBackend("apiKey")} />
              An Anthropic API key (set as ANTHROPIC_API_KEY in Windows; you pay per use)
            </label>
            <label className="flex items-center gap-2">
              <input type="radio" checked={backend === "local"} onChange={() => setBackend("local")} />
              A free AI running on this PC (Ollama or LM Studio). Slower, and finds cheats less often
            </label>
            {backend === "local" && (
              <div className="ml-6 flex flex-col gap-2 rounded-lg bg-zinc-900 p-2 text-xs">
                <label className="flex items-center gap-2">
                  Address
                  <input
                    aria-label="Local AI address"
                    className="flex-1 rounded border border-zinc-700 bg-zinc-950 px-2 py-1"
                    value={settings.localUrl}
                    onChange={(e) => update({ localUrl: e.target.value })}
                  />
                </label>
                <label className="flex items-center gap-2">
                  Model
                  <input
                    aria-label="Local AI model"
                    className="flex-1 rounded border border-zinc-700 bg-zinc-950 px-2 py-1"
                    placeholder="e.g. qwen3-coder (its name in Ollama or LM Studio)"
                    value={settings.model}
                    onChange={(e) => update({ model: e.target.value })}
                  />
                </label>
                <p className="text-zinc-500">
                  Ollama uses http://localhost:11434, LM Studio http://localhost:1234. Pick a model
                  that can use tools and has a large context (64k or more).
                </p>
              </div>
            )}
          </fieldset>
          {backend !== "local" && (
            <fieldset className="flex flex-col gap-1 text-sm">
              <legend className="mb-1">How hard it tries</legend>
              <label className="flex items-center gap-2">
                <input type="radio" checked={settings.quality === "best"} onChange={() => update({ quality: "best" })} />
                Best results (Claude Opus)
              </label>
              <label className="flex items-center gap-2">
                <input type="radio" checked={settings.quality === "cheaper"} onChange={() => update({ quality: "cheaper" })} />
                Cheaper and faster (Claude Sonnet): fine for simple games
              </label>
            </fieldset>
          )}
          <label className="flex items-start gap-2 text-sm">
            <input
              type="checkbox"
              className="mt-1"
              checked={settings.pictures}
              onChange={(e) => update({ pictures: e.target.checked })}
            />
            <span>
              Let the helper look at the game window, so it can read numbers itself and you&apos;re asked less.
              <span className="block text-xs text-zinc-500">
                Only that window, only while it works. The pictures go to the AI you picked and aren&apos;t
                saved.
              </span>
            </span>
          </label>
          {kind === "pc" && (
          <details className="text-sm">
            <summary className="cursor-pointer text-zinc-400">Advanced: code analysis tool (optional)</summary>
            <div className="mt-2 flex flex-col gap-2 text-xs">
              <p className="text-zinc-500">
                An MCP server for reading game files (for example REA). The helper only uses it to
                understand code it found. Leave empty if you don&apos;t have one.
              </p>
              <input
                aria-label="Analysis tool command"
                className="rounded border border-zinc-700 bg-zinc-900 px-2 py-1"
                placeholder="Command, e.g. C:\Tools\rea\rea-mcp.exe"
                value={settings.analysisCommand}
                onChange={(e) => update({ analysisCommand: e.target.value })}
              />
              <input
                aria-label="Analysis tool arguments"
                className="rounded border border-zinc-700 bg-zinc-900 px-2 py-1"
                placeholder="Arguments (optional)"
                value={settings.analysisArgs}
                onChange={(e) => update({ analysisArgs: e.target.value })}
              />
            </div>
          </details>
          )}
          {backend === "subscription" && status?.signedIn === false && (
            <div className="flex flex-col gap-2 rounded-lg bg-amber-950/40 px-3 py-2 text-sm text-amber-200" role="note">
              <p>
                The helper isn&apos;t signed in to Claude on this PC yet. Click <b>Sign in</b>: a
                window opens and your browser asks you to log in to your Claude account. When it
                says you&apos;re signed in, come back and click <b>Check again</b>.
              </p>
              <div className="flex gap-2">
                <button className={button} onClick={() => void signIn()}>
                  Sign in
                </button>
                <button className={button} onClick={() => void recheck()}>
                  Check again
                </button>
              </div>
            </div>
          )}
          <label className="flex items-center gap-2 text-sm">
            Stop if it costs more than $
            <input
              aria-label="Spending limit"
              className="w-20 rounded border border-zinc-700 bg-zinc-900 px-2 py-1"
              value={budget}
              onChange={(e) => setBudget(e.target.value)}
            />
            <span className="text-xs text-zinc-500">(an estimate; a typical run is well under this)</span>
          </label>
          <button
            className={`${big} self-start`}
            disabled={
              (kind === "web" ? !webGame : !pid) ||
              starting ||
              (wishes.length === 0 && !other.trim()) ||
              (backend === "subscription" && status?.signedIn === false) ||
              (backend === "local" && !settings.model.trim())
            }
            onClick={() => void start()}
          >
            Start
          </button>
        </div>
      )}

      {!showForm && (
        <ol className="flex max-w-3xl flex-col gap-2" aria-label="Conversation">
          {events.map((e, i) => {
            switch (e.type) {
              case "say":
                return (
                  <li key={i} className="whitespace-pre-wrap rounded-lg bg-zinc-900 px-3 py-2 text-sm">
                    {e.text}
                  </li>
                );
              case "activity":
                return events[i - 1]?.type === "activity" &&
                  (events[i - 1] as { summary: string }).summary === e.summary ? null : (
                  <li key={i} className="text-xs text-zinc-500">
                    {e.summary}…
                  </li>
                );
              case "ask":
                return (
                  <li key={i} className="rounded-lg border border-emerald-800 px-3 py-2 text-sm">
                    {e.text}
                  </li>
                );
              case "answered":
                return (
                  <li key={i} className="self-end rounded-lg bg-emerald-950/60 px-3 py-1 text-sm">
                    {e.text}
                  </li>
                );
              case "note":
                return (
                  <li key={i} className="self-end whitespace-pre-wrap rounded-lg bg-emerald-950/60 px-3 py-1 text-sm">
                    <span className="block text-[10px] uppercase tracking-wide text-emerald-400">Your message</span>
                    {e.text}
                  </li>
                );
              case "error":
                return (
                  <li key={i} className="rounded-lg bg-red-950/40 px-3 py-2 text-sm text-red-300">
                    {e.message}
                  </li>
                );
              case "finished":
                return (
                  <li
                    key={i}
                    className={`rounded-lg px-3 py-2 text-sm ${
                      e.ok ? "bg-emerald-950/50 text-emerald-200" : "bg-amber-950/40 text-amber-200"
                    }`}
                    role="status"
                  >
                    {e.summary}
                    {e.ok && (
                      <p className="mt-1 text-xs">
                        {webTask
                          ? "Open Web games: the new cheats are in the game's cheats, with Lock and Set."
                          : "Open Cheats, attach to the game, and load the new pack on the Trainer tab."}
                      </p>
                    )}
                  </li>
                );
              default:
                return null;
            }
          })}
        </ol>
      )}

      {ask && (
        <div className="sticky bottom-0 flex max-w-3xl flex-col gap-3 rounded-lg border border-emerald-700 bg-zinc-950 p-3" role="dialog" aria-label="Your turn">
          <p className="text-base">{ask.text}</p>
          {ask.kind === "done" && (
            <button className={`${big} self-start`} onClick={() => void answer("Done")}>
              Done
            </button>
          )}
          {ask.kind === "yesno" && (
            <div className="flex gap-2">
              <button className={big} onClick={() => void answer("Yes")}>
                Yes
              </button>
              <button className={button} onClick={() => void answer("No")}>
                No
              </button>
            </div>
          )}
          <form
            className="flex gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              if (reply.trim()) void answer(reply.trim());
            }}
          >
            <input
              aria-label={ask.kind === "text" ? "Your answer" : "Your own answer"}
              className="flex-1 rounded border border-zinc-700 bg-zinc-900 px-2 py-1 text-sm placeholder:text-zinc-500"
              placeholder={
                ask.kind === "text"
                  ? undefined
                  : ask.kind === "yesno"
                    ? "Or write your own answer, e.g. “Yes, but fire damage still hurts”"
                    : "Or write your own answer"
              }
              value={reply}
              onChange={(e) => setReply(e.target.value)}
              autoFocus={ask.kind === "text"}
            />
            <button className={ask.kind === "text" ? big : button} type="submit" disabled={!reply.trim()}>
              Send
            </button>
          </form>
          <button className="self-start text-xs text-zinc-500 underline" onClick={() => void answer("I can't do that or I don't see it.")}>
            I can't do this
          </button>
        </div>
      )}
      {running && !ask && !finished && (
        <form
          className="sticky bottom-0 flex max-w-3xl flex-col gap-1 rounded-lg border border-zinc-800 bg-zinc-950 p-3"
          aria-label="Message the helper"
          onSubmit={(e) => {
            e.preventDefault();
            if (note.trim()) void sendNote(note.trim());
          }}
        >
          <p className="text-xs text-zinc-500">Working… You can still tell the helper something.</p>
          <div className="flex gap-2">
            <input
              aria-label="Message the helper"
              className="flex-1 rounded border border-zinc-700 bg-zinc-900 px-2 py-1 text-sm placeholder:text-zinc-500"
              placeholder="e.g. “Infinite ammo only works with the pistol”"
              maxLength={2000}
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />
            <button className={button} type="submit" disabled={!note.trim()}>
              Send
            </button>
          </div>
          <p className="text-[11px] text-zinc-600">It reads your message after its current step.</p>
        </form>
      )}
    </div>
  );
}

/** The AI helper can't start yet: say why, and offer the engine download. */
function AiSetup({ status, onReady }: { status: AiStatus; onReady: (s: AiStatus) => void }) {
  const [installing, setInstalling] = useState(status.installing);
  const [progress, setProgress] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const canDownload = status.download === "missing" || status.download === "outdated";
  const needsNode = /Node\.js/.test(status.problem ?? "");

  useEffect(() => onAiInstallProgress(setProgress), []);

  async function download() {
    setError(null);
    setInstalling(true);
    try {
      await api.aiInstall();
      onReady(await api.aiStatus());
    } catch (e) {
      setError(friendlyError(e));
    } finally {
      setInstalling(false);
    }
  }

  return (
    <div className="m-auto flex max-w-md flex-col items-center gap-3 p-6 text-center text-sm text-zinc-300">
      <p className="font-medium">The AI helper can't start yet.</p>
      <p className="text-zinc-400">{status.problem}</p>
      {needsNode && (
        <button className={button} onClick={() => void api.openLink("node").catch(() => undefined)}>
          Open nodejs.org
        </button>
      )}
      {canDownload && !needsNode && (
        <>
          <p className="text-xs text-zinc-500">
            The engine is Anthropic's Claude Agent SDK, downloaded from npm. It's only needed for “Make a trainer (AI)”;
            everything else works without it.
          </p>
          <button
            className="rounded-lg btn-accent px-4 py-2 text-sm font-semibold disabled:opacity-40"
            disabled={installing}
            onClick={() => void download()}
          >
            {installing ? "Downloading…" : "Download the AI engine"}
          </button>
        </>
      )}
      {installing && progress && (
        <p className="text-xs text-sky-300" role="status">
          {progress}
        </p>
      )}
      {error && (
        <p className="rounded-lg bg-red-950/40 px-3 py-2 text-red-300" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
