// Writes dist/package.json: what the installed app downloads from npm on first
// use (exact versions of the packages this build was made and tested with).
import { readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const own = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const require = createRequire(join(root, "package.json"));

const dependencies = {};
for (const name of Object.keys(own.dependencies)) {
  // Packages with an "exports" map hide package.json; find it from the entry.
  let dir = dirname(require.resolve(name));
  for (;;) {
    try {
      const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
      if (pkg.name === name) {
        dependencies[name] = pkg.version;
        break;
      }
    } catch {
      // keep going up
    }
    const up = dirname(dir);
    if (up === dir) throw new Error(`can't find the installed version of ${name}`);
    dir = up;
  }
}

const runtime = {
  name: "juicebox-ai-helper",
  version: own.version,
  private: true,
  type: "module",
  description: "JuiceBox Trainer's AI helper (downloaded by the app on first use).",
  dependencies,
};
writeFileSync(join(root, "dist", "package.json"), JSON.stringify(runtime, null, 2) + "\n");
console.log(`dist/package.json: ${Object.entries(dependencies).map(([n, v]) => `${n}@${v}`).join(", ")}`);
