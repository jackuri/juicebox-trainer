import { describeFinding } from "../lib/format";
import type { Verdict } from "../lib/ipc";

export function GuardVerdict({ verdict }: { verdict: Verdict }) {
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
      <p className="font-medium">OpenTrainer will not attach to this process.</p>
      <ul className="mt-1 list-disc pl-5 text-red-300/90">
        {verdict.findings.map((f, i) => (
          <li key={`${f.kind}-${f.detail}-${i}`}>{describeFinding(f)}</li>
        ))}
      </ul>
      <p className="mt-2 text-xs text-red-300/70">
        Anti-cheat or online play was detected. This can&apos;t be turned off.
      </p>
    </div>
  );
}
