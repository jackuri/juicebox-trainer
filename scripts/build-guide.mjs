// Writes docs/GETTING_STARTED.md from the app's tutorial and questions
// (apps/desktop/src/lib/guide.ts), so GitHub readers see the same text as Help.
// Usage: npm run guide
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { guideMarkdown } from "../apps/desktop/src/lib/guide.ts";

const out = fileURLToPath(new URL("../docs/GETTING_STARTED.md", import.meta.url));
writeFileSync(out, guideMarkdown());
console.log(`Wrote ${out}`);
