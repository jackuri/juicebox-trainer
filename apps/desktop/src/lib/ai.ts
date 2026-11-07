import type { AiEvent } from "./ipc";

/** The pending question: the last "ask" not yet answered. */
export function pendingAsk(events: AiEvent[]): Extract<AiEvent, { type: "ask" }> | null {
  const answered = new Set(events.filter((e) => e.type === "answered").map((e) => e.id));
  for (let i = events.length - 1; i >= 0; i--) {
    const e = events[i]!;
    if (e.type === "exit" || e.type === "finished") return null;
    if (e.type === "ask") return answered.has(e.id) ? null : e;
  }
  return null;
}
