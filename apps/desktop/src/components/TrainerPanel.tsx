import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import { chosenPacks as chosenFor, formatStep, groupByCategory } from "../lib/trainer";
import { CheatSettings } from "./CheatSettings";
import { PresetBar } from "./PresetBar";
import { RemotePanel } from "./RemotePanel";
import {
  api,
  type BuildMatch,
  type Cheat,
  type CheatState,
  type CommandError,
  isSessionLost,
  onTrainerChanged,
  PACK_SOURCE_LABEL,
  type PackSummary,
  packsFor,
  type SessionInfo,
  toCommandError,
  type TrainerView,
} from "../lib/ipc";

const POLL_MS = 500;

const SLIDER_DEBOUNCE_MS = 150;

interface Props {
  session: SessionInfo;
  onSessionLost: (e: CommandError) => void;
  /** Opens the Packs tab (open a pack file, community packs). */
  onShowPacks?: () => void;
}

/** Trainer tab: pick a pack for the attached game, then toggle its cheats. */
export function TrainerPanel({ session, onSessionLost, onShowPacks }: Props) {
  const [packs, setPacks] = useState<PackSummary[] | null>(null);
  const [folder, setFolder] = useState<string | null>(null);
  const [view, setView] = useState<TrainerView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [repairNote, setRepairNote] = useState<string | null>(null);
  const [repairing, setRepairing] = useState(false);
  const [loading, setLoading] = useState(false);
  const [overlay, setOverlay] = useState(false);
  /** The trainer's state was read at least once (so `view === null` means "no pack loaded"). */
  const [polled, setPolled] = useState(false);
  /** The pack was loaded automatically (the only one for this game). */
  const [autoLoaded, setAutoLoaded] = useState(false);

  useEffect(() => {
    api.overlayVisible().then(setOverlay, () => undefined);
  }, []);

  const fail = useCallback(
    (e: unknown) => {
      const err = toCommandError(e);
      if (isSessionLost(err)) onSessionLost(err);
      else setError(err.message);
    },
    [onSessionLost],
  );

  const loadPacks = useCallback(() => {
    api.packsList().then(setPacks, fail);
    api.packsFolder().then(setFolder, () => undefined);
  }, [fail]);

  useEffect(() => loadPacks(), [loadPacks]);

  const poll = useCallback(() => {
    api.trainerState().then(
      (v) => {
        setView(v);
        setPolled(true);
      },
      (e: unknown) => {
      const err = toCommandError(e);
      if (isSessionLost(err)) onSessionLost(err);
      },
    );
  }, [onSessionLost]);

  useEffect(() => {
    poll();
    const timer = setInterval(poll, POLL_MS);
    const stop = onTrainerChanged(poll);
    return () => {
      clearInterval(timer);
      stop();
    };
  }, [poll]);

  async function act(fn: () => Promise<TrainerView | void>) {
    setError(null);
    try {
      const v = await fn();
      if (v) setView(v);
    } catch (e) {
      fail(e);
      poll();
    }
  }

  const load = useCallback(
    async (key: string, auto = false) => {
      setLoading(true);
      setError(null);
      try {
        setView(await api.trainerLoad(key));
        setAutoLoaded(auto);
      } catch (e) {
        fail(e);
      } finally {
        setLoading(false);
      }
    },
    [fail],
  );

  // One pack for this game and none loaded: load it straight away (loading only
  // reads the pack and finds its cheats; nothing is turned on).
  const pid = session.process.pid;
  const only = packs && polled && !view ? packsFor(packs, session.process) : [];
  const autoKey = only.length === 1 ? (only[0]?.key ?? null) : null;
  // A pack that was loaded for this game (by hand, by a preset, or before this tab
  // opened) means the player has chosen: after "Unload", don't load it again.
  useEffect(() => {
    if (view) chosenFor.add(pid);
  }, [view, pid]);
  useEffect(() => {
    if (!autoKey || chosenFor.has(pid)) return;
    chosenFor.add(pid);
    void Promise.resolve().then(() => load(autoKey, true));
  }, [autoKey, pid, load]);

  if (!view) {
    const matching = packs ? packsFor(packs, session.process) : [];
    return (
      <section className="flex flex-col gap-3" aria-label="Trainer">
        <h2 className="text-sm font-medium text-zinc-300">
          Cheat packs for <span className="font-mono">{session.process.name}</span>
        </h2>
        {packs === null && <p className="text-sm text-zinc-500">Looking for packs…</p>}
        {matching.map((p) => (
          <PackRow key={p.key} pack={p} loading={loading} onLoad={() => void load(p.key)} />
        ))}
        {packs !== null && matching.length === 0 && (
          <p className="text-sm text-zinc-400">
            No pack for this game yet. Get one in Packs (open a pack file, or install one from the community
            list), make one with the AI helper, or find values yourself with the Scanner. Your packs are in{" "}
            <code className="text-xs">{folder ?? "%APPDATA%\\OpenTrainer\\packs"}</code>.
          </p>
        )}
        <div className="flex gap-2">
          {onShowPacks && (
            <button
              className="rounded-lg bg-emerald-800 px-3 py-1.5 text-xs hover:bg-emerald-700"
              onClick={onShowPacks}
            >
              Get a pack…
            </button>
          )}
          <button
            className="rounded-lg bg-zinc-800 px-3 py-1.5 text-xs hover:bg-zinc-700"
            onClick={() => void act(() => api.openPacksFolder())}
          >
            Open packs folder
          </button>
          <button className="rounded-lg bg-zinc-800 px-3 py-1.5 text-xs hover:bg-zinc-700" onClick={loadPacks}>
            Refresh
          </button>
        </div>
        {error && (
          <p className="text-xs text-red-400" role="alert">
            {error}
          </p>
        )}
      </section>
    );
  }

  async function repair() {
    setError(null);
    setRepairNote(null);
    setRepairing(true);
    try {
      const r = await api.trainerRepair();
      setView(r.view);
      const names = (ids: string[]) =>
        ids.map((id) => r.view.pack.cheats.find((c) => c.id === id)?.name ?? id).join(", ");
      const parts = [];
      if (r.fixed.length) parts.push(`Repaired: ${names(r.fixed.map((f) => f.cheat))}. Saved as your copy of the pack.`);
      if (r.broken.length) parts.push(`Couldn't repair: ${names(r.broken.map((b) => b[0]))}. The AI helper may be able to remake them.`);
      if (!parts.length) parts.push("Nothing to repair.");
      setRepairNote(parts.join(" "));
    } catch (e) {
      fail(e);
    } finally {
      setRepairing(false);
    }
  }

  const { pack } = view;
  const favourites = view.settings.favourites;
  const favs = pack.cheats.filter((c) => favourites.includes(c.id));
  const groups: [string, Cheat[]][] = [
    ...(favs.length ? [["★ Favourites", favs] as [string, Cheat[]]] : []),
    ...groupByCategory(pack.cheats.filter((c) => !favourites.includes(c.id))),
  ];
  const anyFailed = view.cheats.some((c) => c.status === "failed");
  const stateOf = (id: string) => view.cheats.find((c) => c.id === id);

  return (
    <section className="flex flex-col gap-3" aria-label="Trainer">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="text-lg font-semibold tracking-tight">{pack.game.name}</h2>
        <span className="text-xs text-zinc-500">
          v{pack.meta.packVersion} · by {pack.meta.author}
        </span>
        {pack.meta.author.trim().toLowerCase() === "ai" && <Badge tone="violet">AI-made</Badge>}
        <BuildBadge build={view.build} />
        <button
          className={`ml-auto rounded-lg px-3 py-1.5 text-xs font-medium ring-1 ${
            overlay ? "bg-emerald-950 text-emerald-200 ring-emerald-800" : "bg-zinc-800 ring-zinc-700 hover:bg-zinc-700"
          }`}
          aria-pressed={overlay}
          title="A see-through list of cheats on top of the game (windowed or borderless games)"
          onClick={() => void act(async () => setOverlay(await api.overlaySet(!overlay)))}
        >
          {overlay ? "Hide overlay" : "Show overlay"}
        </button>
        <button
          className="rounded-lg bg-zinc-800 px-3 py-1.5 text-xs font-medium ring-1 ring-zinc-700 hover:bg-zinc-700"
          onClick={() => void act(async () => {
            await api.trainerUnload();
            setView(null);
            setAutoLoaded(false);
          })}
        >
          Unload pack
        </button>
      </div>
      {autoLoaded && (
        <p className="text-xs text-zinc-500" role="status">
          Loaded automatically: it&apos;s the only pack for this game. Nothing is on until you turn it on.
        </p>
      )}
      {(anyFailed || repairNote) && (
        <div className="flex flex-wrap items-center gap-2 rounded-lg bg-zinc-900 px-3 py-2 text-xs" role="status">
          {repairNote ? (
            <span className="text-zinc-300">{repairNote}</span>
          ) : (
            <span className="text-zinc-300">
              Some cheats don&apos;t match this version of the game (maybe it was updated).
            </span>
          )}
          {anyFailed && (
            <button
              className="rounded bg-emerald-800 px-2 py-1 hover:bg-emerald-700 disabled:opacity-40"
              disabled={repairing}
              onClick={() => void repair()}
            >
              {repairing ? "Repairing…" : "Try to repair"}
            </button>
          )}
        </div>
      )}
      {buildWarning(view.build) && (
        <p className="rounded-lg bg-amber-950/40 px-3 py-2 text-xs text-amber-300">{buildWarning(view.build)}</p>
      )}
      {error && (
        <p className="rounded-lg bg-red-950/40 px-3 py-2 text-xs text-red-300" role="alert">
          {error}
        </p>
      )}

      <PresetBar view={view} act={act} />

      {groups.map(([category, cheats]) => (
        <div key={category} className="flex flex-col gap-2">
          <h3 className="px-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-zinc-400">{category}</h3>
          <ul className="divide-y divide-zinc-800/70 overflow-hidden rounded-2xl bg-zinc-900/80 ring-1 ring-zinc-800">
            {cheats.map((c) => {
              const st = stateOf(c.id);
              return st ? (
                <CheatRow
                  key={c.id}
                  cheat={c}
                  state={st}
                  hotkeyError={view.hotkeyErrors[c.id]}
                  pad={view.pads[c.id]}
                  note={view.settings.notes[c.id]}
                  favourite={favourites.includes(c.id)}
                  onFavourite={(on) => void act(() => api.cheatFavourite(c.id, on))}
                  settings={<CheatSettings cheat={c} view={view} act={act} />}
                  onToggle={(on) => void act(() => api.trainerToggle(c.id, on))}
                  onSet={(v) => void act(() => api.trainerSet(c.id, v))}
                  onRelease={() => void act(() => api.trainerRelease(c.id))}
                  onPress={() => void act(() => api.trainerPress(c.id))}
                />
              ) : null;
            })}
          </ul>
        </div>
      ))}

      <RemotePanel />

      {view.warnings.length > 0 && (
        <details className="text-xs text-zinc-500">
          <summary className="cursor-pointer">{view.warnings.length} pack warning(s)</summary>
          <ul className="mt-1 list-disc pl-5">
            {view.warnings.map((w) => (
              <li key={w.path + w.message}>
                <code>{w.path}</code>: {w.message}
              </li>
            ))}
          </ul>
        </details>
      )}
    </section>
  );
}

