import type { AskKind, Outbound } from "./protocol.js";

/** Matches the player's answers to the questions the helper asked. */
export class AskBridge {
  private next = 1;
  private pending = new Map<string, (answer: string) => void>();

  constructor(private send: (m: Outbound) => void) {}

  ask(text: string, kind: AskKind): Promise<string> {
    const id = `q${this.next++}`;
    return new Promise((resolve) => {
      this.pending.set(id, resolve);
      this.send({ type: "ask", id, text, kind });
    });
  }

  /** Returns false for an unknown or already answered question. */
  answer(id: string, text: string): boolean {
    const resolve = this.pending.get(id);
    if (!resolve) return false;
    this.pending.delete(id);
    resolve(text);
    return true;
  }

  /** Answers everything still waiting (on cancel). */
  cancelAll(text = "The player cancelled."): void {
    for (const [id, resolve] of this.pending) {
      this.pending.delete(id);
      resolve(text);
    }
  }

  get waiting(): number {
    return this.pending.size;
  }
}
