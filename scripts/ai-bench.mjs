// The AI helper benchmark (v1.21.0, PLAN.md §5.6 step 1): runs the real AI helper on JuiceBox Trainer's
// own test game (testbed_native) with a simulated player who answers from the game's real state,
// then checks the cheat pack it saved on a freshly started copy of the game (so cheats that only
// worked until a restart don't count). Prints a score table and writes target/ai-bench/<time>.json.
//
// It uses your Claude login (or ANTHROPIC_API_KEY with --api-key) and costs a little each run: the
// budget per scenario is capped (--budget, default $1.50). Everything it writes (packs, notes, save
// backups, logs) goes to a folder under target/ai-bench, never to your own JuiceBox Trainer folders.
//
//   node scripts/ai-bench.mjs                      # x64 with the best model
//   node scripts/ai-bench.mjs x64 x86 x64-cheaper  # several scenarios
//
// Scenarios: x64, x86 (32-bit build), x64-cheaper (the cheaper model). Needs `npm run build:agent`
// and the testbed builds (cargo test builds them).
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { createInterface } from "node:readline";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1")), "..");
// JB_BENCH_SIDECAR / JB_BENCH_MCP: another build to compare with (e.g. an older version).
const SIDECAR = process.env.JB_BENCH_SIDECAR ?? path.join(ROOT, "sidecars", "ai-agent", "dist", "index.js");
const MCP = process.env.JB_BENCH_MCP ?? path.join(ROOT, "target", "debug", "jb-mcp.exe");
const TESTBED = {
  x64: path.join(ROOT, "target", "testbed", "debug", "testbed_native.exe"),
  x86: path.join(ROOT, "target", "testbed", "i686-pc-windows-msvc", "debug", "testbed_native.exe"),
};
const SCENARIOS = {
  x64: { testbed: "x64" },
  x86: { testbed: "x86" },
  "x64-cheaper": { testbed: "x64", model: "claude-sonnet-5-5" },
};
const WISHES = ["Infinite health", "Lots of money / gold", "Infinite ammo"];
const STATS = ["health", "gold", "ammo"];

const args = process.argv.slice(2);
const flag = (name, def) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : def;
};
const budget = Number(flag("--budget", "1.5"));
const apiKey = args.includes("--api-key");
const picked = args.filter((a, i) => !a.startsWith("--") && args[i - 1] !== "--budget");
const scenarios = picked.length ? picked : ["x64"];
for (const s of scenarios) if (!SCENARIOS[s]) throw new Error(`unknown scenario ${s} (${Object.keys(SCENARIOS).join(", ")})`);
for (const f of [SIDECAR, ...new Set(scenarios.map((s) => TESTBED[SCENARIOS[s].testbed]))]) {
  if (!existsSync(f)) throw new Error(`missing ${f}: run npm run build:agent and cargo test first`);
}
if (!existsSync(MCP)) spawnSync("cargo", ["build", "-p", "jb-mcp"], { cwd: ROOT, stdio: "inherit" });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const stamp = new Date().toISOString().replace(/[:.]/g, "-");
const OUT = path.join(ROOT, "target", "ai-bench", stamp);
mkdirSync(OUT, { recursive: true });

/** The test game, driven through its --json command line. */
class Testbed {
  constructor(exe) {
    this.exe = exe;
    this.start();
  }
  start() {
    this.p = spawn(this.exe, ["--json"], { stdio: ["pipe", "pipe", "ignore"] });
    this.lines = [];
    this.waiting = null;
    createInterface({ input: this.p.stdout }).on("line", (l) => {
      if (this.waiting) {
        const w = this.waiting;
        this.waiting = null;
        w(l);
      } else this.lines.push(l);
    });
  }
  get pid() {
    return this.p.pid;
  }
  async send(cmd) {
    await sleep(50);
    this.lines = [];
    const reply = new Promise((r) => (this.waiting = r));
    this.p.stdin.write(cmd + "\n");
    return JSON.parse(await Promise.race([reply, sleep(5000).then(() => "{}")]));
  }
  state() {
    return this.send("state");
  }
  async restart() {
    this.p.kill();
    await sleep(500);
    this.start();
    await sleep(800);
  }
  kill() {
    this.p.kill();
  }
}

/** Which stats a question is about. */
const about = (q) =>
  STATS.filter((s) =>
    s === "health" ? /health|hp\b|life|damage|hit/i.test(q) : s === "gold" ? /gold|coin|money|cash|spend|earn|buy/i.test(q) : /ammo|bullet|shot|shoot|fire|reload/i.test(q),
  );
const act = (tb, stat, q = "") =>
  tb.send(stat === "health" ? "hit 7" : stat === "gold" ? (/earn|gain|get|pick/i.test(q) ? "earn 25" : "spend 13") : /reload/i.test(q) ? "reload" : "fire 2");

