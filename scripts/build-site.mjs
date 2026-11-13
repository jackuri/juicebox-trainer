// Builds the website (GitHub Pages) into site/dist: the pages in site/pages wrapped in the shared
// header and footer, the Getting started page made from the app's own tutorial and questions
// (apps/desktop/src/lib/guide.ts), the packer titles from lib/levels.ts, and the juice box pictures.
// Then checks that every link and picture inside the site exists.
// Usage: npm run site            (build + check)
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { LESSONS, parts, QUESTIONS, TOPICS } from "../apps/desktop/src/lib/guide.ts";
import { STREAK_DAYS, STREAK_XP, TITLES, XP_PER_DAY, XP_PER_PACK, XP_PER_RATING } from "../apps/desktop/src/lib/levels.ts";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const SITE = path.join(ROOT, "site");
const OUT = path.join(SITE, "dist");
const version = JSON.parse(readFileSync(path.join(ROOT, "package.json"), "utf8")).version;

export const REPO = "https://github.com/jackuri/juicebox-trainer";
export const PACKS_REPO = "https://github.com/jackuri/juicebox-packs";

const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
/** Guide text: **buttons** in bold, `keys` as code. */
const rich = (text) =>
  parts(text)
    .map((p) => (p.kind === "bold" ? `<b>${esc(p.text)}</b>` : p.kind === "code" ? `<code>${esc(p.text)}</code>` : esc(p.text)))
    .join("");
const slug = (s) => s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");

const NAV = [
  ["index.html", "Home"],
  ["start.html", "Getting started"],
  ["community.html", "Community"],
  ["safety.html", "Play fair & safety"],
];

