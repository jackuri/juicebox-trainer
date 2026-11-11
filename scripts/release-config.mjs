// Decides how the release build is made and writes release.conf.json (merged
// into tauri.conf.json by `tauri build --config`). Used by the release
// workflow; run it locally to see what a release would do.
//
// Inputs (environment):
//   JB_PUBLIC=true            the repository is public (updates can be served)
//   JB_HAS_UPDATER_KEY=true   TAURI_SIGNING_PRIVATE_KEY is set
//   JB_CERT_THUMBPRINT=...    a code-signing certificate is installed
//
// Output: release.conf.json, and JB_UPDATES=1|0 in $GITHUB_ENV (when set).
import { appendFileSync, writeFileSync } from "node:fs";

const yes = (v) => String(v ?? "").toLowerCase() === "true";
const isPublic = yes(process.env.JB_PUBLIC);
const updaterKey = yes(process.env.JB_HAS_UPDATER_KEY);
const thumbprint = (process.env.JB_CERT_THUMBPRINT ?? "").trim();

export function releaseConfig({ isPublic, updaterKey, thumbprint }) {
  const conf = { bundle: { createUpdaterArtifacts: updaterKey } };
  if (thumbprint) {
    conf.bundle.windows = {
      certificateThumbprint: thumbprint,
      digestAlgorithm: "sha256",
      timestampUrl: "http://timestamp.digicert.com",
    };
  }
  return { conf, updates: isPublic && updaterKey };
}

const { conf, updates } = releaseConfig({ isPublic, updaterKey, thumbprint });
writeFileSync("release.conf.json", JSON.stringify(conf, null, 2) + "\n");
if (process.env.GITHUB_ENV) appendFileSync(process.env.GITHUB_ENV, `JB_UPDATES=${updates ? 1 : 0}\n`);

console.log(`Code signing:      ${thumbprint ? "on" : "off (no certificate yet; Windows will warn when installing)"}`);
console.log(`Update files:      ${updaterKey ? "made and signed" : "off (no updater key)"}`);
console.log(`In-app updates:    ${updates ? "on" : "off (the repository is private or has no updater key)"}`);