function PackRow({ pack, loading, onLoad }: { pack: PackSummary; loading: boolean; onLoad: () => void }) {
  return (
    <div className="flex items-center gap-3 rounded-2xl bg-zinc-900/80 px-4 py-3 ring-1 ring-zinc-800">
      <div className="min-w-0">
        <p className="text-sm font-medium">
          {pack.name ?? pack.key}{" "}
          <span className="text-xs text-zinc-500">
            {pack.packVersion && `v${pack.packVersion} · `}
            {pack.cheats} cheat{pack.cheats === 1 ? "" : "s"} · {PACK_SOURCE_LABEL[pack.source]}
          </span>
          {pack.ai && (
            <span className="ml-2">
              <Badge tone="violet">AI-made</Badge>
            </span>
          )}
        </p>
        {pack.error && <p className="text-xs text-red-400">{pack.error}</p>}
      </div>
      <button
        className="btn-accent ml-auto rounded-lg px-5 py-1.5 text-xs font-semibold disabled:opacity-40"
        disabled={Boolean(pack.error) || loading}
        aria-label={`Load ${pack.name ?? pack.key}`}
        onClick={onLoad}
      >
        Load
      </button>
    </div>
  );
}

interface RowProps {
  cheat: Cheat;
  state: CheatState;
  hotkeyError: string | undefined;
  pad?: string;
  note?: string;
  favourite?: boolean;
  onFavourite?: (on: boolean) => void;
  settings?: ReactNode;
  onToggle: (on: boolean) => void;
  onSet: (value: string) => void;
  onRelease: () => void;
  onPress: () => void;
}

