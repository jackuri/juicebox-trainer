import { useCallback, useEffect, useMemo, useState } from "react";
import { ShareDialog } from "./ShareDialog";
import {
  api,
  type CommunityList,
  type CommunityPack,
  type Credit,
  isTable,
  PACK_SOURCE_LABEL,
  type PackPreview,
  type PackSummary,
  toCommandError,
} from "../lib/ipc";

const button = "rounded-lg bg-zinc-800 px-3 py-1.5 text-xs hover:bg-zinc-700 disabled:cursor-not-allowed disabled:opacity-40";
const primary =
  "rounded-lg bg-emerald-700 px-3 py-1.5 text-xs font-medium hover:bg-emerald-600 disabled:cursor-not-allowed disabled:opacity-40";

/** Where the pack in the install dialog comes from. */
type Incoming = { kind: "file"; path: string; exe?: string } | { kind: "community"; id: string };

/** A file to open in Packs; `exe` names the game for a table that doesn't say. */
export interface OpenFile {
  path: string;
  exe?: string;
}

interface Props {
  /** An .otpack opened from Explorer, dropped on the window, or a downloaded table. */
  openPath: OpenFile | null;
  onOpened: () => void;
  /** Go to the Cheats view (after installing a pack). */
  onShowCheats: () => void;
}

