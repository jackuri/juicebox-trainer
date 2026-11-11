import { type ReactNode, useState } from "react";
import { problemText } from "../lib/explain";
import type { Problem } from "../lib/ipc";

/**
 * One problem in plain words (v1.22.0): what happened, why, what to try, and
 * the technical details folded away.
 */
export function ProblemCard({
  problem,
  actions,
  onDismiss,
  showTime = false,
}: {
  problem: Problem;
  /** Buttons that fix it (e.g. "Reload game"). */
  actions?: ReactNode;
  onDismiss?: () => void;
  showTime?: boolean;
}) {
  const [copied, setCopied] = useState(false);
  const serious = problem.crashed || problem.kind === "appCrash";
  async function copy() {
    try {
      await navigator.clipboard.writeText(problemText(problem));
      setCopied(true);
    } catch {
      setCopied(false);
    }
  }
  return (
    <div
      className={`flex flex-col gap-2 rounded-xl px-4 py-3 text-sm ring-1 ${
        serious ? "bg-red-950/40 text-red-100 ring-red-900/60" : "bg-amber-950/40 text-amber-100 ring-amber-900/60"
      }`}
      role="alert"
      aria-label={problem.title}
    >
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <p className="font-semibold">{problem.title}</p>
          {showTime && <p className="text-xs opacity-70">{new Date(problem.id).toLocaleString()}</p>}
        </div>
        {onDismiss && (
          <button className="text-xs opacity-70 hover:opacity-100" aria-label="Hide" onClick={onDismiss}>
            ✕
          </button>
        )}
      </div>
      <p>{problem.why}</p>
      {problem.advice.length > 0 && (
        <div>
          <p className="font-medium">What to try</p>
          <ul className="mt-0.5 list-disc pl-5">
            {problem.advice.map((a) => (
              <li key={a}>{a}</li>
            ))}
          </ul>
        </div>
      )}
      <div className="flex flex-wrap items-center gap-2">
        {actions}
        {problem.details.length > 0 && (
          <details className="text-xs opacity-80">
            <summary className="cursor-pointer select-none">Details</summary>
            <ul className="mt-1 list-disc pl-5">
              {problem.details.map((d) => (
                <li key={d} className="break-words">
                  {d}
                </li>
              ))}
            </ul>
          </details>
        )}
        <button className="ml-auto rounded bg-black/20 px-2 py-0.5 text-xs hover:bg-black/40" onClick={() => void copy()}>
          {copied ? "Copied" : "Copy details"}
        </button>
      </div>
    </div>
  );
}
