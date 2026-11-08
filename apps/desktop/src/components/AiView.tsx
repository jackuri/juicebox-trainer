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
  toCommandError,
} from "../lib/ipc";
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
}

type Backend = "subscription" | "apiKey" | "local";

interface Settings {
  backend: Backend;
  budget: string;
  localUrl: string;
  model: string;
  analysisCommand: string;
  analysisArgs: string;
}

const DEFAULTS: Settings = {
  backend: "subscription",
  budget: "5",
  localUrl: "http://localhost:11434",
  model: "",
  analysisCommand: "",
  analysisArgs: "",
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
export function AiView({ processes, games, onRefreshProcesses }: Props) {
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [events, setEvents] = useState<AiEvent[]>([]);
  const [pid, setPid] = useState("");
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
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);

  useEffect(() => {
    api.aiStatus().then(
      (s) => {
        setStatus(s);
        setEvents(s.transcript);
      },
      (e: unknown) => setError(toCommandError(e).message),
    );
    return onAiEvent((e) => {
      setEvents((list) => [...list, e]);
      if (e.type === "exit") {
        api.aiStatus().then(setStatus, () => undefined);
      }
    });
  }, []);

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
      await api.aiStart({
        pid: Number(pid),
        wishes: list,
        notes: "",
        backend,
        maxBudgetUsd: Number(budget) || 5,
        localUrl: backend === "local" ? settings.localUrl : undefined,
        model: backend === "local" ? settings.model : undefined,
        analysisCommand: settings.analysisCommand.trim() || undefined,
        analysisArgs: settings.analysisArgs.split(" ").filter(Boolean),
      });
      setStatus((s) => (s ? { ...s, running: true } : s));
    } catch (e) {
      setError(toCommandError(e).message);
    } finally {
      setStarting(false);
    }
  }

  async function signIn() {
    setError(null);
    try {
      await api.aiSignIn();
    } catch (e) {
      setError(toCommandError(e).message);
    }
  }

  async function recheck() {
    try {
      const s = await api.aiStatus();
      setStatus((old) => (old ? { ...old, signedIn: s.signedIn } : s));
    } catch (e) {
      setError(toCommandError(e).message);
    }
  }

  async function answer(text: string) {
    if (!ask) return;
    try {
      await api.aiAnswer(ask.id, text);
      setEvents((list) => [...list, { type: "answered", id: ask.id, text }]);
      setReply("");
    } catch (e) {
      setError(toCommandError(e).message);
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
          <p className="text-sm text-zinc-400">
            Start your game, pick it here and tick what you'd like. The helper asks you to do small
            things in the game (like "spend some gold") while it searches, then saves a cheat pack.
            Single-player games only: games with anti-cheat are refused.
          </p>
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
              !pid ||
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
                        Open Cheats, attach to the game, and load the new pack on the Trainer tab.
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
          {ask.kind === "text" && (
            <form
              className="flex gap-2"
              onSubmit={(e) => {
                e.preventDefault();
                if (reply.trim()) void answer(reply.trim());
              }}
            >
              <input
                aria-label="Your answer"
                className="flex-1 rounded border border-zinc-700 bg-zinc-900 px-2 py-1"
                value={reply}
                onChange={(e) => setReply(e.target.value)}
                autoFocus
              />
              <button className={big} type="submit" disabled={!reply.trim()}>
                Send
              </button>
            </form>
          )}
          <button className="self-start text-xs text-zinc-500 underline" onClick={() => void answer("I can't do that or I don't see it.")}>
            I can't do this
          </button>
        </div>
      )}
      {running && !ask && !finished && <p className="text-xs text-zinc-500">Working…</p>}
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
      setError(toCommandError(e).message);
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