/** Your packs, .otpack import / export, and the community list. */
export function PacksView({ openPath, onOpened, onShowCheats }: Props) {
  const [packs, setPacks] = useState<PackSummary[] | null>(null);
  const [community, setCommunity] = useState<CommunityList | null>(null);
  const [communityError, setCommunityError] = useState<string | null>(null);
  const [loadingCommunity, setLoadingCommunity] = useState(true);
  const [filter, setFilter] = useState("");
  const [incoming, setIncoming] = useState<{ from: Incoming; preview: PackPreview } | null>(null);
  const [confirmRemove, setConfirmRemove] = useState<string | null>(null);
  const [sharing, setSharing] = useState<{ pack: PackSummary; mode: "pack" | "link" } | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ text: string; path?: string; cheats?: boolean } | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** The table maker's credit, typed in the install dialog. */
  const [credit, setCredit] = useState<Credit>({});

  const loadPacks = useCallback(() => {
    api.packsList().then(setPacks, (e: unknown) => setError(toCommandError(e).message));
  }, []);

  /** Downloads the list; `loadingCommunity` is set by the caller. */
  const loadCommunity = useCallback((refresh: boolean) => {
    api
      .communityList(refresh)
      .then(
        (list) => {
          setCommunity(list);
          setCommunityError(null);
        },
        (e: unknown) => setCommunityError(toCommandError(e).message),
      )
      .finally(() => setLoadingCommunity(false));
  }, []);

  useEffect(() => {
    loadPacks();
    loadCommunity(false);
  }, [loadPacks, loadCommunity]);

  const preview = useCallback(async (from: Incoming, keepCredit = false) => {
    setError(null);
    setMessage(null);
    if (!keepCredit) setCredit({});
    setBusy(true);
    try {
      const p = from.kind === "file" ? await api.packsPreview(from.path, from.exe) : await api.communityPreview(from.id);
      setIncoming({ from, preview: p });
    } catch (e) {
      setError(toCommandError(e).message);
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    if (!openPath) return;
    // After this render: show what's in the file the player opened.
    void Promise.resolve().then(() => {
      onOpened();
      return preview({ kind: "file", path: openPath.path, exe: openPath.exe });
    });
  }, [openPath, onOpened, preview]);

  async function pickFile() {
    setError(null);
    try {
      const path = await api.packsPickOtpack();
      if (path) await preview({ kind: "file", path });
    } catch (e) {
      setError(toCommandError(e).message);
    }
  }

  /** For a Cheat Engine table: the player names the game's exe. */
  async function chooseExe(path: string) {
    setError(null);
    try {
      const exe = await api.pickExe("Choose the game's exe");
      const name = exe?.split(/[\\/]/).pop();
      if (name) await preview({ kind: "file", path, exe: name }, true);
    } catch (e) {
      setError(toCommandError(e).message);
    }
  }

  async function install() {
    if (!incoming) return;
    const { from, preview: p } = incoming;
    const replace = Boolean(p.existing && !p.existing.same);
    setBusy(true);
    setError(null);
    try {
      if (from.kind === "file") await api.packsImport(from.path, replace, from.exe, p.checksum, p.table ? credit : undefined);
      else await api.communityInstall(from.id, replace);
      setIncoming(null);
      setMessage({ text: `Installed the pack for ${p.game}. Start the game and load it in Cheats.`, cheats: true });
      loadPacks();
      if (from.kind === "community") loadCommunity(false);
    } catch (e) {
      setError(toCommandError(e).message);
    } finally {
      setBusy(false);
    }
  }

  async function exportPack(p: PackSummary) {
    setError(null);
    setMessage(null);
    try {
      const path = await api.packsExport(p.key);
      if (path) setMessage({ text: `Saved ${p.name ?? "the pack"} as ${path}. Send this file to share it.`, path });
    } catch (e) {
      setError(toCommandError(e).message);
    }
  }

  async function remove(p: PackSummary) {
    setConfirmRemove(null);
    setError(null);
    try {
      const kept = await api.packsRemove(p.key);
      setMessage({ text: `Removed ${p.name ?? "the pack"}. A copy is kept in ${kept}.`, path: kept });
      loadPacks();
    } catch (e) {
      setError(toCommandError(e).message);
    }
  }

  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return (community?.packs ?? []).filter(
      (p) => !q || p.name.toLowerCase().includes(q) || p.exe.toLowerCase().includes(q),
    );
  }, [community, filter]);

  return (
    <main className="flex min-h-0 flex-col gap-5 overflow-y-auto p-6" aria-label="Packs">
      {message && (
        <div className="flex items-center gap-3 rounded-lg bg-emerald-950/40 px-3 py-2 text-sm text-emerald-200" role="status">
          <span className="flex-1">{message.text}</span>
          {message.path && (
            <button className={button} onClick={() => void api.packsShowFile(message.path ?? "")}>
              Show in folder
            </button>
          )}
          {message.cheats && (
            <button className={button} onClick={onShowCheats}>
              Go to Cheats
            </button>
          )}
        </div>
      )}
      {error && (
        <p className="rounded-lg bg-red-950/40 px-3 py-2 text-sm text-red-300" role="alert">
          {error}
        </p>
      )}

      {sharing && <ShareDialog pack={sharing.pack} mode={sharing.mode} onClose={() => setSharing(null)} />}

      {incoming && (
        <InstallDialog
          preview={incoming.preview}
          fromCommunity={incoming.from.kind === "community"}
          busy={busy}
          onChooseExe={
            incoming.from.kind === "file" && isTable(incoming.from.path)
              ? () => incoming.from.kind === "file" && void chooseExe(incoming.from.path)
              : undefined
          }
          credit={incoming.preview.table ? credit : undefined}
          onCredit={setCredit}
          onInstall={() => void install()}
          onCancel={() => setIncoming(null)}
        />
      )}

      <section className="flex flex-col gap-2" aria-label="Your packs">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="text-2xl font-semibold tracking-tight">Your packs</h2>
          <button className={`${button} ml-auto`} disabled={busy} onClick={() => void pickFile()}>
            Open a pack or table…
          </button>
          <button className={button} onClick={() => void api.openPacksFolder().catch(() => undefined)}>
            Pack folder
          </button>
        </div>
        <p className="text-xs text-zinc-500">
          Share a pack by sending its <code>.otpack</code> file. To open one, double-click it, drop it on this window or use
          “Open a pack or table…”. A plain <code>pack.json</code> (e.g. from a forum or the community repository) opens the
          same way. So does a Cheat Engine table (<code>.CT</code>): its number values (health, money…) become a pack;
          its scripts are left out, because OpenTrainer never runs code from a file.
        </p>
        {packs === null && <p className="text-sm text-zinc-500">Loading…</p>}
        <ul className="flex flex-col gap-2">
          {(packs ?? []).map((p) => (
            <li key={p.key} className="flex flex-wrap items-center gap-3 rounded-xl bg-zinc-900/70 px-3 py-2 ring-1 ring-zinc-800" data-testid={`pack-${p.key}`}>
              <div className="min-w-0 flex-1">
                <p className="text-sm">
                  {p.name ?? p.key}{" "}
                  <span className="text-xs text-zinc-500">
                    {p.packVersion && `v${p.packVersion} · `}
                    {p.cheats} cheat{p.cheats === 1 ? "" : "s"} · {PACK_SOURCE_LABEL[p.source]}
                    {p.exe && ` · ${p.exe}`}
                  </span>
                  {p.ai && <span className="ml-2 rounded bg-violet-900/60 px-1.5 py-0.5 text-[10px] text-violet-200">AI-made</span>}
                </p>
                {p.error && <p className="text-xs text-red-400">{p.error}</p>}
              </div>
              {(p.source === "user" || p.source === "imported") && (
                <button
                  className={primary}
                  disabled={Boolean(p.error)}
                  aria-label={`Share ${p.name ?? p.key} with everyone`}
                  onClick={() => {
                    setMessage(null);
                    setSharing({ pack: p, mode: "pack" });
                  }}
                >
                  Share with everyone
                </button>
              )}
              {p.tableLink && (
                <button
                  className={button}
                  disabled={Boolean(p.error)}
                  aria-label={`Share the table link for ${p.name ?? p.key}`}
                  title="Add a link to the table's page to the community list (the table itself isn't shared)"
                  onClick={() => {
                    setMessage(null);
                    setSharing({ pack: p, mode: "link" });
                  }}
                >
                  Share the table&apos;s link
                </button>
              )}
              <button className={button} disabled={Boolean(p.error)} aria-label={`Export ${p.name ?? p.key}`} onClick={() => void exportPack(p)}>
                Export
              </button>
              {p.source !== "bundled" &&
                (confirmRemove === p.key ? (
                  <span className="flex items-center gap-2 text-xs">
                    Remove it? (a copy is kept)
                    <button className="rounded-lg bg-red-800 px-2 py-1 hover:bg-red-700" onClick={() => void remove(p)}>
                      Remove
                    </button>
                    <button className={button} onClick={() => setConfirmRemove(null)}>
                      Keep
                    </button>
                  </span>
                ) : (
                  <button className={button} aria-label={`Remove ${p.name ?? p.key}`} onClick={() => setConfirmRemove(p.key)}>
                    Remove
                  </button>
                ))}
            </li>
          ))}
        </ul>
      </section>

      <section className="flex flex-col gap-2 border-t border-zinc-900 pt-4" aria-label="Community packs">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="text-lg font-semibold tracking-tight">Community packs</h2>
          {community?.updated && <span className="text-xs text-zinc-500">list from {community.updated}</span>}
          <input
            aria-label="Find a community pack"
            className="ml-auto w-56 rounded-lg bg-zinc-900 px-2 py-1 text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
            placeholder="Find a game…"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
          <button
            className={button}
            disabled={loadingCommunity}
            onClick={() => {
              setLoadingCommunity(true);
              loadCommunity(true);
            }}
          >
            Refresh
          </button>
          <button
            className={button}
            onClick={() =>
              setMessage({
                text: (packs ?? []).some((p) => p.source === "user" || p.source === "imported")
                  ? "Click “Share with everyone” next to one of your packs above. OpenTrainer does the rest."
                  : "Make a pack first (with the AI helper, or open a Cheat Engine table), then click “Share with everyone” next to it.",
              })
            }
          >
            Share your pack
          </button>
        </div>
        <p className="text-xs text-zinc-500">
          Packs made by other players. Each one is checked like any pack before it's installed, and OpenTrainer still refuses
          games with anti-cheat.
        </p>
        {loadingCommunity && !community && <p className="text-sm text-zinc-500">Loading the list…</p>}
        {communityError && (
          <p className="rounded-lg bg-amber-950/40 px-3 py-2 text-sm text-amber-200" role="status">
            Couldn't load the community list: {communityError}
          </p>
        )}
        {community && community.packs.length === 0 && <p className="text-sm text-zinc-400">No packs on the list yet.</p>}
        <ul className="flex flex-col gap-2">
          {shown.map((p) => (
            <CommunityRow key={p.id} pack={p} busy={busy} onPreview={() => void preview({ kind: "community", id: p.id })} />
          ))}
        </ul>
      </section>
    </main>
  );
}

