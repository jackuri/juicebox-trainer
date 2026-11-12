import { useEffect, useRef, useState } from "react";
import { pendingAsk } from "../lib/ai";
import { fromSettings, loadSettings, settingsReady } from "../lib/aiSettings";
import { friendlyError } from "../lib/explain";
import { type AiEvent, type AiStatus, api, onAiEvent } from "../lib/ipc";

const small = "rounded-lg bg-zinc-800 px-3 py-1.5 text-xs ring-1 ring-zinc-700 hover:bg-zinc-700 disabled:opacity-40";
const go = "btn-accent rounded-lg px-4 py-1.5 text-sm font-semibold disabled:opacity-40";
const box = "rounded-lg border border-zinc-700 bg-zinc-900 px-2 py-1.5 text-sm placeholder:text-zinc-500";

interface Props {
  /** A PC game: its process and loaded pack. */
  pid?: number;
  packKey?: string;
  /** A web game instead (v1.31.0): its id, open in the game window. */
  webGame?: string;
  /** The cheat to fix, or null to add a new cheat. */
  fix: { id: string; name: string } | null;
  /** What's wrong (from the cheat book), to start with. */
  problem?: string;
  /** A live session is running already: show it. */
  resume?: boolean;
  onClose: () => void;
  /** Opens "Make a trainer (AI)" to set the helper up. */
  onOpenAi?: () => void;
}

/**
 * The AI helper in the Cheats view (v1.30.0): fixes a cheat, or makes a new
 * one, while the player keeps playing with their other cheats on. When it's
 * done, the pack is loaded again and their cheats go back on.
 */
