import { describeFinding } from "../lib/format";
import type { Verdict } from "../lib/ipc";

export function GuardVerdict({ verdict }: { verdict: Verdict }) {
  const notes = verdict.notes ?? [];
  if (verdict.allowed && notes.length > 0) {
    const names = [...new Set(notes.map((n) => n.product))].join(", ");
    return (
      <div
        className="rounded-lg border border-emerald-900 bg-emerald-950/40 px-3 py-2 text-sm text-emerald-300"
        role="status"
      >
        <p>No anti-cheat running. You can attach.</p>
        <p className="mt-1 text-xs text-emerald-300/75">
          {names} comes with this game but isn&apos;t running now. JuiceBox Trainer checks every few seconds
          and stops at once if it starts. Play offline.
        </p>
      </div>
    );
  }
  if (verdict.allowed) {
    return (
      <div
        className="rounded-lg border border-emerald-900 bg-emerald-950/40 px-3 py-2 text-sm text-emerald-300"
        role="status"
      >
        No anti-cheat found. You can attach.
      </div>
    );
  }
  return (
    <div
      className="rounded-lg border border-red-900 bg-red-950/40 px-3 py-2 text-sm text-red-300"
      role="alert"
    >
      <p className="font-medium">JuiceBox Trainer will not attach to this process.</p>
      <ul className="mt-1 list-disc pl-5 text-red-300/90">
        {verdict.findings.map((f, i) => (
          <li key={`${f.kind}-${f.detail}-${i}`}>{describeFinding(f)}</li>
        ))}
      </ul>
      {verdict.findings.some((f) => f.kind === "installMarker") &&
        verdict.findings.some((f) => ["process", "service", "driver", "module"].includes(f.kind)) && (
          <p className="mt-2 text-xs text-red-300/80">
            This game&apos;s anti-cheat is running. JuiceBox Trainer only works while it isn&apos;t. Some games have
            an official way to play without it (for example Halo MCC&apos;s &quot;Anti-Cheat Disabled&quot;
            choice when you press Play in Steam).
          </p>
        )}
      <p className="mt-2 text-xs text-red-300/70">
        Anti-cheat or online play was detected. This can&apos;t be turned off.
      </p>
    </div>
  );
}