const STATE_TEXT: Record<CommunityPack["installed"], string | null> = {
  none: null,
  installed: "Installed",
  update: "Update available",
  other: "You have another pack for this game",
};

function CommunityRow({ pack, busy, onPreview }: { pack: CommunityPack; busy: boolean; onPreview: () => void }) {
  const state = STATE_TEXT[pack.installed];
  return (
    <li className="flex flex-wrap items-center gap-3 rounded-xl bg-zinc-900/70 px-3 py-2 ring-1 ring-zinc-800" data-testid={`community-${pack.id}`}>
      <div className="min-w-0 flex-1">
        <p className="text-sm">
          {pack.name}{" "}
          <span className="text-xs text-zinc-500">
            v{pack.packVersion} · {pack.cheats} cheat{pack.cheats === 1 ? "" : "s"} · {pack.exe} · by {pack.author}
          </span>
          {pack.author.trim().toLowerCase() === "ai" && (
            <span className="ml-2 rounded bg-violet-900/60 px-1.5 py-0.5 text-[10px] text-violet-200">AI-made</span>
          )}
        </p>
        {pack.notes && <p className="text-xs text-zinc-500">{pack.notes}</p>}
        {state && <p className={`text-xs ${pack.installed === "installed" ? "text-emerald-400" : "text-sky-300"}`}>{state}</p>}
      </div>
      <button className={pack.installed === "installed" ? button : primary} disabled={busy} aria-label={`See ${pack.name}`} onClick={onPreview}>
        {pack.installed === "update" ? "See the update" : "See what's inside"}
      </button>
    </li>
  );
}

