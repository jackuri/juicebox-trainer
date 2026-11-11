import { type ReactNode, useEffect, useState } from "react";
import { AGREEMENT_TITLE, PLAY_FAIR, WAIVER } from "../lib/agreement";
import { type AgreementStatus, api } from "../lib/ipc";
import { friendlyError } from "../lib/explain";

/** The agreement's text (also shown again from About). */
export function AgreementText() {
  return (
    <div className="flex flex-col gap-3 text-sm">
      <section aria-label="Play fair">
        <h3 className="mb-1 font-semibold text-amber-200">Play fair</h3>
        <ul className="list-disc space-y-1 pl-5 text-zinc-300">
          {PLAY_FAIR.map((t) => (
            <li key={t}>{t}</li>
          ))}
        </ul>
      </section>
      <section aria-label="Your responsibility">
        <h3 className="mb-1 font-semibold text-amber-200">Your responsibility</h3>
        <ul className="list-disc space-y-1 pl-5 text-zinc-300">
          {WAIVER.map((t) => (
            <li key={t}>{t}</li>
          ))}
        </ul>
      </section>
    </div>
  );
}

/**
 * Shows the app only once the player has signed the current agreement (typing
 * their name). Until then, the agreement fills the window.
 */
export function AgreementGate({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<AgreementStatus | null>(null);
  const [name, setName] = useState("");
  const [agreed, setAgreed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.agreementStatus().then(setStatus, (e: unknown) => setError(friendlyError(e)));
  }, []);

  if (status?.accepted) return <>{children}</>;

  async function sign() {
    setBusy(true);
    setError(null);
    try {
      setStatus(await api.agreementSign(name));
    } catch (e) {
      setError(friendlyError(e));
    } finally {
      setBusy(false);
    }
  }

  const canSign = agreed && name.trim().length >= 2 && !busy;
  return (
    <main className="flex h-full items-start justify-center overflow-y-auto p-6" aria-label="Agreement">
      <div className="flex w-full max-w-2xl flex-col gap-4 rounded-2xl bg-zinc-900/90 p-6 ring-1 ring-zinc-800" role="dialog" aria-label={AGREEMENT_TITLE}>
        <div>
          <h2 className="text-2xl font-semibold tracking-tight">{AGREEMENT_TITLE}</h2>
          <p className="text-sm text-zinc-400">
            {status?.signed ? "The agreement changed since you signed it. Please read it again." : "Please read this once and sign it to start."}
          </p>
        </div>
        <AgreementText />
        {error && (
          <p className="rounded-lg bg-red-950/40 px-3 py-2 text-sm text-red-300" role="alert">
            {error}
          </p>
        )}
        {status && (
          <div className="flex flex-col gap-3 border-t border-zinc-800 pt-4 text-sm">
            <label className="flex items-start gap-2">
              <input type="checkbox" className="mt-1" checked={agreed} onChange={(e) => setAgreed(e.target.checked)} />
              I&apos;ve read this, I&apos;ll play fair, and I agree that I use OpenTrainer at my own risk.
            </label>
            <label className="flex flex-col gap-1">
              Type your name to sign
              <input
                aria-label="Your name"
                className="rounded-lg bg-zinc-950 px-3 py-1.5 outline-none ring-1 ring-zinc-800 focus:ring-emerald-600"
                maxLength={80}
                value={name}
                onChange={(e) => setName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && canSign) void sign();
                }}
              />
            </label>
            <button className="btn-accent self-start rounded-xl px-6 py-2 text-sm font-semibold disabled:cursor-not-allowed disabled:opacity-40" disabled={!canSign} onClick={() => void sign()}>
              Sign and continue
            </button>
          </div>
        )}
      </div>
    </main>
  );
}
