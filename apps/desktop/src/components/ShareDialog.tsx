import { useEffect, useState } from "react";
import {
  api,
  type DeviceCode,
  type GithubStatus,
  type PackSummary,
  type SharedPack,
  TABLE_AUTHOR,
  toCommandError,
} from "../lib/ipc";

const button = "rounded-lg bg-zinc-800 px-3 py-1.5 text-xs hover:bg-zinc-700 disabled:cursor-not-allowed disabled:opacity-40";
const primary =
  "btn-accent rounded-lg px-4 py-2 text-sm font-semibold disabled:cursor-not-allowed disabled:opacity-40";

interface Props {
  pack: PackSummary;
  onClose: () => void;
}

/**
 * Shares one of the player's packs with the community list. OpenTrainer does
 * the GitHub part; the player only signs in once with a code.
 */
export function ShareDialog({ pack, onClose }: Props) {
  const [status, setStatus] = useState<GithubStatus | null>(null);
  const [code, setCode] = useState<DeviceCode | null>(null);
  const [copied, setCopied] = useState(false);
  const [note, setNote] = useState("");
  const [tested, setTested] = useState(false);
  const [tableOk, setTableOk] = useState(false);
  const [sharing, setSharing] = useState(false);
  const [shared, setShared] = useState<SharedPack | null>(null);
  const [error, setError] = useState<string | null>(null);
  const name = pack.name ?? pack.id ?? "this pack";
  const fromTable = pack.author === TABLE_AUTHOR;

  useEffect(() => {
    api.githubStatus().then(setStatus, (e: unknown) => setError(toCommandError(e).message));
  }, []);

  async function signIn() {
    setError(null);
    try {
      const c = await api.githubSignIn();
      setCode(c);
      const login = await api.githubSignInWait();
      setCode(null);
      setStatus({ configured: true, signedIn: true, login });
    } catch (e) {
      const err = toCommandError(e);
      setCode(null);
      if (err.kind !== "cancelled") setError(err.message);
    }
  }

  async function copyAndOpen() {
    if (!code) return;
    try {
      await navigator.clipboard.writeText(code.userCode);
      setCopied(true);
    } catch {
      // The code is on screen; typing it works too.
    }
    await api.githubOpenDevicePage().catch((e: unknown) => setError(toCommandError(e).message));
  }

  async function share() {
    setError(null);
    setSharing(true);
    try {
      setShared(await api.communityShare(pack.key, note, tableOk));
    } catch (e) {
      const err = toCommandError(e);
      if (err.kind === "signedOut") setStatus((s) => (s ? { ...s, signedIn: false, login: null } : s));
      setError(err.message);
    } finally {
      setSharing(false);
    }
  }

  async function signOut() {
    await api.githubSignOut().catch(() => undefined);
    setStatus((s) => (s ? { ...s, signedIn: false, login: null } : s));
  }

  function close() {
    if (code) void api.githubSignInCancel().catch(() => undefined);
    onClose();
  }

  return (
    <div
      className="flex flex-col gap-3 rounded-2xl bg-zinc-900/90 p-5 ring-1 ring-emerald-800/70"
      role="dialog"
      aria-label="Share with the community"
    >
      <div>
        <p className="text-lg font-semibold tracking-tight">Share “{name}” with everyone</p>
        <p className="text-xs text-zinc-400">
          OpenTrainer puts your pack on the community list for you. A maintainer checks every pack before it shows up.
        </p>
      </div>

      {error && (
        <p className="rounded-lg bg-red-950/40 px-3 py-2 text-sm text-red-300" role="alert">
          {error}
        </p>
      )}

      {!status && !error && <p className="text-sm text-zinc-400">Checking…</p>}

      {status && !status.configured && (
        <div className="flex flex-col gap-2 text-sm text-zinc-300">
          <p>Sharing from the app isn&apos;t set up in this version yet. You can still share the GitHub way.</p>
          <button className={`${button} self-start`} onClick={() => void api.openLink("contribute").catch(() => undefined)}>
            Show me how
          </button>
        </div>
      )}

      {status?.configured && !status.signedIn && !code && (
        <div className="flex flex-col gap-2 text-sm text-zinc-300">
          <p>
            You need a free GitHub account (the site where the community list lives). You sign in once; OpenTrainer does
            the rest. It never sees your password.
          </p>
          <button className={`${primary} self-start`} onClick={() => void signIn()}>
            Sign in with GitHub
          </button>
        </div>
      )}

      {code && (
        <div className="flex flex-col gap-3 text-sm text-zinc-300" role="status">
          <p>1. Your code:</p>
          <p className="select-all font-mono text-3xl font-semibold tracking-[0.2em] text-white" data-testid="device-code">
            {code.userCode}
          </p>
          <p>2. Paste it on GitHub&apos;s page, then click Authorize.</p>
          <button className={`${primary} self-start`} onClick={() => void copyAndOpen()}>
            {copied ? "Copied. Open GitHub again" : "Copy the code and open GitHub"}
          </button>
          <p className="text-xs text-zinc-500">Waiting for you to finish on GitHub…</p>
        </div>
      )}

      {status?.configured && status.signedIn && !shared && (
        <div className="flex flex-col gap-3 text-sm">
          <label className="flex flex-col gap-1">
            Which version of the game did you test it on? (optional)
            <input
              aria-label="Tested on"
              className="rounded-lg bg-zinc-950 px-3 py-1.5 text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
              placeholder="e.g. Steam, version 1.2"
              maxLength={500}
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />
          </label>
          <label className="flex items-start gap-2">
            <input type="checkbox" className="mt-1" checked={tested} onChange={(e) => setTested(e.target.checked)} />
            I tried every cheat in this pack and they work. It&apos;s for a single-player game.
          </label>
          {fromTable && (
            <label className="flex items-start gap-2">
              <input type="checkbox" className="mt-1" checked={tableOk} onChange={(e) => setTableOk(e.target.checked)} />
              This pack came from a Cheat Engine table. I made that table, or its author says it may be shared.
            </label>
          )}
          <div className="flex flex-wrap items-center gap-2">
            <button
              className={primary}
              disabled={sharing || !tested || (fromTable && !tableOk)}
              onClick={() => void share()}
            >
              {sharing ? "Sharing…" : "Share"}
            </button>
            <button className={button} onClick={close} disabled={sharing}>
              Cancel
            </button>
            <span className="ml-auto text-xs text-zinc-500">
              Signed in to GitHub{status.login ? ` as ${status.login}` : ""} ·{" "}
              <button className="underline hover:text-zinc-300" onClick={() => void signOut()} disabled={sharing}>
                Sign out
              </button>
            </span>
          </div>
          {sharing && (
            <p className="text-xs text-zinc-500">Making your copy of the list, adding the pack and sending it…</p>
          )}
        </div>
      )}

      {shared && (
        <div className="flex flex-col gap-2 text-sm" role="status">
          <p className="text-emerald-300">
            Sent! A maintainer will check it and add it to the list, usually within a few days.
            {shared.update && " It replaces the pack the list already has for this game."}
          </p>
          <div className="flex gap-2">
            <button className={button} onClick={() => void api.githubOpenPr(shared.url).catch(() => undefined)}>
              See it on GitHub
            </button>
            <button className={button} onClick={onClose}>
              Close
            </button>
          </div>
        </div>
      )}

      {!shared && (!status?.signedIn || !status.configured) && (
        <button className={`${button} self-start`} onClick={close}>
          {code ? "Cancel" : "Close"}
        </button>
      )}
    </div>
  );
}
