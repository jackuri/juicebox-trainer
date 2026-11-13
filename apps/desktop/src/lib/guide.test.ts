import { describe, expect, it } from "vitest";
import doc from "../../../../docs/GETTING_STARTED.md?raw";
import { findQuestions, guideMarkdown, LESSONS, parts, QUESTIONS, TOPICS } from "./guide";

describe("guide", () => {
  it("keeps docs/GETTING_STARTED.md the same as the app (run npm run guide)", () => {
    expect(doc.replace(/\r\n/g, "\n")).toBe(guideMarkdown());
  });

  it("has unique lessons and questions, each under a known topic", () => {
    expect(new Set(LESSONS.map((l) => l.id)).size).toBe(LESSONS.length);
    expect(new Set(QUESTIONS.map((x) => x.q)).size).toBe(QUESTIONS.length);
    for (const x of QUESTIONS) expect(TOPICS).toContain(x.topic);
    for (const t of TOPICS) expect(QUESTIONS.some((x) => x.topic === t)).toBe(true);
  });

  it("closes every ** and ` mark", () => {
    const texts = [...LESSONS.flatMap((l) => [l.title, l.summary, ...l.steps, l.tip ?? ""]), ...QUESTIONS.flatMap((x) => [x.q, x.a])];
    for (const t of texts) {
      const rest = parts(t)
        .filter((p) => p.kind === "plain")
        .map((p) => p.text)
        .join("");
      expect(rest, t).not.toMatch(/\*\*|`/);
    }
  });

  it("splits bold buttons and code keys", () => {
    expect(parts("Click **Play** or press `F1`.")).toEqual([
      { text: "Click ", kind: "plain" },
      { text: "Play", kind: "bold" },
      { text: " or press ", kind: "plain" },
      { text: "F1", kind: "code" },
      { text: ".", kind: "plain" },
    ]);
  });

  it("finds questions by every word and by topic", () => {
    expect(findQuestions("antivirus").map((x) => x.q)).toEqual(["My antivirus warns about JuiceBox Trainer"]);
    expect(findQuestions("FLASHLIGHT battery").some((x) => x.q.includes("doesn't show a number"))).toBe(true);
    expect(findQuestions("zzz nothing")).toEqual([]);
    expect(findQuestions("", "Saves").every((x) => x.topic === "Saves")).toBe(true);
    expect(findQuestions("").length).toBe(QUESTIONS.length);
  });
});
