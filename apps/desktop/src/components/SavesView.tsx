import { useCallback, useEffect, useState } from "react";
import { backupReason, fileSize, isSystemProcess, timeAgo } from "../lib/format";
import {
  api,
  type Backup,
  type EditReport,
  onSavesChanged,
  type ProcessInfo,
  type QuickCode,
  type RestoreReport,
  type SaveDiff,
  type SaveFile,
  type SaveGame,
  type SavesOverview,
  toCommandError,
} from "../lib/ipc";
import { SaveDiffView } from "./SaveDiffView";
import { SaveEditor } from "./SaveEditor";

const button = "rounded bg-zinc-800 px-2 py-1 text-xs hover:bg-zinc-700 disabled:opacity-40";
const input =
  "min-w-0 flex-1 rounded border border-zinc-700 bg-zinc-900 px-2 py-1 text-sm placeholder:text-zinc-600";

function when(ms: number): string {
  return new Date(ms).toLocaleString();
}

interface Props {
  processes: ProcessInfo[];
  onRefreshProcesses: () => void;
  /** Switches to the AI helper view (after starting "explain a save"). */
  onOpenAi?: () => void;
}

/** Save backups: games, their save folders, backups, compare and restore. */
export function SavesView({ processes, onRefreshProcesses, onOpenAi }: Props) {
  const [overview, setOverview] = useState<SavesOverview | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const [pid, setPid] = useState("");
  const [exePath, setExePath] = useState("");

  const apply = useCallback((o: SavesOverview) => {
    setOverview(o);
    setSelectedId((id) => (id && o.games.some((g) => g.id === id) ? id : (o.games[0]?.id ?? null)));
  }, []);

  const reload = useCallback(() => {
    api.savesOverview().then(apply, (e: unknown) => setError(toCommandError(e).message));
    setReloadKey((k) => k + 1);
  }, [apply]);

  useEffect(() => {
    api.savesOverview().then(apply, (e: unknown) => setError(toCommandError(e).message));
    return onSavesChanged(reload);
  }, [apply, reload]);

  async function browseExe() {
    setError(null);
    try {
      const picked = await api.pickExe("Pick the game's .exe");
      if (picked) await add(() => api.savesAddExe(picked));
    } catch (e) {
      setError(toCommandError(e).message);
    }
  }

  async function add(how: () => Promise<SaveGame>) {
    setError(null);
    try {
      const g = await how();
      setSelectedId(g.id);
      setPid("");
      setExePath("");
      reload();
    } catch (e) {
      setError(toCommandError(e).message);
    }
  }

  const candidates = processes.filter((p) => !isSystemProcess(p));
  const selected = overview?.games.find((g) => g.id === selectedId) ?? null;

  return (
    <div className="grid min-h-0 grid-cols-[300px_1fr]">
      <aside className="flex min-h-0 flex-col gap-3 overflow-y-auto border-r border-zinc-800 p-3">
        <h2 className="text-[11px] font-semibold uppercase tracking-[0.14em] text-zinc-400">Games</h2>
        {overview && overview.games.length === 0 && (
          <p className="text-xs text-zinc-500">
            No games yet. Add one below, or attach to a game: its saves are backed up automatically.
          </p>
        )}
        <ul className="flex flex-col gap-1" aria-label="Games with backups">
          {overview?.games.map((g) => (
            <li key={g.id}>
              <button
                className={`w-full rounded px-2 py-1.5 text-left ${
                  g.id === selectedId ? "bg-zinc-800" : "hover:bg-zinc-900"
                }`}
                aria-current={g.id === selectedId}
                onClick={() => setSelectedId(g.id)}
              >
                <span className="flex items-center gap-2 text-sm">
                  {g.name}
                  {g.running && (
                    <span className="rounded bg-emerald-900/60 px-1 text-[10px] text-emerald-300">
                      running
                    </span>
                  )}
                  {g.cloudSynced && (
                    <span className="rounded bg-sky-900/60 px-1 text-[10px] text-sky-300">
                      Steam Cloud
                    </span>
                  )}
                </span>
                <span className="block text-xs text-zinc-500">
                  {g.backups} backup{g.backups === 1 ? "" : "s"}
                  {g.latestMs !== null && ` · last ${timeAgo(g.latestMs)}`}
                </span>
              </button>
            </li>
          ))}
        </ul>

        <div className="mt-2 flex flex-col gap-2 border-t border-zinc-800 pt-3">
          <h3 className="text-xs font-medium text-zinc-400">Add a game</h3>
          <div className="flex gap-1">
            <select
              aria-label="Running program"
              className={input}
              value={pid}
              onChange={(e) => setPid(e.target.value)}
              onFocus={onRefreshProcesses}
            >
              <option value="">Pick a running game…</option>
              {candidates.map((p) => (
                <option key={p.pid} value={p.pid}>
                  {p.name} ({p.pid})
                </option>
              ))}
            </select>
            <button
              className={button}
              disabled={!pid}
              onClick={() => void add(() => api.savesAddProcess(Number(pid)))}
            >
              Add
            </button>
          </div>
          <div className="flex gap-1">
            <button className={button} onClick={() => void browseExe()}>
              Browse…
            </button>
            <input
              aria-label="Game exe path"
              className={input}
              placeholder="or paste the game's .exe path"
              value={exePath}
              onChange={(e) => setExePath(e.target.value)}
            />
            <button
              className={button}
              disabled={!exePath.trim()}
              onClick={() => void add(() => api.savesAddExe(exePath))}
            >
              Add
            </button>
          </div>
          {overview?.root && (
            <p className="break-all text-[11px] text-zinc-600">Backups are kept in {overview.root}</p>
          )}
        </div>
      </aside>

      <main className="flex min-h-0 flex-col gap-4 overflow-y-auto p-4">
        {error && (
          <p className="rounded-lg bg-red-950/40 px-3 py-2 text-sm text-red-300" role="alert">
            {error}
          </p>
        )}
        {selected ? (
          <GamePanel
            key={selected.id}
            game={selected}
            steamRunning={overview?.steamRunning ?? false}
            reloadKey={reloadKey}
            onChanged={reload}
            onOpenAi={onOpenAi}
          />
        ) : (
          <div className="m-auto max-w-sm text-center text-sm text-zinc-400">
            <p>OpenTrainer backs up a game's saves when the game starts and when it closes.</p>
            <p className="mt-2 text-xs text-zinc-500">Add a game on the left to begin.</p>
          </div>
        )}
      </main>
    </div>
  );
}