export function LiveHelper({ pid = 0, packKey = "", webGame, fix, problem = "", resume = false, onClose, onOpenAi }: Props) {
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [events, setEvents] = useState<AiEvent[]>([]);
  const [started, setStarted] = useState(resume);
  const [text, setText] = useState(problem);
  const [reply, setReply] = useState("");
  const [note, setNote] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [settings] = useState(loadSettings);
  const listening = useRef(resume);

  useEffect(() => {
    api.aiStatus().then(
      (s) => {
        setStatus(s);
        if (resume) setEvents(s.transcript);
      },
      (e: unknown) => setError(friendlyError(e)),
    );
    return onAiEvent((e) => {
      if (listening.current) setEvents((list) => [...list, e]);
    });
  }, [resume]);

  const exited = events.some((e) => e.type === "exit");
  const running = started && !exited;
  const ask = pendingAsk(events);
  const done = [...events].reverse().find((e) => e.type === "liveDone");
  const finished = [...events].reverse().find((e) => e.type === "finished");
  const lastActivity = [...events].reverse().find((e) => e.type === "activity");

  async function start() {
    setError(null);
    setStarting(true);
    setEvents([]);
    listening.current = true;
    try {
      const said = text.trim();
      await api.aiStart(
        webGame
          ? {
              pid: 0,
              task: "webTrainer",
              webGame,
              wishes: fix ? [] : [said],
              notes: "",
              webFix: { cheat: fix?.name, problem: fix ? said : undefined },
              ...fromSettings(settings),
              analysisCommand: undefined,
            }
          : {
              pid,
              wishes: fix ? [] : [said],
              notes: "",
              live: { packKey, fix: fix?.id, problem: fix ? said : undefined },
              ...fromSettings(settings),
            },
      );
      setStarted(true);
    } catch (e) {
      listening.current = false;
      setError(friendlyError(e));
    } finally {
      setStarting(false);
    }
  }

  async function answer(t: string) {
    if (!ask) return;
    try {
      await api.aiAnswer(ask.id, t);
      setEvents((list) => [...list, { type: "answered", id: ask.id, text: t }]);
      setReply("");
    } catch (e) {
      setError(friendlyError(e));
    }
  }

  async function tell(t: string) {
    try {
      await api.aiNote(t);
      setEvents((list) => [...list, { type: "note", text: t }]);
      setNote("");
    } catch (e) {
      setError(friendlyError(e));
    }
  }

  function again() {
    listening.current = false;
    setStarted(false);
    setEvents([]);
    setText("");
  }

  const title = fix ? `Fix “${fix.name}” with AI` : webGame ? "Ask the AI about your cheats" : "Make a new cheat with AI";
  const notReady = status && (!status.available || !settingsReady(settings, status.signedIn));

  return (
    <section
      className="flex flex-col gap-3 rounded-2xl bg-violet-950/30 p-4 ring-1 ring-violet-800/60"
      aria-label="AI helper"
    >
      <div className="flex items-center gap-2">
        <h3 className="text-sm font-semibold text-violet-100">{title}</h3>
        {running ? (
          <button className={`${small} ml-auto`} onClick={() => void api.aiCancel().catch(() => undefined)}>
            Stop
          </button>
        ) : (
          <button className={`${small} ml-auto`} onClick={onClose}>
            Close
          </button>
        )}
      </div>

      {error && (
        <p className="text-xs text-red-300" role="alert">
          {error}
        </p>
      )}

      {!started && notReady && (
        <div className="flex flex-col gap-2 text-sm">
          <p className="text-zinc-300">
            {status?.available
              ? "Sign in to the AI helper first (or pick a local model)."
              : (status?.problem ?? "The AI helper isn't set up yet.")}
          </p>
          {onOpenAi && (
            <button className={`${small} self-start`} onClick={onOpenAi}>
              Set up the AI helper
            </button>
          )}
        </div>
      )}

      {!started && !notReady && (
        <div className="flex flex-col gap-2">
          <label className="flex flex-col gap-1 text-sm">
            <span>{fix ? `What's wrong with ${fix.name}?` : webGame ? "What do you want, or what are you stuck on?" : "What cheat do you want?"}</span>
            <textarea
              className={`${box} min-h-16`}
              aria-label={fix ? "What's wrong" : "The cheat you want"}
              placeholder={
                fix
                  ? "e.g. “It only works with the gun I start with, not guns I pick up”"
                  : webGame
                    ? "e.g. “Put all my Weak enemy cheats together into one that keeps every zombie at 1 HP”"
                    : "e.g. “Infinite flashlight battery. It drains, but the game shows no number”"
              }
              maxLength={2000}
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
          </label>
          <p className="text-xs text-zinc-400">
            Keep playing: your {webGame ? "locks" : "other cheats"} stay on. The AI may ask you to do small things in the
            game{webGame ? ", and it may change or put together the cheats you asked about" : ""}. It uses your settings from
            Make a trainer (AI), up to ${fromSettings(settings).maxBudgetUsd}.
          </p>
          <button className={`${go} self-start`} disabled={starting || !text.trim()} onClick={() => void start()}>
            {starting ? "Starting…" : fix ? "Fix it now" : webGame ? "Send to the AI" : "Make it now"}
          </button>
        </div>
      )}

      {started && (
        <ol className="flex max-h-80 flex-col gap-1.5 overflow-y-auto" aria-label="Conversation">
          {events.map((e, i) => {
            switch (e.type) {
              case "say":
                return (
                  <li key={i} className="whitespace-pre-wrap rounded-lg bg-zinc-900 px-3 py-1.5 text-sm">
                    {e.text}
                  </li>
                );
              case "ask":
                // The question waiting for an answer shows in the box below.
                return ask?.id === e.id ? null : (
                  <li key={i} className="rounded-lg border border-violet-700 px-3 py-1.5 text-sm">
                    {e.text}
                  </li>
                );
              case "answered":
              case "note":
                return (
                  <li key={i} className="self-end whitespace-pre-wrap rounded-lg bg-violet-900/50 px-3 py-1 text-sm">
                    {e.text}
                  </li>
                );
              case "error":
                return (
                  <li key={i} className="rounded-lg bg-red-950/40 px-3 py-1.5 text-sm text-red-300">
                    {e.message}
                  </li>
                );
              case "finished":
                return (
                  <li key={i} className={`rounded-lg px-3 py-1.5 text-sm ${e.ok ? "bg-emerald-950/50 text-emerald-200" : "bg-amber-950/40 text-amber-200"}`}>
                    {e.summary}
                  </li>
                );
              case "liveDone":
                return (
                  <li key={i} className="rounded-lg bg-emerald-950/40 px-3 py-1.5 text-sm text-emerald-200" role="status">
                    {e.reloaded ? "Your cheats are loaded again with the changes." : "Nothing in your cheats was changed."}
                    {e.backOn.length > 0 && ` Back on: ${e.backOn.join(", ")}.`}
                    {e.problems.length > 0 && <span className="block text-amber-300">Not back on: {e.problems.join("; ")}</span>}
                  </li>
                );
              default:
                return null;
            }
          })}
          {running && !ask && (
            <li className="text-xs text-zinc-500" role="status">
              {lastActivity?.type === "activity" ? `${lastActivity.summary}…` : "Starting…"}
            </li>
          )}
          {exited && !done && !finished && <li className="text-xs text-zinc-500">The AI helper stopped.</li>}
        </ol>
      )}

      {ask && (
        <div className="flex flex-col gap-2 rounded-lg border border-violet-600 bg-zinc-950 p-3" role="dialog" aria-label="Your turn">
          <p className="text-sm">{ask.text}</p>
          {ask.kind === "done" && (
            <button className={`${go} self-start`} onClick={() => void answer("Done")}>
              Done
            </button>
          )}
          {ask.kind === "yesno" && (
            <div className="flex gap-2">
              <button className={go} onClick={() => void answer("Yes")}>
                Yes
              </button>
              <button className={small} onClick={() => void answer("No")}>
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
              className={`${box} flex-1`}
              aria-label={ask.kind === "text" ? "Your answer" : "Your own answer"}
              placeholder={ask.kind === "text" ? undefined : "Or write your own answer"}
              value={reply}
              onChange={(e) => setReply(e.target.value)}
            />
            <button className={small} type="submit" disabled={!reply.trim()}>
              Send
            </button>
          </form>
          <button className="self-start text-xs text-zinc-500 underline" onClick={() => void answer("I can't do that or I don't see it.")}>
            I can&apos;t do this
          </button>
        </div>
      )}

      {running && !ask && (
        <form
          className="flex gap-2"
          aria-label="Message the helper"
          onSubmit={(e) => {
            e.preventDefault();
            if (note.trim()) void tell(note.trim());
          }}
        >
          <input
            className={`${box} flex-1`}
            aria-label="Message the helper"
            placeholder="Tell the AI something, e.g. “the battery only drains with the light on”"
            maxLength={2000}
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
          <button className={small} type="submit" disabled={!note.trim()}>
            Send
          </button>
        </form>
      )}

      {exited && (
        <div className="flex gap-2">
          <button className={small} onClick={again}>
            Ask again
          </button>
        </div>
      )}
    </section>
  );
}