function page(file, title, description, body) {
  const nav = NAV.map(
    ([href, label]) => `<a href="${href}"${href === file ? ' aria-current="page"' : ""}>${label}</a>`,
  ).join("\n        ");
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${esc(title)}</title>
  <meta name="description" content="${esc(description)}">
  <meta property="og:title" content="${esc(title)}">
  <meta property="og:description" content="${esc(description)}">
  <meta property="og:image" content="assets/juicebox/default.png">
  <meta name="theme-color" content="#0b0d12">
  <link rel="icon" href="assets/juicebox/default.png">
  <link rel="stylesheet" href="assets/style.css">
</head>
<body>
  <a class="skip" href="#main">Skip to the page</a>
  <header class="top">
    <div class="wrap top-row">
      <a class="brand" href="index.html"><img src="assets/juicebox/default.png" alt="" width="36" height="36"> JuiceBox Trainer</a>
      <nav aria-label="Pages">
        ${nav}
      </nav>
      <a class="btn btn-small" href="index.html#download">Download</a>
    </div>
  </header>
  <main id="main">
${body}
  </main>
  <footer class="foot">
    <div class="wrap foot-row">
      <div>
        <p class="brand"><img src="assets/juicebox/wink.png" alt="" width="28" height="28"> JuiceBox Trainer</p>
        <p class="muted">Free and open source (MIT). Made by players, for PC games without anti-cheat.
        Not made by or linked to any game's maker.</p>
      </div>
      <ul class="foot-links">
        <li><a href="${REPO}">Source code on GitHub</a></li>
        <li><a href="${REPO}/releases">All versions</a></li>
        <li><a href="${PACKS_REPO}">Community list</a></li>
        <li><a href="${REPO}/blob/main/docs/USER_GUIDE.md">User guide</a></li>
        <li><a href="${REPO}/blob/main/LICENSE">License</a></li>
      </ul>
    </div>
    <p class="wrap muted small">Version ${esc(version)} of this site. Nothing on this site tracks you: no cookies, no analytics.</p>
  </footer>
  <script src="assets/site.js" defer></script>
</body>
</html>
`;
}

/** Getting started: the tutorial and the questions, from the app's own text. */
function startBody() {
  const lessons = LESSONS.map(
    (l, i) => `      <article class="lesson card" id="${slug(l.title)}">
        <header><span class="num">${i + 1}</span><div><h3>${esc(l.title)}</h3><p class="muted">${esc(l.summary)} · about ${l.minutes} min</p></div></header>
        <ol>
${l.steps.map((s) => `          <li>${rich(s)}</li>`).join("\n")}
        </ol>
${l.tip ? `        <p class="tip"><b>Tip:</b> ${rich(l.tip)}</p>\n` : ""}      </article>`,
  ).join("\n");
  const toc = LESSONS.map((l, i) => `<li><a href="#${slug(l.title)}">${i + 1}. ${esc(l.title)}</a></li>`).join("");
  const faq = TOPICS.map(
    (t) => `      <section class="topic" data-topic="${esc(t)}">
        <h3>${esc(t)}</h3>
${QUESTIONS.filter((x) => x.topic === t)
  .map((x) => `        <details class="qa"><summary>${esc(x.q)}</summary><p>${rich(x.a)}</p></details>`)
  .join("\n")}
      </section>`,
  ).join("\n");
  return `    <section class="hero hero-small">
      <div class="wrap">
        <h1>Getting started</h1>
        <p class="lead">Nine short lessons, from your first cheats to making your own, by hand or with AI.
        They're in the app too: open <b>Help</b>, and each lesson's <b>Show me</b> button opens the right screen.</p>
        <ol class="toc">${toc}</ol>
      </div>
    </section>
    <section class="wrap stack" aria-label="Tutorial">
${lessons}
    </section>
    <section class="wrap stack" id="questions" aria-label="Common questions">
      <h2>Common questions</h2>
      <input type="search" id="faq-search" class="search" placeholder="Search the questions, e.g. antivirus, flashlight, controller" aria-label="Search the questions">
      <p id="faq-none" class="muted" hidden>No question matches that. Try other words, or look through the lessons above.</p>
${faq}
    </section>`;
}

function build() {
  // Empty the folder (not remove it, so a preview server running in it keeps working).
  mkdirSync(OUT, { recursive: true });
  for (const f of readdirSync(OUT)) rmSync(path.join(OUT, f), { recursive: true, force: true });
  mkdirSync(path.join(OUT, "assets"), { recursive: true });
  cpSync(path.join(SITE, "assets"), path.join(OUT, "assets"), { recursive: true });
  cpSync(path.join(ROOT, "apps/desktop/public/juicebox"), path.join(OUT, "assets/juicebox"), { recursive: true });
  // The packer titles and XP rules, the same as the app's.
  writeFileSync(
    path.join(OUT, "assets/levels.js"),
    `// Made from apps/desktop/src/lib/levels.ts by scripts/build-site.mjs.
window.JB_LEVELS = ${JSON.stringify({ titles: TITLES, pack: XP_PER_PACK, rating: XP_PER_RATING, day: XP_PER_DAY, streakDays: STREAK_DAYS, streak: STREAK_XP })};
`,
  );
  for (const file of readdirSync(path.join(SITE, "pages"))) {
    const src = readFileSync(path.join(SITE, "pages", file), "utf8");
    const m = /^<!--\s*title:\s*(.+?)\s*\|\s*description:\s*(.+?)\s*-->\r?\n/.exec(src);
    if (!m) throw new Error(`${file}: the first line must be <!-- title: … | description: … -->`);
    const body = src.slice(m[0].length).replace(/\{\{(\w+)\}\}/g, (_, k) => {
      const v = { REPO, PACKS_REPO, VERSION: version, PACK_XP: XP_PER_PACK, RATING_XP: XP_PER_RATING, DAY_XP: XP_PER_DAY, STREAK_DAYS, STREAK_XP }[k];
      if (v === undefined) throw new Error(`${file}: unknown {{${k}}}`);
      return esc(v);
    });
    writeFileSync(path.join(OUT, file), page(file, m[1], m[2], body));
  }
  writeFileSync(
    path.join(OUT, "start.html"),
    page(
      "start.html",
      "Getting started · JuiceBox Trainer",
      "A short tutorial and common questions: use cheats in PC games, and make your own by hand or with AI.",
      startBody(),
    ),
  );
  writeFileSync(path.join(OUT, ".nojekyll"), "");
}

/** Every href/src inside the site points at a file that's there (and #ids on the same page exist). */
function check() {
  const problems = [];
  const pages = readdirSync(OUT).filter((f) => f.endsWith(".html"));
  if (!pages.includes("index.html")) problems.push("no index.html");
  for (const file of pages) {
    const html = readFileSync(path.join(OUT, file), "utf8");
    const ids = new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]));
    for (const [, ref] of html.matchAll(/\s(?:href|src)="([^"]+)"/g)) {
      if (/^(https:|mailto:)/.test(ref)) continue;
      if (/^http:/.test(ref)) problems.push(`${file}: plain http link ${ref}`);
      const [target, hash] = ref.split("#");
      if (target) {
        if (!existsSync(path.join(OUT, target))) problems.push(`${file}: missing ${target}`);
        else if (hash && target.endsWith(".html")) {
          const other = readFileSync(path.join(OUT, target), "utf8");
          if (!other.includes(`id="${hash}"`)) problems.push(`${file}: ${target} has no #${hash}`);
        }
      } else if (hash && !ids.has(hash)) problems.push(`${file}: no #${hash} on the page`);
    }
    // Only our own scripts and stylesheet: nothing loaded from other sites.
    for (const [, src] of html.matchAll(/<script[^>]*\ssrc="([^"]+)"/g)) if (/^https?:/.test(src)) problems.push(`${file}: outside script ${src}`);
    for (const [, href] of html.matchAll(/<link[^>]*rel="stylesheet"[^>]*href="([^"]+)"/g)) if (/^https?:/.test(href)) problems.push(`${file}: outside stylesheet ${href}`);
  }
  for (const f of readdirSync(path.join(OUT, "assets")).filter((f) => f.endsWith(".js"))) {
    const js = readFileSync(path.join(OUT, "assets", f), "utf8");
    for (const [, host] of js.matchAll(/https:\/\/([a-z0-9.-]+)/g))
      if (!["api.github.com", "raw.githubusercontent.com", "github.com"].includes(host)) problems.push(`assets/${f}: talks to ${host}`);
  }
  return problems;
}

build();
const problems = check();
if (problems.length) {
  console.error(`The website has ${problems.length} problem(s):\n- ${problems.join("\n- ")}`);
  process.exit(1);
}
console.log(`Built the website in ${path.relative(ROOT, OUT)} (${readdirSync(OUT).filter((f) => f.endsWith(".html")).length} pages); every link is there.`);