function CheatRow(props: RowProps) {
  const { cheat, state, hotkeyError, pad, note, favourite, onFavourite, settings } = props;
  const failed = state.status === "failed";
  return (
    <li
      className={`flex flex-col gap-1 px-4 py-3 transition-colors hover:bg-zinc-800/40 ${
        state.active ? "bg-emerald-950/25" : ""
      }`}
      data-testid={`cheat-${cheat.id}`}
    >
      <div className="flex items-center gap-3">
        {onFavourite && (
          <button
            className={`text-sm ${favourite ? "text-amber-300" : "text-zinc-600 hover:text-zinc-400"}`}
            aria-label={`Favourite ${cheat.name}`}
            aria-pressed={Boolean(favourite)}
            onClick={() => onFavourite(!favourite)}
          >
            {favourite ? "★" : "☆"}
          </button>
        )}
        <div className="min-w-0 flex-1">
          <p className={`text-[15px] font-medium ${failed ? "text-zinc-500" : "text-zinc-100"}`}>
            {cheat.name}
            {state.active && cheat.ui !== "toggle" && (
              <span className="ml-2">
                <Badge tone="sky">held</Badge>
              </span>
            )}
          </p>
          {cheat.description && <p className="text-xs text-zinc-500">{cheat.description}</p>}
          {note && <p className="text-xs italic text-zinc-400">{note}</p>}
        </div>
        {pad && (
          <kbd className="keycap" title={`Controller: ${pad}`}>
            🎮 {pad}
          </kbd>
        )}
        {state.hotkey && (
          <kbd
            className={`keycap ${hotkeyError ? "!text-red-400 line-through" : ""}`}
            title={hotkeyError ?? `Hotkey: ${state.hotkey}`}
          >
            {state.hotkey}
          </kbd>
        )}
        <Control
          cheat={cheat}
          state={state}
          onToggle={props.onToggle}
          onSet={props.onSet}
          onRelease={props.onRelease}
          onPress={props.onPress}
        />
      </div>
      {state.message && (
        <p className={`text-xs ${failed ? "text-red-400" : "text-amber-400"}`} role={failed ? "alert" : undefined}>
          {state.message}
        </p>
      )}
      {hotkeyError && <p className="text-xs text-amber-400">{hotkeyError}</p>}
      <div className="flex gap-4">
        <details className="text-[11px] text-zinc-500">
          <summary className="cursor-pointer select-none">What it changes</summary>
          <ul className="mt-1 font-mono">
            {state.touches.map((t) => (
              <li key={t}>{t}</li>
            ))}
          </ul>
        </details>
        {settings && (
          <details className="flex-1 text-[11px] text-zinc-500">
            <summary className="cursor-pointer select-none">Keys &amp; note</summary>
            {settings}
          </details>
        )}
      </div>
    </li>
  );
}

