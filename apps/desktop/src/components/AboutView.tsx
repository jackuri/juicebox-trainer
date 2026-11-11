import { useEffect, useState } from "react";
import { AgreementText } from "./AgreementGate";
import { type AgreementStatus, type AiStatus, api, onUpdateProgress, type UpdateInfo, type UpdateStatus } from "../lib/ipc";
import { friendlyError } from "../lib/explain";

const button = "rounded-lg bg-zinc-800 px-3 py-1.5 text-xs hover:bg-zinc-700 disabled:cursor-not-allowed disabled:opacity-40";

/** Version, updates, the user guide and what JuiceBox Trainer never does. */
export function AboutView() {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [ai, setAi] = useState<AiStatus | null>(null);
  const [agreement, setAgreement] = useState<AgreementStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [result, setResult] = useState<UpdateInfo | null | undefined>(undefined);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<string | null>(null);
  const [confirmAi, setConfirmAi] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.updateStatus().then(setStatus, (e: unknown) => setError(friendlyError(e)));
    api.aiStatus().then(setAi, () => undefined);
    api.agreementStatus().then(setAgreement, () => undefined);
  }, []);
  useEffect(
    () =>
      onUpdateProgress((got, total) =>
        setProgress(total ? `Downloading… ${Math.round((got / total) * 100)}%` : `Downloading… ${Math.round(got / 1e6)} MB`),
      ),
    [],
  );

  /** One click: look for a newer version and, if there is one, install it and restart. */
  async function update() {
    if (!status) return;
    setError(null);
    setResult(undefined);
    if (!status.enabled) {
      // This copy can't update itself: open the newest release to download.
      await api.openLink("latest").catch((e: unknown) => setError(friendlyError(e)));
      return;
    }
    setChecking(true);
    let found: UpdateInfo | null;
    try {
      found = await api.updateCheck();
    } catch (e) {
      setError(friendlyError(e));
      return;
    } finally {
      setChecking(false);
    }
    setResult(found);
    if (!found) return;
    setInstalling(true);
    try {
      await api.updateInstall();
    } catch (e) {
      setError(friendlyError(e));
      setInstalling(false);
      setProgress(null);
    }
  }
  async function removeAi() {
    setConfirmAi(false);
    setError(null);
    try {
      await api.aiUninstall();
      setAi(await api.aiStatus());
    } catch (e) {
      setError(friendlyError(e));
    }
  }

  return (
    <main className="flex min-h-0 max-w-3xl flex-col gap-5 overflow-y-auto p-6 text-sm" aria-label="About">
      <section>
        <h2 className="text-2xl font-semibold tracking-tight">JuiceBox Trainer {status && <span className="font-mono text-sm text-zinc-400">v{status.version}</span>}</h2>
        <p className="text-zinc-400">
          Free and open-source (MIT licence) trainer, save manager and AI cheat-pack maker for single-player PC games.
        </p>
        <div className="mt-2 flex gap-2">
          <button className={button} onClick={() => void api.openLink("guide").catch(() => undefined)}>
            User guide
          </button>
          <button className={button} onClick={() => void api.openLink("releases").catch(() => undefined)}>
            Releases page
          </button>
        </div>
      </section>

      {error && (
        <p className="rounded-lg bg-red-950/40 px-3 py-2 text-red-300" role="alert">
          {error}
        </p>
      )}

      <section className="flex flex-col gap-2" aria-label="Updates">
        <h3 className="font-medium">Updates</h3>
        <div className="flex flex-wrap items-center gap-3">
          <button
            className="btn-accent rounded-xl px-6 py-2 text-sm font-semibold disabled:cursor-not-allowed disabled:opacity-40"
            disabled={!status || checking || installing}
            onClick={() => void update()}
          >
            {installing ? "Updating…" : checking ? "Checking…" : "Update"}
          </button>
          {result === null && !checking && (
            <span className="text-emerald-300" role="status">
              You have the newest version.
            </span>
          )}
          {progress && <span className="text-xs text-zinc-300">{progress}</span>}
        </div>
        {status?.enabled ? (
          <p className="text-xs text-zinc-500">
            Gets the newest JuiceBox Trainer and installs it. JuiceBox Trainer closes for a moment (games get their original code back
            first) and opens again in the new version.
          </p>
        ) : (
          status && <p className="text-xs text-zinc-500">{status.offReason}</p>
        )}
        {result && (
          <div className="flex flex-col gap-1 rounded-lg bg-sky-950/40 px-3 py-2 text-sky-100" role="status">
            <p>
              Updating to JuiceBox Trainer {result.version}
              {result.date ? ` (${result.date})` : ""}…
            </p>
            {result.notes && <p className="max-h-40 overflow-y-auto whitespace-pre-wrap text-xs text-sky-200/80">{result.notes}</p>}
          </div>
        )}
      </section>
      {ai && (ai.download === "ready" || ai.download === "outdated") && (
        <section className="flex flex-col gap-2" aria-label="AI engine">
          <h3 className="font-medium">AI engine</h3>
          <p className="text-zinc-400">The AI helper's engine is downloaded on this PC (about 330 MB).</p>
          {confirmAi ? (
            <div className="flex items-center gap-2 text-xs">
              Delete it? You can download it again later.
              <button className="rounded-lg bg-red-800 px-2 py-1 hover:bg-red-700" onClick={() => void removeAi()}>
                Delete
              </button>
              <button className={button} onClick={() => setConfirmAi(false)}>
                Keep
              </button>
            </div>
          ) : (
            <div>
              <button className={button} onClick={() => setConfirmAi(true)}>
                Delete the AI engine
              </button>
            </div>
          )}
        </section>
      )}

      {agreement?.signed && (
        <section className="flex flex-col gap-1" aria-label="Your agreement">
          <h3 className="font-medium">Your agreement</h3>
          <p className="text-zinc-400">
            Signed by {agreement.signed.name} on {agreement.signed.date}.
          </p>
          <details className="text-zinc-400">
            <summary className="cursor-pointer text-emerald-300 hover:text-emerald-200">Read it again</summary>
            <div className="mt-2">
              <AgreementText />
            </div>
          </details>
        </section>
      )}

      <section className="flex flex-col gap-1" aria-label="What JuiceBox Trainer never does">
        <h3 className="font-medium">What JuiceBox Trainer never does</h3>
        <ul className="list-disc pl-5 text-zinc-400">
          <li>Work in online or multiplayer games, or in any game with anti-cheat. It checks every game and refuses those.</li>
          <li>Hide from, switch off or trick anti-cheat, DRM or ownership checks.</li>
          <li>Run code written by the AI. AI cheats are checked data, and every change can be undone.</li>
          <li>Change a save without backing it up first.</li>
        </ul>
      </section>
    </main>
  );
}
