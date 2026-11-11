// After `tauri build`: collects what goes on the GitHub release into
// release-out/ — the installer, SHA256SUMS.txt, notes.md (this version's
// CHANGELOG section) and, when the build made an update signature,
// latest.json for the in-app updater.
//
// Usage: node scripts/release-files.mjs [repo]   (repo defaults to jackuri/juicebox-trainer)
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const repo = process.argv[2] ?? "jackuri/juicebox-trainer";
const root = new URL("..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const version = JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version;
const target = process.env.CARGO_TARGET_DIR ?? join(root, "target");
const nsis = join(target, "release", "bundle", "nsis");

/** This version's section of CHANGELOG.md, without its heading. */
export function notesFor(changelog, version) {
  const lines = changelog.split(/\r?\n/);
  const start = lines.findIndex((l) => l.startsWith(`## [${version}]`));
  if (start < 0) throw new Error(`CHANGELOG.md has no entry for ${version}`);
  let end = lines.findIndex((l, i) => i > start && l.startsWith("## ["));
  if (end < 0) end = lines.length;
  return lines.slice(start + 1, end).join("\n").trim() + "\n";
}

// Tauri names the installer after the product ("JuiceBox Trainer"); the
// release uses a name without spaces (GitHub turns spaces into dots).
const built = `JuiceBox Trainer_${version}_x64-setup.exe`;
const installer = `JuiceBoxTrainer_${version}_x64-setup.exe`;
if (!existsSync(join(nsis, built))) {
  const found = existsSync(nsis) ? readdirSync(nsis).join(", ") : "nothing";
  throw new Error(`no ${built} in ${nsis} (found: ${found}); run tauri build first`);
}

const out = join(root, "release-out");
rmSync(out, { recursive: true, force: true });
mkdirSync(out);
copyFileSync(join(nsis, built), join(out, installer));

const sha = createHash("sha256").update(readFileSync(join(out, installer))).digest("hex");
writeFileSync(join(out, "SHA256SUMS.txt"), `${sha}  ${installer}\n`);

const notes = notesFor(readFileSync(join(root, "CHANGELOG.md"), "utf8"), version);
writeFileSync(join(out, "notes.md"), notes);

const sig = join(nsis, `${built}.sig`);
if (existsSync(sig)) {
  const latest = {
    version,
    notes,
    pub_date: new Date().toISOString(),
    platforms: {
      "windows-x86_64": {
        signature: readFileSync(sig, "utf8").trim(),
        url: `https://github.com/${repo}/releases/download/v${version}/${installer}`,
      },
    },
  };
  writeFileSync(join(out, "latest.json"), JSON.stringify(latest, null, 2) + "\n");
  copyFileSync(sig, join(out, `${installer}.sig`));
}

console.log(`release-out/: ${readdirSync(out).join(", ")}`);
console.log(`SHA-256 ${sha}`);
