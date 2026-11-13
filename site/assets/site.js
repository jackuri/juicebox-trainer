// JuiceBox Trainer website: the download button, the community pages and the question search.
// Everything is read in the visitor's browser from GitHub; nothing is sent anywhere else.
(() => {
  const LIST = "https://raw.githubusercontent.com/jackuri/juicebox-packs/main/";
  const API = "https://api.github.com/repos/jackuri/juicebox-trainer/releases/latest";
  const ISSUES = "https://github.com/jackuri/juicebox-packs/issues/";

  const $ = (sel) => document.querySelector(sel);
  const el = (tag, attrs = {}, ...kids) => {
    const n = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) {
      if (k === "class") n.className = v;
      else n.setAttribute(k, v);
    }
    for (const k of kids) if (k !== null && k !== undefined) n.append(k);
    return n;
  };
  const json = (name) =>
    fetch(LIST + name, { cache: "no-cache" }).then((r) => {
      if (!r.ok) throw new Error(`${name}: ${r.status}`);
      return r.json();
    });
  const fail = (box, what) => {
    if (!box) return;
    box.replaceChildren(el("p", { class: "muted" }, `Couldn't load ${what} right now. `, el("a", { href: "https://github.com/jackuri/juicebox-packs" }, "See the community list on GitHub"), "."));
  };

  // Levels: the same rules as the app (assets/levels.js, made from the app's code).
  const xpFor = (level) => 5 * level * (level - 1);
  const levelFor = (xp) => {
    let l = 1;
    while (l < 99 && xp >= xpFor(l + 1)) l++;
    return l;
  };
  const titleFor = (level) => {
    const titles = (window.JB_LEVELS && window.JB_LEVELS.titles) || [[1, "Recruit Packer"]];
    let t = titles[0][1];
    for (const [at, name] of titles) if (level >= at) t = name;
    return t;
  };
  const stars = (r) => {
    if (!r || !r.count) return el("span", { class: "stars muted" }, "Not rated yet");
    const full = Math.round(r.stars);
    return el("span", { class: "stars", title: `${r.stars.toFixed(1)} of 3 stars from ${r.count} player${r.count === 1 ? "" : "s"}` }, "★".repeat(full) + "☆".repeat(3 - full), el("span", { class: "muted" }, ` ${r.stars.toFixed(1)} (${r.count})`));
  };

  // The download button: straight to the newest installer when GitHub says which one it is.
  const btn = $("#download-btn");
  if (btn) {
    fetch(API)
      .then((r) => (r.ok ? r.json() : null))
      .then((rel) => {
        const exe = rel && (rel.assets || []).find((a) => /_x64-setup\.exe$/.test(a.name));
        if (!exe) return;
        btn.href = exe.browser_download_url;
        btn.textContent = `Download ${rel.tag_name} for Windows`;
        const note = $("#download-note");
        if (note) {
          note.replaceChildren(
            `${(exe.size / 1048576).toFixed(0)} MB installer, released ${new Date(rel.published_at).toLocaleDateString()}. Free, no account needed. `,
            el("a", { href: rel.html_url }, "What's new"),
          );
        }
      })
      .catch(() => undefined);
  }

  // Home: the numbers and the top packers.
  if ($("#stats") || $("#top-board")) {
    Promise.all([json("index.json"), json("xp.json")])
      .then(([index, xp]) => {
        const set = (k, v) => {
          const b = document.querySelector(`[data-stat="${k}"]`);
          if (b) b.textContent = String(v);
        };
        set("packs", (index.packs || []).filter((p) => p.id !== "testbed-demo").length + (index.tables || []).length);
        set("web", (index.web || []).length);
        set("players", (xp.players || []).length);
        const board = $("#top-board");
        if (board) {
          const top = (xp.players || []).slice(0, 5);
          board.replaceChildren(
            ...(top.length
              ? top.map((p) => {
                  const lv = levelFor(p.xp);
                  return el("li", {}, el("span", { class: "lvl" }, `LVL ${lv}`), el("span", { class: "who" }, el("b", {}, p.login), el("small", { class: "muted" }, titleFor(lv))), el("span", { class: "xp" }, `${p.xp} XP`));
                })
              : [el("li", { class: "muted" }, "No packers yet: be the first!")]),
          );
        }
      })
      .catch(() => fail($("#top-board"), "the leaderboard"));
  }

  // Community page.
  if ($("#pack-list")) {
    Promise.all([json("index.json"), json("ratings.json").catch(() => ({ ratings: {} })), json("wanted.json").catch(() => ({ wanted: [] })), json("xp.json").catch(() => ({ players: [] }))])
      .then(([index, ratings, wanted, xp]) => {
        const packs = (index.packs || []).filter((p) => p.id !== "testbed-demo");
        const tables = index.tables || [];
        const r = ratings.ratings || {};
        $("#pack-list").replaceChildren(
          ...packs.map((p) =>
            el(
              "article",
              { class: "card item" },
              el("div", { class: "item-head" }, el("h3", {}, p.name), stars(r[p.id])),
              el("p", { class: "meta" }, `${p.cheats} cheat${p.cheats === 1 ? "" : "s"} · by ${p.author === "ai" ? "the AI helper" : p.author}${p.engine ? ` · ${p.engine}` : ""} · ${p.exe}`),
              p.notes ? el("p", {}, p.notes) : null,
            ),
          ),
          ...tables.map((t) =>
            el(
              "article",
              { class: "card item" },
              el("div", { class: "item-head" }, el("h3", {}, t.game), el("span", { class: "badge" }, "Cheat table")),
              el("p", { class: "meta" }, `${t.title}${t.cheats ? ` · ${t.cheats} cheats` : ""} · made by ${t.by}`),
              /^https:\/\//.test(t.url) ? el("p", {}, el("a", { href: t.url, rel: "nofollow noopener" }, "The table's page, by its maker")) : null,
            ),
          ),
          ...(packs.length + tables.length === 0 ? [el("p", { class: "muted" }, "No packs yet. Be the first to share one!")] : []),
        );
        const web = index.web || [];
        $("#web-list").replaceChildren(
          ...(web.length
            ? web.map((w) =>
                el(
                  "article",
                  { class: "card item" },
                  el("div", { class: "item-head" }, el("h3", {}, w.game), el("span", { class: "badge" }, `${(w.cheats || []).length} cheats`)),
                  el("p", { class: "meta" }, `by ${w.by || "a player"}${w.added ? ` · added ${w.added}` : ""}`),
                  el("ul", { class: "cheat-names" }, ...(w.cheats || []).slice(0, 8).map((c) => el("li", {}, c.name))),
                  w.notes ? el("p", { class: "muted" }, w.notes) : null,
                ),
              )
            : [el("p", { class: "muted" }, "No web game cheats yet.")]),
        );
        const want = (wanted.wanted || []).slice().sort((a, b) => b.votes - a.votes);
        $("#wanted-list").replaceChildren(
          ...(want.length
            ? want.slice(0, 30).map((w) =>
                el("article", { class: "card item row" }, el("b", { class: "votes" }, `👍 ${w.votes}`), el("h3", {}, w.game), el("a", { href: ISSUES + Number(w.number) }, "Add your 👍")),
              )
            : [el("p", { class: "muted" }, "Nobody has asked for a game yet. Ask for yours in the app.")]),
        );
        const players = xp.players || [];
        $("#board-body").replaceChildren(
          ...(players.length
            ? players.slice(0, 100).map((p, i) => {
                const lv = levelFor(p.xp);
                return el("tr", {}, el("td", {}, String(i + 1)), el("td", {}, el("b", {}, p.login)), el("td", {}, String(lv)), el("td", {}, titleFor(lv)), el("td", {}, String(p.xp)), el("td", {}, String(p.packs || 0)), el("td", {}, String(p.ratings || 0)), el("td", {}, String(p.days || 0)));
              })
            : [el("tr", {}, el("td", { colspan: "8", class: "muted" }, "No packers yet: be the first!"))]),
        );
      })
      .catch(() => {
        fail($("#pack-list"), "the packs");
        fail($("#web-list"), "the web cheats");
        fail($("#wanted-list"), "the wanted games");
      });
    const titles = $("#titles");
    const fill = () => {
      if (!window.JB_LEVELS) return;
      titles.replaceChildren(...window.JB_LEVELS.titles.map(([at, name]) => el("li", {}, el("span", { class: "lvl" }, `LVL ${at}`), name)));
    };
    if (window.JB_LEVELS) fill();
    else window.addEventListener("load", fill);
  }

  // Getting started: search the questions.
  const search = $("#faq-search");
  if (search) {
    search.addEventListener("input", () => {
      const words = search.value.toLowerCase().split(/\s+/).filter(Boolean);
      let shown = 0;
      for (const topic of document.querySelectorAll(".topic")) {
        let any = false;
        for (const qa of topic.querySelectorAll(".qa")) {
          const hit = words.every((w) => qa.textContent.toLowerCase().includes(w));
          qa.hidden = !hit;
          qa.open = hit && words.length > 0;
          if (hit) any = true;
        }
        topic.hidden = !any;
        if (any) shown++;
      }
      $("#faq-none").hidden = shown > 0;
    });
  }
})();
