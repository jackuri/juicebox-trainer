import { useEffect, useState } from "react";
import { api, type DeviceCode, type GithubStatus, type SharedPack, toCommandError, type WebGame } from "../lib/ipc";
import { friendlyError } from "../lib/explain";

const button = "rounded-lg bg-zinc-800 px-3 py-1.5 text-xs hover:bg-zinc-700 disabled:cursor-not-allowed disabled:opacity-40";
const primary = "btn-accent rounded-lg px-4 py-2 text-sm font-semibold disabled:cursor-not-allowed disabled:opacity-40";
const field = "rounded-lg bg-zinc-950 px-3 py-1.5 text-sm outline-none ring-1 ring-zinc-800 focus:ring-emerald-600";

const onThisPc = (url: string) => {
  try {
    return ["localhost", "127.0.0.1", "[::1]"].includes(new URL(url).hostname);
  } catch {
    return false;
  }
};

/**
 * Shares the cheats saved for a web game (v1.19.0): as a file to give to friends, or on the community
 * list (OpenTrainer does the GitHub part, like for packs).
 */
export function WebShareDialog({ game, onClose }: { game: WebGame; onClose: () => void }) {
  const [by, setBy] = useState("");
  const [notes, setNotes] = useState("");
  const [saved, setSaved] = useState<string | null>(null);
  const [status, setStatus] = useState<GithubStatus | null>(null);
  const [code, setCode] = useState<DeviceCode | null>(null);
  const [tested, setTested] = useState(false);
  const [sharing, setSharing] = useState(false);
  const [shared, setShared] = useState<SharedPack | null>(null);
  const [error, setError] = useState<string | null>(null);
  const local = onThisPc(game.url);

  useEffect(() => {
    // The name the player signed the agreement with is a good guess for who found the cheats.
    api.agreementStatus().then(
      (s) => setBy((b) => b || s?.signed?.name || ""),
      () => undefined,
    );
  }, []);

  const note = notes.trim() || null;

  async function save() {
    setError(null);
    try {
      const path = await api.webExport(game.id, by, note);
      if (path) setSaved(path);
    } catch (e) {
      setError(friendlyError(e));
    }
  }

  async function startSharing() {
    setError(null);
    try {
      setStatus(await api.githubStatus());
    } catch (e) {
      setError(friendlyError(e));
    }
  }

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
      if (err.kind !== "cancelled") setError(friendlyError(err));
    }
  }

  async function copyAndOpen() {
    if (!code) return;
    await navigator.clipboard.writeText(code.userCode).catch(() => undefined);
    await api.githubOpenDevicePage().catch((e: unknown) => setError(friendlyError(e)));
  }

  async function share() {
    setError(null);
    setSharing(true);
    try {
      setShared(await api.communityShareWeb(game.id, by, note));
    } catch (e) {
      const err = toCommandError(e);
      if (err.kind === "signedOut") setStatus((s) => (s ? { ...s, signedIn: false, login: null } : s));
      setError(friendlyError(err));
    } finally {
      setSharing(false);
    }
  }

  function close() {
    if (code) void api.githubSignInCancel().catch(() => undefined);
    onClose();
  }

  return (
    <div className="flex flex-col gap-3 rounded-2xl bg-zinc-900/90 p-5 ring-1 ring-emerald-800/70" role="dialog" aria-label={`Share the cheats for ${game.name}`}>
      <div>
        <p className="text-lg font-semibold tracking-tight">Share your cheats for {game.name}</p>
        <p className="text-xs text-zinc-400">
          Only the cheats are shared (where each number is and its value), never the game or your account. Remind friends: cheats are for
          playing alone or in private games with friends you invited.
        </p>
      </div>

      {error && (
        <p className="rounded-lg bg-red-950/40 px-3 py-2 text-sm text-red-300" role="alert">
          {error}
        </p>
      )}

      <div className="flex flex-wrap gap-3">
        <label className="flex min-w-48 flex-1 flex-col gap-1">
          Who found these cheats
          <input aria-label="Who found these cheats" className={field} maxLength={100} value={by} onChange={(e) => setBy(e.target.value)} />
        </label>
        <label className="flex min-w-48 flex-[2] flex-col gap-1">
          Notes (optional)
          <input
            aria-label="Notes"
            className={field}
            maxLength={500}
            placeholder="e.g. works from level 2, the game's version…"
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
          />
        </label>
      </div>

      <section className="flex flex-col gap-2 rounded-xl bg-zinc-950/50 p-3" aria-label="Give to friends">
        <p className="font-medium">Give them to friends</p>
        <p className="text-xs text-zinc-400">Saves a small file. Your friend opens it in OpenTrainer (Web games, then “Open a cheats file”).</p>
        <div className="flex flex-wrap items-center gap-2">
          <button className={button} disabled={!by.trim()} onClick={() => void save()}>
            Save to a file
          </button>
          {saved && (
            <>
              <span className="min-w-0 truncate text-xs text-emerald-300" role="status">
                Saved: {saved}
              </span>
              <button className={button} onClick={() => void api.packsShowFile(saved).catch(() => undefined)}>
                Show in folder
              </button>
            </>
          )}
        </div>
      </section>

      <section className="flex flex-col gap-2 rounded-xl bg-zinc-950/50 p-3" aria-label="Share with everyone">
        <p className="font-medium">Share with everyone</p>
        {local ? (
          <p className="text-xs text-zinc-400">This game runs on your own PC, so it can&apos;t go on the community list. Save it to a file instead.</p>
        ) : shared ? (
          <div className="flex flex-col gap-2 text-sm" role="status">
            <p className="text-emerald-300">
              Sent! A maintainer will check it and add it to the list, usually within a few days.
              {shared.update && " The list already had cheats for this game: this replaces them."}
            </p>
            <button className={`${button} self-start`} onClick={() => void api.githubOpenPr(shared.url).catch(() => undefined)}>
              See it on GitHub
            </button>
          </div>
        ) : !status ? (
          <>
            <p className="text-xs text-zinc-400">Puts them on OpenTrainer&apos;s community list, where other players of this game find them.</p>
            <button className={`${button} self-start`} disabled={!by.trim()} onClick={() => void startSharing()}>
              Share with everyone…
            </button>
          </>
        ) : !status.configured ? (
          <p className="text-xs text-zinc-400">Sharing from the app isn&apos;t set up in this version yet. Save to a file instead.</p>
        ) : code ? (
          <div className="flex flex-col gap-2 text-sm" role="status">
            <p>1. Your code:</p>
            <p className="select-all font-mono text-2xl font-semibold tracking-[0.2em] text-white" data-testid="device-code">
              {code.userCode}
            </p>
            <p>2. Paste it on GitHub&apos;s page, then click Authorize.</p>
            <button className={`${primary} self-start`} onClick={() => void copyAndOpen()}>
              Copy the code and open GitHub
            </button>
          </div>
        ) : !status.signedIn ? (
          <>
            <p className="text-xs text-zinc-400">You need a free GitHub account (where the list lives). You sign in once; OpenTrainer never sees your password.</p>
            <button className={`${primary} self-start`} onClick={() => void signIn()}>
              Sign in with GitHub
            </button>
          </>
        ) : (
          <>
            <label className="flex items-start gap-2 text-sm">
              <input type="checkbox" className="mt-1" checked={tested} onChange={(e) => setTested(e.target.checked)} />I tried every cheat and they
              work. I found them myself, or the person who did said they may be shared.
            </label>
            <div className="flex flex-wrap items-center gap-2">
              <button className={primary} disabled={sharing || !tested || !by.trim()} onClick={() => void share()}>
                {sharing ? "Sharing…" : "Share"}
              </button>
              <span className="text-xs text-zinc-500">Signed in to GitHub{status.login ? ` as ${status.login}` : ""}</span>
            </div>
          </>
        )}
      </section>

      <button className={`${button} self-start`} onClick={close} disabled={sharing}>
        Close
      </button>
    </div>
  );
}
