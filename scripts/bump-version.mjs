#!/usr/bin/env node
// Bumps the version in every manifest at once. Usage: npm run bump -- 0.2.0
import { execSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version ?? "")) {
  console.error("usage: node scripts/bump-version.mjs <semver>");
  process.exit(1);
}

const root = fileURLToPath(new URL("..", import.meta.url));

function edit(rel, transform) {
  const path = `${root}${rel}`;
  const before = readFileSync(path, "utf8");
  const after = transform(before);
  if (after === before) throw new Error(`${rel}: version field not found or already ${version}`);
  writeFileSync(path, after);
  console.log(`updated ${rel}`);
}

// Cargo workspace version (all crates inherit it).
edit("Cargo.toml", (s) =>
  s.replace(/(\[workspace\.package\][^[]*?\nversion = ")[^"]+(")/, `$1${version}$2`),
);

// First top-level "version" key in each JSON manifest.
for (const rel of [
  "package.json",
  "apps/desktop/package.json",
  "apps/desktop/src-tauri/tauri.conf.json",
]) {
  edit(rel, (s) => s.replace(/("version":\s*")[^"]+(")/, `$1${version}$2`));
}

// Refresh lockfiles so they agree with the manifests.
execSync("npm install --package-lock-only --no-audit --no-fund", { cwd: root, stdio: "inherit" });
execSync("cargo update --workspace", { cwd: root, stdio: "inherit" });
console.log(`version is now ${version}`);
