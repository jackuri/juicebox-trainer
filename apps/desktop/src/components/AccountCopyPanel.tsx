import { useEffect, useState } from "react";
import { accountLabel, fileSize } from "../lib/format";
import {
  api,
  type AccountsView,
  type CopyPlan,
  type CopyReport,
  } from "../lib/ipc";
import { friendlyError } from "../lib/explain";

const button = "rounded bg-zinc-800 px-2 py-1 text-xs hover:bg-zinc-700 disabled:opacity-40";
const select = "rounded border border-zinc-700 bg-zinc-900 px-2 py-1 text-sm";

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

interface Props {
  gameId: string;
  running: boolean;
  /** Changes when the game's saves or folders may have changed. */
  reloadKey: string;
  onCopied: () => void;
}

/**
 * Copies a game's saves from one of the player's Steam accounts to another,
 * changing the Steam ID inside them. Shown only when the game keeps saves per
 * Steam account and the PC knows another account.
 */
export function AccountCopyPanel({ gameId, running, reloadKey, onCopied }: Props) {
  const [view, setView] = useState<AccountsView | null>(null);
  const [from, setFrom] = useState("");
  const [toChoice, setTo] = useState("");
  const [plan, setPlan] = useState<CopyPlan | null>(null);
  const [report, setReport] = useState<CopyReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    api.savesAccounts(gameId).then(
      (v) => {
        if (!live || !v) return;
        setView(v);
        const first = [...v.withSaves].sort((a, b) => b.files - a.files)[0];
        setFrom((f) => (v.withSaves.some((s) => s.account.id64 === f) ? f : (first?.account.id64 ?? "")));
      },
      () => live && setView(null),
    );
    return () => {
      live = false;
    };
  }, [gameId, reloadKey]);

  const targets = view?.accounts.filter((a) => a.id64 !== from) ?? [];
  // The chosen account, or the first other one.
  const to = targets.some((a) => a.id64 === toChoice) ? toChoice : (targets[0]?.id64 ?? "");

  if (!view || view.withSaves.length === 0 || view.accounts.length < 2) return null;

  async function run(action: () => Promise<void>) {
    setError(null);
    setBusy(true);
    try {
      await action();
    } catch (e) {
      setError(friendlyError(e));
    } finally {
      setBusy(false);
    }
  }

  const check = () =>
    run(async () => {
      setReport(null);
      setPlan(await api.savesAccountPlan(gameId, from, to));
    });

  const copy = () =>
    run(async () => {
      const r = await api.savesAccountCopy(gameId, from, to);
      setPlan(null);
      setReport(r);
      onCopied();
    });

  const toAccount = view.accounts.find((a) => a.id64 === to);
  const replacing = plan?.files.filter((f) => f.replaces).length ?? 0;
  const withIds = plan?.files.filter((f) => f.ids > 0).length ?? 0;

  return (
    <section className="flex flex-col gap-2" aria-label="Copy to another Steam account">
      <h3 className="text-sm font-medium text-zinc-300">Copy saves to another Steam account</h3>
      <p className="text-xs text-zinc-500">
        This game keeps saves for each Steam account. You can copy one account&apos;s saves to
        another of your accounts on this PC. Your Steam ID inside the saves is changed to the other
        account&apos;s, so the game accepts them. The saves you copy from aren&apos;t changed, and
        everything is backed up first.
      </p>
      <div className="flex flex-wrap items-center gap-2 text-sm">
        <label className="flex items-center gap-1">
          From
          <select
            aria-label="Copy from account"
            className={select}
            value={from}
            onChange={(e) => {
              setFrom(e.target.value);
              setPlan(null);
            }}
          >
            {view.withSaves.map((s) => (
              <option key={s.account.id64} value={s.account.id64}>
                {accountLabel(s.account)} — {s.files} file{s.files === 1 ? "" : "s"},{" "}
                {fileSize(s.bytes)}
              </option>
            ))}
          </select>
        </label>
        <label className="flex items-center gap-1">
          to
          <select
            aria-label="Copy to account"
            className={select}
            value={to}
            onChange={(e) => {
              setTo(e.target.value);
              setPlan(null);
            }}
          >
            {targets.map((a) => (
              <option key={a.id64} value={a.id64}>
                {accountLabel(a)}
                {view.withSaves.some((s) => s.account.id64 === a.id64) ? " (has saves)" : ""}
              </option>
            ))}
          </select>
        </label>
        <button className={button} disabled={busy || !from || !to} onClick={() => void check()}>
          Check what will be copied
        </button>
      </div>

      {error && (
        <p className="rounded-lg bg-red-950/40 px-3 py-2 text-sm text-red-300" role="alert">
          {error}
        </p>
      )}

      {plan && (
        <div className="flex flex-col gap-2 rounded-lg bg-zinc-900 px-3 py-2 text-sm" aria-label="Copy preview">
          <p>
            {plan.files.length} file{plan.files.length === 1 ? "" : "s"} will be copied to{" "}
            {accountLabel(plan.to)}.
            {withIds > 0 &&
              ` Your Steam ID is changed in ${withIds} of them.`}
          </p>
          {replacing > 0 && (
            <p className="text-amber-300">
              {accountLabel(plan.to)} already has {replacing} of these file
              {replacing === 1 ? "" : "s"}; they will be replaced. The old ones stay in the backup
              marked &quot;Before copying to another account&quot;.
            </p>
          )}
          <ul className="max-h-40 overflow-y-auto font-mono text-[11px] text-zinc-400">
            {plan.files.map((f) => (
              <li key={f.to}>
                {fileName(f.from)}
                {fileName(f.to) !== fileName(f.from) && ` → ${fileName(f.to)}`} ({fileSize(f.size)})
                {f.replaces && <span className="text-amber-300"> replaces a file</span>}
                {f.ids > 0 && <span className="text-sky-300"> Steam ID changed</span>}
              </li>
            ))}
          </ul>
          {plan.cloudWarning && <p className="text-xs text-sky-200">{plan.cloudWarning}</p>}
          <div className="flex gap-2">
            <button
              className="rounded-lg btn-accent px-3 py-1.5 text-sm disabled:opacity-50"
              disabled={busy || running}
              title={running ? "Close the game first" : undefined}
              onClick={() => void copy()}
            >
              Copy to {toAccount ? accountLabel(toAccount) : "that account"}
            </button>
            <button className={button} onClick={() => setPlan(null)}>
              Cancel
            </button>
          </div>
          {running && <p className="text-xs text-amber-300">Close the game first.</p>}
        </div>
      )}

      {report && (
        <div className="rounded-lg bg-emerald-950/40 px-3 py-2 text-sm text-emerald-200" role="status">
          <p>
            Copied {report.copied} file{report.copied === 1 ? "" : "s"}
            {report.replaced > 0 && ` (${report.replaced} replaced)`}. Sign in to Steam with the
            other account and start the game to play with these saves.
            {report.copied > 0 &&
              " The saves from just before are in the backup marked “Before copying to another account”."}
          </p>
          {report.cloudWarning && <p className="mt-1 text-xs text-sky-200">{report.cloudWarning}</p>}
          <button className={`${button} mt-2`} onClick={() => setReport(null)}>
            OK
          </button>
        </div>
      )}
    </section>
  );
}