type ControlProps = Pick<RowProps, "cheat" | "state" | "onToggle" | "onSet" | "onRelease" | "onPress">;

function Control({ cheat, state, onToggle, onSet, onRelease, onPress }: ControlProps) {
  const disabled = state.status === "failed";
  switch (cheat.ui) {
    case "toggle":
      return (
        <button
          role="switch"
          aria-checked={state.active}
          aria-label={cheat.name}
          disabled={disabled}
          onClick={() => onToggle(!state.active)}
          className={`relative h-7 w-[68px] shrink-0 rounded-full text-[10px] font-bold tracking-wider transition-all disabled:opacity-40 ${
            state.active ? "btn-accent" : "bg-zinc-800 text-zinc-500 ring-1 ring-zinc-700"
          }`}
        >
          <span className={`absolute top-1/2 -translate-y-1/2 ${state.active ? "left-3" : "right-3"}`} aria-hidden="true">
            {state.active ? "ON" : "OFF"}
          </span>
          <span
            className={`absolute top-1 h-5 w-5 rounded-full shadow transition-all ${
              state.active ? "left-[44px] bg-white" : "left-1 bg-zinc-500"
            }`}
          />
        </button>
      );
    case "button":
      return (
        <button
          className="min-w-[68px] rounded-full bg-emerald-950/60 px-4 py-1.5 text-xs font-semibold text-emerald-200 ring-1 ring-emerald-800 hover:bg-emerald-900/70 disabled:opacity-40"
          disabled={disabled}
          aria-label={cheat.name}
          onClick={onPress}
        >
          {cheat.button?.add !== undefined
            ? `${cheat.button.add >= 0 ? "+" : ""}${cheat.button.add}`
            : `Set ${cheat.button?.set}`}
        </button>
      );
    case "number":
      return (
        <NumberControl cheat={cheat} state={state} disabled={disabled} onSet={onSet} onRelease={onRelease} />
      );
    case "slider":
      return (
        <SliderControl cheat={cheat} state={state} disabled={disabled} onSet={onSet} onRelease={onRelease} />
      );
  }
}