/** The stat named last in a question ("…health is right. What gold number…" is gold). */
const named = (q) => {
  let best = null;
  let at = -1;
  for (const [w, re] of [["health", /health|\bhp\b|life/gi], ["gold", /gold|coins?|money|cash/gi], ["ammo", /ammo|bullets?/gi]]) {
    for (const m of q.matchAll(re)) if (m.index > at) [best, at] = [w, m.index];
  }
  return best;
};
/** Asked to do something first ("take some damage, then type…"). */
const doFirst = (q) => /\b(take|fire|shoot|spend|earn|buy|get hit|lose)\b[^.?!]*\bthen\b/i.test(q);

/** The simulated player: answers the helper from the game's real state. */
async function answer(tb, q, kind, last) {
  const stats = about(q);
  if (kind === "text") {
    if (doFirst(q)) for (const w of stats) await act(tb, w, q);
    const s = await tb.state();
    const w = named(q) ?? stats[0] ?? last.stat;
    return w && s[w] !== undefined ? String(s[w]) : "I don't see that number.";
  }
  if (kind === "done") {
    if (/restart|close the game|reopen|start the game again|open the game again/i.test(q)) {
      await tb.restart();
      return "Done";
    }
    for (const w of stats.length ? stats : [last.stat ?? "gold"]) {
      await act(tb, w, q);
      last.stat = w;
    }
    return "Done";
  }
  // yes / no
  const before = await tb.state();
  if (/stay|still|same|didn't|did not|not go down|full|keep/i.test(q) && !/now show/i.test(q)) {
    const list = stats.length ? stats : [last.stat ?? "health"];
    for (const w of list) await act(tb, w, q);
    const after = await tb.state();
    return list.every((w) => after[w] >= before[w]) ? "Yes" : "No";
  }
  // "health 93, ammo 28 and gold 999,999": every pair must be right.
  const plain = q.replace(/(\d),(\d)/g, "$1$2");
  const pairs = [...plain.matchAll(/\b(health|gold|ammo)\D{0,12}?(-?\d+(?:\.\d+)?)/gi)];
  if (pairs.length > 1) return pairs.every((m) => Math.abs(before[m[1].toLowerCase()] - Number(m[2])) < 0.01) ? "Yes" : "No";
  const nums = [...plain.matchAll(/-?\d+(\.\d+)?/g)].map((m) => Number(m[0]));
  if (nums.length) return nums.some((n) => STATS.some((w) => Math.abs(before[w] - n) < 0.01)) ? "Yes" : "No";
  return "Yes";
}

/** Runs the helper once; returns what happened. */
async function runHelper(name, sc, sandbox) {
  const tb = new Testbed(TESTBED[sc.testbed]);
  await sleep(800);
  const env = { ...process.env, APPDATA: path.join(sandbox, "Roaming"), LOCALAPPDATA: path.join(sandbox, "Local") };
  for (const k of Object.keys(env)) if (/^(CLAUDECODE$|CLAUDE_CODE_|CLAUDE_AGENT_SDK_)/i.test(k)) delete env[k];
  mkdirSync(env.APPDATA, { recursive: true });
  mkdirSync(env.LOCALAPPDATA, { recursive: true });
  Object.assign(env, { JB_MCP_COMMAND: MCP, JB_MCP_ARGS: "[]", JB_APP_VERSION: "bench" });
  const helper = spawn("node", [SIDECAR], { env, stdio: ["pipe", "pipe", "pipe"] });
  const send = (m) => helper.stdin.write(JSON.stringify(m) + "\n");
  const log = [];
  const result = { scenario: name, questions: 0, tools: {}, cost: 0, seconds: 0, finished: null, errors: [] };
  const t0 = Date.now();
  const last = {};
  let queue = Promise.resolve();
  await new Promise((resolve) => {
    createInterface({ input: helper.stdout }).on("line", (line) => {
      let m;
      try {
        m = JSON.parse(line);
      } catch {
        return;
      }
      log.push(m);
      if (m.type === "ready") {
        send({
          type: "start",
          game: { pid: tb.pid, name: "testbed_native.exe", exe: TESTBED[sc.testbed] },
          wishes: WISHES,
          backend: apiKey ? "apiKey" : "subscription",
          model: sc.model,
          maxBudgetUsd: budget,
        });
      } else if (m.type === "ask") {
        result.questions++;
        queue = queue.then(async () => {
          const a = await answer(tb, m.text, m.kind, last);
          console.log(`  [${name}] ${m.kind}: ${m.text.slice(0, 100)} -> ${a}`);
          send({ type: "answer", id: m.id, text: a });
        });
      } else if (m.type === "activity") {
        result.tools[m.tool] = (result.tools[m.tool] ?? 0) + 1;
      } else if (m.type === "cost") {
        result.cost = m.usd;
      } else if (m.type === "error") {
        result.errors.push(m.message);
      } else if (m.type === "finished") {
        result.finished = { ok: m.ok, summary: m.summary };
      }
    });
    const timer = setTimeout(() => {
      result.errors.push("timed out after 20 minutes");
      helper.kill();
    }, 20 * 60 * 1000);
    helper.on("exit", () => {
      clearTimeout(timer);
      resolve();
    });
  });
  result.seconds = Math.round((Date.now() - t0) / 1000);
  tb.kill();
  writeFileSync(path.join(OUT, `${name}-transcript.json`), JSON.stringify(log, null, 2));
  return result;
}