interface DialogProps {
  preview: PackPreview;
  fromCommunity: boolean;
  busy: boolean;
  onInstall: () => void;
  onCancel: () => void;
  /** Set for a Cheat Engine table: lets the player name the game's exe. */
  onChooseExe?: () => void;
  /** Set for a Cheat Engine table: the maker's credit the player types. */
  credit?: Credit;
  onCredit?: (c: Credit) => void;
}

const field = "w-full rounded bg-zinc-900 px-2 py-1 text-xs outline-none ring-1 ring-zinc-800 focus:ring-emerald-600";

/** What a pack does, before it's installed. */
export function InstallDialog({ preview: p, fromCommunity, busy, onInstall, onCancel, onChooseExe, credit, onCredit }: DialogProps) {
  const replaces = p.existing && !p.existing.same ? p.existing : null;
  const skipped = p.skipped ?? [];
  const empty = p.cheats.length === 0;
  const canInstall = !p.needsExe && !empty;
  return (
    <div className="flex flex-col gap-3 rounded-lg border border-emerald-700 bg-zinc-950 p-4" role="dialog" aria-label="Install pack">
      <div>
        <p className="font-medium">
          Install the pack for {p.game}?
          {p.ai && <span className="ml-2 rounded bg-violet-900/60 px-1.5 py-0.5 text-[10px] text-violet-200">AI-made</span>}
        </p>
        <p className="text-xs text-zinc-400">
          For <code>{p.needsExe ? "?" : p.exe}</code> · version {p.version} · by {p.author}
          {fromCommunity ? " · from the community list" : ""}
        </p>
        {p.notes && <p className="mt-1 text-xs text-zinc-400">{p.notes}</p>}
        {(p.source?.by || p.source?.url) && (
          <p className="mt-1 text-xs text-zinc-400" data-testid="source">
            Cheats made by {p.source.by ?? "their maker"}
            {p.source.url && (
              <>
                {" · "}
                <span className="select-all break-all text-zinc-300">{p.source.url}</span>
              </>
            )}
          </p>
        )}
      </div>
      {credit && onCredit && (
        <fieldset className="flex flex-col gap-1.5 rounded bg-zinc-900/70 p-2 text-xs ring-1 ring-zinc-800">
          <legend className="px-1 text-zinc-300">Credit the table&apos;s maker (kept with the pack and shown when it&apos;s shared)</legend>
          <label className="flex flex-col gap-0.5">
            <span className="text-zinc-400">Who made it (their name on the site)</span>
            <input className={field} maxLength={100} value={credit.by ?? ""} onChange={(e) => onCredit({ ...credit, by: e.target.value })} />
          </label>
          <label className="flex flex-col gap-0.5">
            <span className="text-zinc-400">Its page (copy the address from your browser)</span>
            <input
              className={field}
              maxLength={400}
              placeholder="https://…"
              value={credit.url ?? ""}
              onChange={(e) => onCredit({ ...credit, url: e.target.value })}
            />
          </label>
        </fieldset>
      )}
      {p.needsExe && !empty && (
        <div className="flex flex-wrap items-center gap-2 rounded bg-amber-950/40 px-2 py-1.5 text-xs text-amber-200" role="status">
          <span className="flex-1">This table doesn&apos;t say which game it&apos;s for. Choose the game&apos;s exe to install it.</span>
          {onChooseExe && (
            <button className={primary} onClick={onChooseExe}>
              Choose the game&apos;s exe…
            </button>
          )}
        </div>
      )}
      {!p.needsExe && onChooseExe && !empty && (
        <p className="text-xs text-zinc-500">
          Wrong game?{" "}
          <button className="text-emerald-300 underline hover:text-emerald-200" onClick={onChooseExe}>
            Choose its exe
          </button>
        </p>
      )}
      {empty && (
        <p className="rounded bg-amber-950/40 px-2 py-1.5 text-xs text-amber-200" role="status">
          Nothing in this table can be used by OpenTrainer: everything in it is a script or something only Cheat Engine
          understands (see below).
        </p>
      )}
      <ul className="flex max-h-72 flex-col gap-1 overflow-y-auto text-sm">
        {p.cheats.map((c, i) => (
          <li key={`${i}-${c.name}`}>
            {c.name}
            {c.patch && <span className="ml-2 text-[10px] text-amber-300">changes game code (put back when turned off)</span>}
            {c.description && <span className="block text-xs text-zinc-500">{c.description}</span>}
          </li>
        ))}
      </ul>
      {skipped.length > 0 && (
        <details className="text-xs text-zinc-400" open={empty}>
          <summary>
            {skipped.length} {skipped.length === 1 ? "entry" : "entries"} not imported
          </summary>
          <ul className="mt-1 max-h-48 list-disc overflow-y-auto pl-5">
            {skipped.map((s, i) => (
              <li key={`${i}-${s.name}`}>
                {s.name}: {s.why}
              </li>
            ))}
          </ul>
        </details>
      )}
      {p.warnings.length > 0 && (
        <details className="text-xs text-amber-300">
          <summary>{p.warnings.length} warning{p.warnings.length === 1 ? "" : "s"}</summary>
          <ul className="mt-1 list-disc pl-5">
            {p.warnings.map((w) => (
              <li key={w.path + w.message}>{w.message}</li>
            ))}
          </ul>
        </details>
      )}
      {replaces && (
        <p className="rounded bg-amber-950/40 px-2 py-1 text-xs text-amber-200">
          This replaces your pack for this game: {replaces.name} v{replaces.version}
          {replaces.ai ? " (made by the AI helper)" : ""}. The old one is kept as a backup next to it.
        </p>
      )}
      {p.existing?.same && <p className="text-xs text-emerald-300">You already have this exact pack.</p>}
      <div className="flex gap-2">
        {!p.existing?.same && (
          <button className={primary} disabled={busy || !canInstall} onClick={onInstall}>
            {replaces ? "Replace and install" : "Install"}
          </button>
        )}
        <button className={button} onClick={onCancel}>
          {p.existing?.same ? "Close" : "Cancel"}
        </button>
      </div>
    </div>
  );
}