interface ValueControlProps {
  cheat: Cheat;
  state: CheatState;
  disabled: boolean;
  onSet: (value: string) => void;
  onRelease: () => void;
}

function NumberControl({ cheat, state, disabled, onSet, onRelease }: ValueControlProps) {
  const [text, setText] = useState("");
  const submit = () => {
    if (!text.trim()) return;
    onSet(text.trim());
    setText("");
  };
  return (
    <div className="flex items-center gap-1">
      <input
        className="w-28 rounded-lg bg-zinc-900 px-2 py-1 font-mono text-xs outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
        aria-label={`${cheat.name} value`}
        placeholder={state.value ?? "??"}
        value={text}
        disabled={disabled}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && submit()}
      />
      <button
        className="rounded-lg bg-zinc-800 px-2 py-1 text-xs hover:bg-zinc-700 disabled:opacity-40"
        disabled={disabled || !text.trim()}
        onClick={submit}
      >
        Set
      </button>
      {state.active && (
        <button className="rounded-lg bg-zinc-800 px-2 py-1 text-xs hover:bg-zinc-700" onClick={onRelease}>
          Release
        </button>
      )}
    </div>
  );
}

function SliderControl({ cheat, state, disabled, onSet, onRelease }: ValueControlProps) {
  const cfg = cheat.slider!;
  const current = state.value !== null ? Number(state.value) : (cfg.default ?? cfg.min);
  const [draft, setDraft] = useState<number | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => () => {
    if (timer.current) clearTimeout(timer.current);
  }, []);
  const shown = draft ?? current;

  function change(v: number) {
    setDraft(v);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      onSet(formatStep(v, cfg.step));
      setDraft(null);
    }, SLIDER_DEBOUNCE_MS);
  }

  return (
    <div className="flex items-center gap-2">
      <input
        type="range"
        aria-label={cheat.name}
        min={cfg.min}
        max={cfg.max}
        step={cfg.step}
        value={Number.isFinite(shown) ? shown : cfg.min}
        disabled={disabled}
        onChange={(e) => change(Number(e.target.value))}
        className="w-40 accent-emerald-500"
      />
      <span className="w-12 rounded-lg bg-zinc-950 px-1.5 py-0.5 text-center font-mono text-xs ring-1 ring-zinc-800">{Number.isFinite(shown) ? formatStep(shown, cfg.step) : "??"}</span>
      {cfg.default !== undefined && (
        <button
          className="rounded-lg bg-zinc-800 px-2 py-1 text-xs hover:bg-zinc-700 disabled:opacity-40"
          disabled={disabled}
          onClick={() => onSet(String(cfg.default))}
          title="Reset to default"
        >
          Reset
        </button>
      )}
      {state.active && (
        <button className="rounded-lg bg-zinc-800 px-2 py-1 text-xs hover:bg-zinc-700" onClick={onRelease}>
          Release
        </button>
      )}
    </div>
  );
}

function buildWarning(b: BuildMatch): string | null {
  switch (b.status) {
    case "unknown":
      return "This game build is not one the pack lists. Patches still check the original bytes before writing, but values may land in the wrong place.";
    case "unreadable":
      return "Couldn't read the game's exe to check its version.";
    default:
      return null;
  }
}

function BuildBadge({ build }: { build: BuildMatch }) {
  switch (build.status) {
    case "verified":
      return <Badge tone="emerald">Verified build{build.label ? ` ${build.label}` : ""}</Badge>;
    case "listed":
      return <Badge tone="sky">Tested build{build.label ? ` ${build.label}` : ""}</Badge>;
    case "unknown":
      return <Badge tone="amber">Unknown build</Badge>;
    case "untagged":
      return <Badge tone="zinc">No build info</Badge>;
    case "unreadable":
      return <Badge tone="amber">Build not checked</Badge>;
  }
}

const TONES = {
  emerald: "bg-green-950 text-green-300",
  sky: "bg-sky-950 text-sky-300",
  amber: "bg-amber-950 text-amber-300",
  violet: "bg-violet-950 text-violet-300",
  zinc: "bg-zinc-800 text-zinc-400",
} as const;

function Badge({ tone, children }: { tone: keyof typeof TONES; children: ReactNode }) {
  return <span className={`rounded-full px-2 py-0.5 text-[11px] font-medium ${TONES[tone]}`}>{children}</span>;
}
