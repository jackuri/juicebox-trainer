import { useCallback, useEffect, useState } from "react";
import { api, type RemoteStatus } from "../lib/ipc";
import { friendlyError } from "../lib/explain";

const POLL_MS = 2000;

/** "today", "yesterday" or the date. */
function when(seconds: number): string {
  const d = new Date(seconds * 1000);
  const today = new Date();
  const days = Math.round(
    (new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime() -
      new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()) /
      86_400_000,
  );
  if (days <= 0) return "today";
  if (days === 1) return "yesterday";
  return d.toLocaleDateString();
}

/** Phone remote: off until turned on; a phone pairs by scanning the QR code. */
export function RemotePanel() {
  const [status, setStatus] = useState<RemoteStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);

  const poll = useCallback(() => {
    api.remoteStatus().then(setStatus, () => undefined);
  }, []);

  useEffect(() => {
    poll();
    const t = setInterval(poll, POLL_MS);
    return () => clearInterval(t);
  }, [poll]);

  async function run(fn: () => Promise<RemoteStatus | null | void>) {
    setError(null);
    setBusy(true);
    try {
      const s = await fn();
      setStatus(s ?? null);
    } catch (e) {
      setError(friendlyError(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="flex flex-col gap-2 rounded-xl bg-zinc-900/70 px-3 py-2 ring-1 ring-zinc-800" aria-label="Phone remote">
      <div className="flex items-center gap-2">
        <h3 className="text-sm font-medium">Phone remote</h3>
        {status ? (
          <span className="rounded bg-emerald-950 px-1.5 py-0.5 text-[11px] text-emerald-300">On</span>
        ) : (
          <span className="rounded bg-zinc-800 px-1.5 py-0.5 text-[11px] text-zinc-400">Off</span>
        )}
        <div className="ml-auto flex gap-2">
          {status ? (
            <>
              <button
                className="rounded bg-zinc-800 px-2 py-1 text-xs hover:bg-zinc-700"
                disabled={busy}
                onClick={() => void run(() => api.remoteForget())}
              >
                Unpair phones
              </button>
              <button
                className="rounded bg-zinc-800 px-2 py-1 text-xs hover:bg-zinc-700"
                disabled={busy}
                onClick={() => void run(() => api.remoteStop())}
              >
                Turn off
              </button>
            </>
          ) : (
            <button
              className="rounded btn-accent px-2 py-1 text-xs disabled:opacity-40"
              disabled={busy}
              onClick={() => void run(() => api.remoteStart())}
            >
              Turn on
            </button>
          )}
        </div>
      </div>
      {!status && (
        <p className="text-xs text-zinc-500">
          Use your phone as a remote for these cheats, with the free JuiceBox Remote app (Android) or your phone&apos;s
          browser. Your phone must be on the same Wi-Fi as this PC. Windows may ask to let JuiceBox Trainer through the
          firewall: allow it on private networks.
        </p>
      )}
      {status && (
        <div className="flex flex-wrap items-start gap-4">
          <img
            alt="QR code to pair your phone"
            className="h-44 w-44 rounded bg-white p-1"
            src={`data:image/svg+xml;utf8,${encodeURIComponent(status.qrSvg)}`}
          />
          <div className="flex max-w-sm flex-col gap-1.5 text-xs text-zinc-400">
            <p>
              Scan this with the <span className="text-zinc-200">JuiceBox Remote</span> app on your phone: it stays
              paired, even after a restart.{" "}
              <button className="text-emerald-300 underline" onClick={() => void api.openLink("latest").catch(() => undefined)}>
                Get the app
              </button>
            </p>
            <p>
              Or scan it with your phone&apos;s camera to open the remote in its browser for a day. Each code works once;
              a new one shows after.
            </p>
            <p>
              No camera?{" "}
              <button
                className="text-emerald-300 underline"
                onClick={() =>
                  void navigator.clipboard?.writeText(status.pairUrl).then(
                    () => setCopied(true),
                    () => undefined,
                  )
                }
              >
                {copied ? "Link copied" : "Copy the link"}
              </button>{" "}
              and paste it in the app.
            </p>
            {status.phones.length > 0 && (
              <ul className="flex flex-col gap-1" aria-label="Paired phones">
                {status.phones.map((p) => (
                  <li key={p.id} className="flex items-center gap-2 rounded bg-zinc-950/60 px-2 py-1">
                    <span className="text-zinc-200">{p.name}</span>
                    <span className="text-zinc-500">used {when(p.lastSeen)}</span>
                    <button
                      className="ml-auto rounded bg-zinc-800 px-1.5 py-0.5 hover:bg-zinc-700"
                      disabled={busy}
                      aria-label={`Unpair ${p.name}`}
                      onClick={() => void run(() => api.remoteUnpair(p.id))}
                    >
                      Unpair
                    </button>
                  </li>
                ))}
              </ul>
            )}
            {status.devices - status.phones.length > 0 && (
              <p>
                Phone browsers paired today: <span className="text-zinc-200">{status.devices - status.phones.length}</span>.
                They can open <code className="text-zinc-300">{status.address}</code> again.
              </p>
            )}
            <p>
              Phones can only do what the buttons here do. The remote stays on when JuiceBox Trainer starts again, until
              you turn it off.
            </p>
          </div>
        </div>
      )}
      {error && (
        <p className="text-xs text-red-400" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