/** A tiny MCP client for the guarded tool server. */
function toolServer(env) {
  const p = spawn(MCP, [], { env, stdio: ["pipe", "pipe", "ignore"] });
  const waiting = new Map();
  createInterface({ input: p.stdout }).on("line", (l) => {
    try {
      const m = JSON.parse(l);
      waiting.get(m.id)?.(m);
      waiting.delete(m.id);
    } catch {
      /* not ours */
    }
  });
  let id = 0;
  const rpc = (method, params) =>
    new Promise((r) => {
      const n = ++id;
      waiting.set(n, r);
      p.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: n, method, params }) + "\n");
    });
  const call = async (name, a = {}) => {
    const r = await rpc("tools/call", { name, arguments: a });
    const text = r.result?.content?.[0]?.text ?? "";
    if (r.result?.isError) throw new Error(text);
    return JSON.parse(text);
  };
  return { rpc, call, close: () => p.stdin.end() };
}

/** Checks the saved pack on a fresh copy of the game: which stats its cheats really control. */
async function verify(sc, sandbox) {
  const packsDir = path.join(sandbox, "Roaming", "JuiceBox Trainer", "packs");
  const packs = existsSync(packsDir)
    ? readdirSync(packsDir)
        .map((d) => path.join(packsDir, d, "pack.json"))
        .filter(existsSync)
    : [];
  const covered = new Set();
  const cheats = [];
  if (!packs.length) return { packs: 0, covered: [], cheats };
  const tb = new Testbed(TESTBED[sc.testbed]);
  await sleep(800);
  const env = { ...process.env, APPDATA: path.join(sandbox, "verify-Roaming"), LOCALAPPDATA: path.join(sandbox, "verify-Local") };
  const ts = toolServer(env);
  try {
    await ts.rpc("initialize", { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "bench", version: "1" } });
    await ts.call("attach", { pid: tb.pid });
    for (const file of packs) {
      const pack = JSON.parse(readFileSync(file, "utf8"));
      await ts.call("pack_try", { pack });
      for (const c of pack.cheats) {
        const row = { name: c.name, ui: c.ui, works: [] };
        try {
          await ts.call("revert_all");
          await ts.call("pack_try", { pack });
          const s0 = await tb.state();
          if (c.ui === "number" || c.ui === "slider") {
            await ts.call("cheat_set", { id: c.id, value: "777" });
            const s1 = await tb.state();
            row.works = STATS.filter((w) => s1[w] === 777 && s0[w] !== 777);
          } else if (c.ui === "toggle") {
            await ts.call("cheat_set", { id: c.id, on: true });
            await sleep(200);
            const s1 = await tb.state();
            for (const w of STATS) await act(tb, w);
            await sleep(200);
            const s2 = await tb.state();
            row.works = STATS.filter((w) => s2[w] >= s1[w] && (w !== "gold" || s1[w] >= s0[w]));
            // A money cheat may only act when gold changes: earning should give lots.
            if (!row.works.includes("gold") && about(c.name).includes("gold")) {
              await tb.send("earn 25");
              await sleep(200);
              if ((await tb.state()).gold >= 99999) row.works.push("gold");
            }
            // A cheat that stops everything isn't three cheats: count only what it was named for.
            if (row.works.length > 1) row.works = row.works.filter((w) => about(c.name).includes(w));
          }
        } catch (e) {
          row.error = String(e.message ?? e).slice(0, 200);
        }
        for (const w of row.works) covered.add(w);
        cheats.push(row);
      }
    }
    await ts.call("revert_all");
  } finally {
    ts.close();
    tb.kill();
  }
  return { packs: packs.length, covered: [...covered], cheats };
}

const report = [];
for (const name of scenarios) {
  const sc = SCENARIOS[name];
  const sandbox = path.join(OUT, name);
  mkdirSync(sandbox, { recursive: true });
  console.log(`== ${name}`);
  const run = await runHelper(name, sc, sandbox);
  const check = await verify(sc, sandbox);
  const score = STATS.filter((s) => check.covered.includes(s)).length;
  report.push({ ...run, ...check, score: `${score}/${STATS.length}` });
  console.log(`   score ${score}/${STATS.length}, ${run.questions} questions, $${run.cost.toFixed(2)}, ${run.seconds}s`);
}
writeFileSync(path.join(OUT, "report.json"), JSON.stringify(report, null, 2));
console.log("\nscenario       score  questions  tool calls  cost    time");
for (const r of report) {
  const calls = Object.values(r.tools).reduce((a, b) => a + b, 0);
  console.log(`${r.scenario.padEnd(14)} ${r.score.padEnd(6)} ${String(r.questions).padEnd(10)} ${String(calls).padEnd(11)} $${r.cost.toFixed(2).padEnd(6)} ${r.seconds}s`);
}
console.log(`\nDetails: ${OUT}`);
process.exit(0);