interface GameProps {
  game: SaveGame;
  steamRunning: boolean;
  reloadKey: number;
  onChanged: () => void;
  onOpenAi?: () => void;
}

function GamePanel({ game, steamRunning, reloadKey, onChanged, onOpenAi }: GameProps) {
  const [backups, setBackups] = useState<Backup[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [note, setNote] = useState("");
  const [folder, setFolder] = useState("");
  const [busy, setBusy] = useState(false);
  const [diff, setDiff] = useState<{ title: string; diff: SaveDiff; from: string; toNow: boolean } | null>(
    null,
  );
  const [restoring, setRestoring] = useState<Backup | null>(null);
  const [removeExtra, setRemoveExtra] = useState(false);
  const [report, setReport] = useState<RestoreReport | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [removing, setRemoving] = useState(false);
  const [deleteBackups, setDeleteBackups] = useState(false);
  const [files, setFiles] = useState<SaveFile[]>([]);
  const [codes, setCodes] = useState<QuickCode[]>([]);
  const [editing, setEditing] = useState<SaveFile | null>(null);

  useEffect(() => {
    api.savesBackups(game.id).then(setBackups, (e: unknown) => setError(toCommandError(e).message));
    api.savesFiles(game.id).then(setFiles, () => setFiles([]));
    api.savesQuickCodes(game.id).then(setCodes, () => setCodes([]));
  }, [game.id, game.backups, game.folders, reloadKey]);

  function edited(r: EditReport) {
    if (r.cloudWarning) setMessage(r.cloudWarning);
    onChanged();
  }

  const applyCode = (c: QuickCode) =>
    run(async () => {
      const r = await api.savesApplyQuickCode(game.id, c.packKey, c.id);
      setMessage(
        r.changed
          ? `${c.name}: done. Your saves from just before are in the backup marked "Before an edit".`
          : `${c.name}: the save already had that value.`,
      );
      if (r.cloudWarning) setMessage((m) => `${m ?? ""} ${r.cloudWarning}`);
      onChanged();
    });

  async function run(action: () => Promise<void>) {
    setError(null);
    setMessage(null);
    setBusy(true);
    try {
      await action();
    } catch (e) {
      setError(toCommandError(e).message);
    } finally {
      setBusy(false);
    }
  }

  const backupNow = () =>
    run(async () => {
      await api.savesBackupNow(game.id, note);
      setNote("");
      setMessage("Backed up.");
      onChanged();
    });

  const compare = (from: Backup, to: Backup | null) =>
    run(async () => {
      const d = await api.savesDiff(game.id, from.id, to?.id ?? null);
      setDiff({
        title: to
          ? `What changed from ${when(from.createdMs)} to ${when(to.createdMs)}`
          : `What changed since the backup from ${when(from.createdMs)}`,
        diff: d,
        from: from.id,
        toNow: to === null,
      });
    });

  const explain = (from: string) =>
    run(async () => {
      await api.aiStart({
        pid: 0,
        task: "explainSave",
        exe: game.exe,
        backup: from,
        wishes: [],
        notes: "",
        backend: "subscription",
        maxBudgetUsd: 1,
      });
      onOpenAi?.();
    });

  const restore = (b: Backup) =>
    run(async () => {
      const r = await api.savesRestore(game.id, b.id, removeExtra);
      setRestoring(null);
      setRemoveExtra(false);
      setReport(r);
      onChanged();
    });

  const remove = (b: Backup) =>
    run(async () => {
      await api.savesDeleteBackup(game.id, b.id);
      setDeleting(null);
      onChanged();
    });

  const addFolder = (path = folder) =>
    run(async () => {
      await api.savesAddFolder(game.id, path);
      setFolder("");
      onChanged();
    });

  const browseFolder = () =>
    run(async () => {
      const picked = await api.pickFolder(`Pick a save folder for ${game.name}`);
      if (!picked) return;
      await api.savesAddFolder(game.id, picked);
      setFolder("");
      onChanged();
    });

  return (
    <div className="flex max-w-4xl flex-col gap-4">
      <div className="flex flex-wrap items-start gap-3">
        <div className="min-w-0">
          <h2 className="text-2xl font-semibold tracking-tight">{game.name}</h2>
          <p className="truncate text-xs text-zinc-500">{game.exe}</p>
        </div>
        <div className="ml-auto flex gap-2">
          <button className={button} disabled={busy} onClick={() => void run(async () => {
            await api.savesRelocate(game.id);
            onChanged();
          })}>
            Find save folders again
          </button>
          <button className={button} disabled={busy} onClick={() => setRemoving(true)}>
            Remove game
          </button>
        </div>
      </div>

      {removing && (
        <div className="flex flex-wrap items-center gap-3 rounded-lg bg-zinc-900 px-3 py-2 text-sm">
          <span>Stop backing up {game.name}?</span>
          <label className="flex items-center gap-1 text-xs">
            <input
              type="checkbox"
              checked={deleteBackups}
              onChange={(e) => setDeleteBackups(e.target.checked)}
            />
            Also delete its {game.backups} backup{game.backups === 1 ? "" : "s"} (can't be undone)
          </label>
          <button
            className={button}
            onClick={() => void run(async () => {
              await api.savesRemoveGame(game.id, deleteBackups);
              onChanged();
            })}
          >
            Remove
          </button>
          <button className={button} onClick={() => setRemoving(false)}>
            Cancel
          </button>
        </div>
      )}

      {game.running && (
        <p className="rounded-lg bg-emerald-950/40 px-3 py-2 text-sm text-emerald-200" role="status">
          {game.name} is running. Its saves are backed up when it starts and when it closes. Close the
          game to restore a backup.
        </p>
      )}
      {game.cloudSynced && (
        <p className="rounded-lg bg-sky-950/40 px-3 py-2 text-sm text-sky-200" role="note">
          Steam Cloud syncs this game&apos;s saves. Restore only while the game is closed. If Steam
          says the cloud copy is different when you start the game, choose the files on this PC
          (local), or Steam may put the other save back.
          {steamRunning && " Steam is running now."}
        </p>
      )}
      {error && (
        <p className="rounded-lg bg-red-950/40 px-3 py-2 text-sm text-red-300" role="alert">
          {error}
        </p>
      )}
      {message && <p className="text-sm text-emerald-300" role="status">{message}</p>}
      {report && (
        <div className="rounded-lg bg-emerald-950/40 px-3 py-2 text-sm text-emerald-200" role="status">
          <p>
            Restored. {report.written} file{report.written === 1 ? "" : "s"} put back
            {report.unchanged > 0 && `, ${report.unchanged} already the same`}
            {report.removed.length > 0 && `, ${report.removed.length} removed`}. Your saves from
            just before are in the backup marked &quot;Before a restore&quot;, so you can undo this.
          </p>
          {report.extra.length > 0 && (
            <p className="mt-1 text-xs">
              Left alone (not in that backup): {report.extra.join(", ")}
            </p>
          )}
          {report.cloudWarning && <p className="mt-1 text-xs text-sky-200">{report.cloudWarning}</p>}
          <button className={`${button} mt-2`} onClick={() => setReport(null)}>
            OK
          </button>
        </div>
      )}

      <section className="flex flex-col gap-2" aria-label="Save folders">
        <h3 className="text-sm font-medium text-zinc-300">Save folders</h3>
        {game.folders.length === 0 ? (
          <p className="text-sm text-amber-300">
            No save folder found. Add the folder where this game keeps its saves (often in
            Documents, Saved Games or AppData).
          </p>
        ) : (
          <ul className="flex flex-col gap-1">
            {game.folders.map((f) => (
              <li key={f.path} className="flex items-center gap-2 rounded bg-zinc-900 px-2 py-1">
                <div className="min-w-0 flex-1">
                  <p className="truncate font-mono text-xs">{f.path}</p>
                  <p className="text-[11px] text-zinc-500">{f.reason}</p>
                </div>
                {!/^HKCU\\/i.test(f.path) && (
                  <button
                    className={button}
                    onClick={() => void run(() => api.savesOpenFolder(game.id, f.path))}
                  >
                    Open
                  </button>
                )}
                <button
                  className={button}
                  aria-label={`Remove folder ${f.path}`}
                  onClick={() => void run(async () => {
                    await api.savesRemoveFolder(game.id, f.path);
                    onChanged();
                  })}
                >
                  Remove
                </button>
              </li>
            ))}
          </ul>
        )}
        <div className="flex gap-1">
          <button className={button} disabled={busy} onClick={() => void browseFolder()}>
            Browse for a folder…
          </button>
          <input
            aria-label="Folder to add"
            className={input}
            placeholder="or paste its full path"
            value={folder}
            onChange={(e) => setFolder(e.target.value)}
          />
          <button className={button} disabled={!folder.trim() || busy} onClick={() => void addFolder()}>
            Add folder
          </button>
        </div>
      </section>

      {codes.length > 0 && (
        <section className="flex flex-col gap-2" aria-label="Quick codes">
          <h3 className="text-sm font-medium text-zinc-300">Quick codes</h3>
          <p className="text-xs text-zinc-500">
            One-click save edits from cheat packs. Close the game first; your saves are backed up
            before each change.
          </p>
          <ul className="flex flex-wrap gap-2">
            {codes.map((c) => (
              <li key={`${c.packKey}|${c.id}`}>
                <button
                  className="rounded bg-zinc-800 px-3 py-1.5 text-sm hover:bg-zinc-700 disabled:opacity-40"
                  disabled={busy || game.running || c.problem !== null}
                  title={c.problem ?? (game.running ? "Close the game first" : `${c.path} = ${String(c.value)}`)}
                  onClick={() => void applyCode(c)}
                >
                  {c.name}
                </button>
                {c.problem && <p className="mt-0.5 max-w-xs text-[11px] text-zinc-500">{c.problem}</p>}
              </li>
            ))}
          </ul>
        </section>
      )}

      <section className="flex flex-col gap-2" aria-label="Edit saves">
        <h3 className="text-sm font-medium text-zinc-300">Edit saves</h3>
        {files.length === 0 ? (
          <p className="text-sm text-zinc-500">No save files in these folders yet.</p>
        ) : (
          <ul className="flex flex-col gap-1">
            {files.map((f) => (
              <li key={`${f.folder}|${f.path}`} className="flex items-center gap-2 rounded bg-zinc-900 px-2 py-1">
                <span className="min-w-0 flex-1 truncate font-mono text-xs">
                  {f.registry ? `${f.folder} (registry)` : f.path}
                </span>
                <span className="text-[11px] text-zinc-500">{fileSize(f.size)}</span>
                <button
                  className={button}
                  aria-label={`Open ${f.registry ? f.folder : f.path}`}
                  onClick={() => setEditing(f)}
                >
                  Open
                </button>
              </li>
            ))}
          </ul>
        )}
        {editing && (
          <SaveEditor
            key={`${editing.folder}|${editing.path}`}
            gameId={game.id}
            file={editing}
            running={game.running}
            onSaved={edited}
            onClose={() => setEditing(null)}
          />
        )}
      </section>

      <section className="flex flex-col gap-2" aria-label="Backups">
        <div className="flex flex-wrap items-center gap-2">
          <h3 className="text-sm font-medium text-zinc-300">Backups</h3>
          <input
            aria-label="Note for the backup"
            className={`${input} ml-auto max-w-xs`}
            placeholder="Note (optional), e.g. before the boss"
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
          <button
            className="rounded btn-accent px-3 py-1 text-sm font-semibold disabled:opacity-40"
            disabled={busy || game.folders.length === 0}
            onClick={() => void backupNow()}
          >
            Back up now
          </button>
        </div>
        {backups.length === 0 ? (
          <p className="text-sm text-zinc-500">No backups yet.</p>
        ) : (
          <table className="w-full text-left text-sm">
            <thead className="text-xs text-zinc-500">
              <tr>
                <th className="py-1 font-normal">When</th>
                <th className="py-1 font-normal">Why</th>
                <th className="py-1 font-normal">Files</th>
                <th className="py-1 font-normal">Note</th>
                <th className="py-1 font-normal" />
              </tr>
            </thead>
            <tbody>
              {backups.map((b, i) => {
                const previous = backups[i + 1] ?? null;
                return (
                  <tr key={b.id} className="border-t border-zinc-800 align-top">
                    <td className="py-1.5 pr-2">
                      {when(b.createdMs)}
                      <span className="block text-xs text-zinc-500">{timeAgo(b.createdMs)}</span>
                    </td>
                    <td className="py-1.5 pr-2 text-xs">{backupReason(b.reason)}</td>
                    <td className="py-1.5 pr-2 text-xs">
                      {b.files} · {fileSize(b.totalSize)}
                      {b.skipped.length > 0 && (
                        <span className="block text-amber-300">
                          {b.skipped.length} couldn&apos;t be read
                        </span>
                      )}
                    </td>
                    <td className="py-1.5 pr-2 text-xs text-zinc-300">{b.note}</td>
                    <td className="py-1.5">
                      <div className="flex flex-wrap justify-end gap-1">
                        <button className={button} disabled={busy} onClick={() => void compare(b, null)}>
                          Compare with now
                        </button>
                        {previous && (
                          <button
                            className={button}
                            disabled={busy}
                            onClick={() => void compare(previous, b)}
                          >
                            Compare with previous
                          </button>
                        )}
                        <button
                          className={button}
                          disabled={busy || game.running}
                          title={game.running ? "Close the game first" : undefined}
                          onClick={() => {
                            setRestoring(b);
                            setReport(null);
                          }}
                        >
                          Restore
                        </button>
                        {deleting === b.id ? (
                          <>
                            <button className={`${button} text-red-300`} onClick={() => void remove(b)}>
                              Delete for good
                            </button>
                            <button className={button} onClick={() => setDeleting(null)}>
                              Keep
                            </button>
                          </>
                        ) : (
                          <button className={button} disabled={busy} onClick={() => setDeleting(b.id)}>
                            Delete
                          </button>
                        )}
                      </div>
                      {restoring?.id === b.id && (
                        <div className="mt-2 flex flex-col gap-2 rounded bg-zinc-900 p-2 text-xs" role="dialog" aria-label="Restore backup">
                          <p>
                            Put back the saves from {when(b.createdMs)}? Your current saves are
                            backed up first, so you can undo this.
                          </p>
                          <label className="flex items-center gap-1">
                            <input
                              type="checkbox"
                              checked={removeExtra}
                              onChange={(e) => setRemoveExtra(e.target.checked)}
                            />
                            Also delete save files that aren&apos;t in this backup
                          </label>
                          <div className="flex gap-2">
                            <button
                              className="rounded btn-accent px-2 py-1"
                              disabled={busy}
                              onClick={() => void restore(b)}
                            >
                              Restore
                            </button>
                            <button className={button} onClick={() => setRestoring(null)}>
                              Cancel
                            </button>
                          </div>
                        </div>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </section>

      {diff && (
        <SaveDiffView
          title={diff.title}
          diff={diff.diff}
          onClose={() => setDiff(null)}
          onExplain={diff.toNow && onOpenAi ? () => void explain(diff.from) : undefined}
        />
      )}
    </div>
  );
}
