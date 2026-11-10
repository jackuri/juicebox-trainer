// OpenTrainer's helper for web games (v1.18.0). It's added to the game's page before the page's own
// scripts run, and only answers calls from OpenTrainer: find a number in the game's memory
// (WebAssembly memories, also emulators) or in its JavaScript values, read it, change it, lock it.
// It's fixed code that ships with OpenTrainer; packs and the AI never add code here.
(() => {
  "use strict";
  if (window.__otAgent) return;

  const stringify = JSON.stringify;
  const W = window.WebAssembly;
  const OrigMemory = W && W.Memory;

  // ----- memories ---------------------------------------------------------------------------
  /** { m: object with .buffer, label } */
  const memories = [];
  const seen = new WeakSet();
  function addMemory(m, label) {
    if (!m || seen.has(m)) return;
    let buf;
    try {
      buf = m.buffer;
    } catch {
      return;
    }
    // The same bytes reached another way (a Memory and its HEAPU8 view) count once.
    if (seen.has(buf)) return;
    seen.add(m);
    if (buf) seen.add(buf);
    memories.push({ m, label });
  }

  if (W && OrigMemory) {
    const Memory = function Memory(desc) {
      const m = new OrigMemory(desc);
      addMemory(m, "memory");
      return m;
    };
    Memory.prototype = OrigMemory.prototype;
    W.Memory = Memory;

    const fromExports = (instance) => {
      try {
        for (const k of Object.keys(instance.exports || {})) {
          const v = instance.exports[k];
          if (v instanceof OrigMemory) addMemory(v, "module memory");
        }
      } catch {
        // Not an instance.
      }
    };
    const OrigInstance = W.Instance;
    const Instance = function Instance(mod, imports) {
      const i = new OrigInstance(mod, imports);
      fromExports(i);
      return i;
    };
    Instance.prototype = OrigInstance.prototype;
    W.Instance = Instance;
    for (const name of ["instantiate", "instantiateStreaming"]) {
      const orig = W[name];
      if (typeof orig !== "function") continue;
      W[name] = function (...args) {
        return orig.apply(this, args).then((r) => {
          fromExports(r && r.instance ? r.instance : r);
          return r;
        });
      };
    }
  }

  const bufferOf = (i) => {
    const mem = memories[i];
    if (!mem) throw new Error("that memory is gone; look again");
    return mem.m.buffer;
  };

  // ----- JavaScript values ---------------------------------------------------------------------
  const MAX_VISITS = 300000;
  const MAX_DEPTH = 8;
  const isNode = (v) => typeof Node !== "undefined" && v instanceof Node;

  /** Every own number reachable from window: [{ obj, key, path }]. Big byte arrays become memories.
   * `match(value, key, parentPath)` picks the numbers to keep. */
  function walk(match) {
    const out = [];
    const visited = new WeakSet([window.__otAgent || {}]);
    const queue = [{ obj: window, path: [] }];
    let head = 0;
    while (head < queue.length && head < MAX_VISITS) {
      const { obj, path } = queue[head++];
      let keys;
      try {
        keys = Object.keys(obj);
      } catch {
        continue;
      }
      for (const key of keys) {
        if (obj === window && key === "__otAgent") continue;
        let d;
        try {
          d = Object.getOwnPropertyDescriptor(obj, key);
        } catch {
          continue;
        }
        if (!d || !("value" in d)) continue; // getters may do things: never call them
        const v = d.value;
        if (typeof v === "number") {
          if (match(v, key, path)) out.push({ obj, key, path: path.concat(key) });
        } else if (v && typeof v === "object" && path.length < MAX_DEPTH) {
          if (visited.has(v) || isNode(v) || v === window) continue;
          visited.add(v);
          if (ArrayBuffer.isView(v)) {
            if (v.byteLength >= 65536) addMemory({ buffer: v.buffer }, "array " + path.concat(key).join("."));
            continue;
          }
          if (v instanceof ArrayBuffer || (typeof SharedArrayBuffer !== "undefined" && v instanceof SharedArrayBuffer)) {
            if (v.byteLength >= 65536) addMemory({ buffer: v }, "buffer " + path.concat(key).join("."));
            continue;
          }
          queue.push({ obj: v, path: path.concat(key) });
        }
      }
    }
    return out;
  }

  function resolve(path) {
    let obj = window;
    for (let i = 0; i < path.length - 1; i++) {
      obj = obj == null ? undefined : obj[path[i]];
    }
    if (obj == null || typeof obj !== "object") throw new Error("that value isn't there any more");
    return { obj, key: path[path.length - 1] };
  }

  // ----- numbers in memory ---------------------------------------------------------------------
  const SIZE = { u8: 1, i16: 2, i32: 4, f32: 4, f64: 8 };
  function read(dv, off, t) {
    switch (t) {
      case "u8":
        return dv.getUint8(off);
      case "i16":
        return dv.getInt16(off, true);
      case "i32":
        return dv.getInt32(off, true);
      case "f32":
        return dv.getFloat32(off, true);
      default:
        return dv.getFloat64(off, true);
    }
  }
  function write(dv, off, t, v) {
    switch (t) {
      case "u8":
        return dv.setUint8(off, v);
      case "i16":
        return dv.setInt16(off, v, true);
      case "i32":
        return dv.setInt32(off, v, true);
      case "f32":
        return dv.setFloat32(off, v, true);
      default:
        return dv.setFloat64(off, v, true);
    }
  }
  const isFloat = (t) => t === "f32" || t === "f64";
  /** Decimal numbers match when they round to the value (Cheat Engine's "rounded"). */
  const same = (t, x, v) => (isFloat(t) || typeof x === "number" && !Number.isInteger(x) ? Math.abs(x - v) < (Number.isInteger(v) ? 0.5 : 0.001) : x === v);

  // ----- a search --------------------------------------------------------------------------------
  const MAX_HITS = 2000000;
  /** hits: memory ones in typed arrays (mem, offset, type code), JS ones as objects */
  let search = null;
  const TYPE_CODES = ["u8", "i16", "i32", "f32", "f64"];

  function firstSearch(value, kind) {
    const types = kind === "whole" ? ["i32", "i16"] : kind === "decimal" ? ["f32", "f64"] : ["i32", "f32", "f64"];
    const mem = [];
    const off = [];
    const tc = [];
    let tooMany = false;
    for (let i = 0; i < memories.length && !tooMany; i++) {
      let buf;
      try {
        buf = bufferOf(i);
      } catch {
        continue;
      }
      // A place that matched as a bigger number isn't listed again as a smaller one.
      const taken = new Set();
      for (const t of types) {
        const n = Math.floor(buf.byteLength / SIZE[t]);
        const arr =
          t === "i32" ? new Int32Array(buf, 0, n) : t === "i16" ? new Int16Array(buf, 0, n) : t === "f32" ? new Float32Array(buf, 0, n) : new Float64Array(buf, 0, n);
        if (!isFloat(t) && !Number.isInteger(value)) continue;
        for (let j = 0; j < n; j++) {
          if (same(t, arr[j], value)) {
            const at = j * SIZE[t];
            if (taken.has(at)) continue;
            if (mem.length >= MAX_HITS) {
              tooMany = true;
              break;
            }
            taken.add(at);
            mem.push(i);
            off.push(at);
            tc.push(TYPE_CODES.indexOf(t));
          }
        }
      }
    }
    const js = kind === "decimal" ? walk((v) => !Number.isInteger(v) && same("f64", v, value)) : walk((v) => same("i32", v, value) || (kind !== "whole" && same("f64", v, value)));
    search = { mem: Int32Array.from(mem), off: Float64Array.from(off), tc: Uint8Array.from(tc), js };
    remember();
    return summary(tooMany);
  }

  /** Current value of hit `n` (memory hits first, then JS ones). */
  function hitValue(n) {
    const m = search.mem.length;
    if (n < m) {
      const buf = bufferOf(search.mem[n]);
      const t = TYPE_CODES[search.tc[n]];
      if (search.off[n] + SIZE[t] > buf.byteLength) return NaN;
      return read(new DataView(buf), search.off[n], t);
    }
    const h = search.js[n - m];
    const v = h.obj[h.key];
    return typeof v === "number" ? v : NaN;
  }
  function hitCount() {
    return search ? search.mem.length + search.js.length : 0;
  }
  /** Remembers every hit's value, for "changed / went up" searches. */
  function remember() {
    const count = hitCount();
    search.values = new Float64Array(count);
    for (let n = 0; n < count; n++) {
      try {
        search.values[n] = hitValue(n);
      } catch {
        search.values[n] = NaN;
      }
    }
  }

  function nextSearch(how, value) {
    if (!search) throw new Error("search for a value first");
    const keepMem = [];
    const keepJs = [];
    const m = search.mem.length;
    const count = hitCount();
    for (let n = 0; n < count; n++) {
      let now;
      try {
        now = hitValue(n);
      } catch {
        continue;
      }
      const before = search.values[n];
      const t = n < m ? TYPE_CODES[search.tc[n]] : Number.isInteger(now) ? "i32" : "f64";
      const ok =
        how === "equals" ? same(t, now, value) : how === "changed" ? now !== before : how === "unchanged" ? now === before : how === "increased" ? now > before : how === "decreased" ? now < before : false;
      if (ok && !Number.isNaN(now)) (n < m ? keepMem : keepJs).push(n);
    }
    search = {
      mem: Int32Array.from(keepMem, (n) => search.mem[n]),
      off: Float64Array.from(keepMem, (n) => search.off[n]),
      tc: Uint8Array.from(keepMem, (n) => search.tc[n]),
      js: keepJs.map((n) => search.js[n - m]),
    };
    remember();
    return summary(false);
  }

  /** How a hit is found again (saved with a cheat). */
  function targetOf(n) {
    const m = search.mem.length;
    if (n < m) {
      const i = search.mem[n];
      return { k: "m", mem: i, label: memories[i].label, size: bufferOf(i).byteLength, off: search.off[n], t: TYPE_CODES[search.tc[n]] };
    }
    return { k: "j", path: search.js[n - m].path.map(String) };
  }
  function where(target) {
    return target.k === "m" ? `memory ${target.mem + 1} at 0x${target.off.toString(16).toUpperCase()} (${target.t})` : target.path.join(".");
  }

  function summary(tooMany) {
    const count = hitCount();
    const list = [];
    for (let n = 0; n < Math.min(count, 100); n++) {
      const target = targetOf(n);
      list.push({ id: n, target, where: where(target), value: hitValue(n) });
    }
    return { count, tooMany, results: list };
  }

  // ----- reading, writing, locking ------------------------------------------------------------
  function readTarget(target) {
    if (target.k === "m") {
      const buf = bufferOf(target.mem);
      if (target.off + SIZE[target.t] > buf.byteLength) throw new Error("outside the memory");
      return read(new DataView(buf), target.off, target.t);
    }
    const { obj, key } = resolve(target.path);
    const v = obj[key];
    if (typeof v !== "number") throw new Error("that value isn't a number any more");
    return v;
  }
  function writeTarget(target, value) {
    if (typeof value !== "number" || !Number.isFinite(value)) throw new Error("not a number");
    if (target.k === "m") {
      const buf = bufferOf(target.mem);
      if (!SIZE[target.t] || target.off + SIZE[target.t] > buf.byteLength) throw new Error("outside the memory");
      write(new DataView(buf), target.off, target.t, value);
      return;
    }
    const { obj, key } = resolve(target.path);
    if (typeof obj[key] !== "number") throw new Error("that value isn't a number any more");
    obj[key] = value;
  }

  /** id -> { target, value } */
  const locks = new Map();
  setInterval(() => {
    for (const { target, value } of locks.values()) {
      try {
        if (readTarget(target) !== value) writeTarget(target, value);
      } catch {
        // Gone for now (a reload or a new level); tried again next time.
      }
    }
  }, 100);

  // ----- keys (v1.19.0) ------------------------------------------------------------------------
  // The player's keys for their cheats work while the game window is in front. They're listened
  // for here, before the game's own handlers, and the game doesn't see a key that belongs to a cheat.
  const NUMPAD = { NumpadAdd: "Num+", NumpadSubtract: "Num-", NumpadMultiply: "Num*", NumpadDivide: "Num/", NumpadDecimal: "Num." };
  const NAMED = {
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    PageUp: "PageUp",
    PageDown: "PageDown",
    Home: "Home",
    End: "End",
    Insert: "Insert",
    Delete: "Delete",
    Pause: "Pause",
    Space: "Space",
  };
  /** Same names as OpenTrainer's key boxes (`Ctrl+F1`, `Num5`, `G`). */
  function keyText(e) {
    const c = String(e.code || "");
    let key = null;
    if (/^F([1-9]|1\d|2[0-4])$/.test(c)) key = c;
    else if (/^Numpad\d$/.test(c)) key = "Num" + c.slice(6);
    else if (NUMPAD[c]) key = NUMPAD[c];
    else if (/^Key[A-Z]$/.test(c)) key = c.slice(3);
    else if (/^Digit\d$/.test(c)) key = c.slice(5);
    else if (NAMED[c]) key = NAMED[c];
    if (!key) return null;
    return [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", key].filter(Boolean).join("+");
  }
  const typing = (el) => !!el && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName || ""));

  /** key text -> [{ id, name, does, target, value }] */
  let keys = new Map();
  let keysTag = "";

  // A short note in the corner of the game ("Health locked"), so the player sees the key worked.
  let note = null;
  let noteTimer = 0;
  function show(text) {
    try {
      if (!note) {
        const host = document.createElement("div");
        const root = host.attachShadow({ mode: "closed" });
        note = document.createElement("div");
        root.appendChild(note);
        const s = host.style;
        s.position = "fixed";
        s.right = "16px";
        s.bottom = "16px";
        s.zIndex = "2147483647";
        s.pointerEvents = "none";
        const n = note.style;
        n.font = "600 14px system-ui, sans-serif";
        n.color = "#ecfdf5";
        n.background = "rgba(6, 78, 59, 0.92)";
        n.padding = "8px 12px";
        n.borderRadius = "10px";
        n.boxShadow = "0 4px 16px rgba(0,0,0,.4)";
        note.host = host;
      }
      note.textContent = "OpenTrainer: " + text;
      if (!note.host.isConnected) (document.body || document.documentElement).appendChild(note.host);
      note.host.style.display = "block";
      clearTimeout(noteTimer);
      noteTimer = setTimeout(() => {
        note.host.style.display = "none";
      }, 1600);
    } catch {
      // No page to show it on yet.
    }
  }

  function onKey(e) {
    if (!keys.size || e.repeat) return;
    const text = keyText(e);
    const list = text && keys.get(text);
    if (!list) return;
    // A plain letter or digit typed into a box (a chat) is the player typing, not a cheat.
    if (typing(e.target) && !e.ctrlKey && !e.altKey && !/^(F\d+|Num)/.test(text.split("+").pop())) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    for (const k of list) {
      try {
        if (k.does === "set") {
          writeTarget(k.target, k.value);
          show(`${k.name} set to ${k.value}`);
        } else if (locks.has(k.id)) {
          locks.delete(k.id);
          show(`${k.name} unlocked`);
        } else {
          const value = typeof k.value === "number" ? k.value : readTarget(k.target);
          writeTarget(k.target, value);
          locks.set(k.id, { target: k.target, value });
          show(`${k.name} locked`);
        }
      } catch (err) {
        show(`${k.name}: ${err && err.message ? err.message : err}`);
      }
    }
  }
  window.addEventListener("keydown", onKey, true);

  const methods = {
    info() {
      walk(() => false); // also finds big byte arrays the hooks missed (e.g. Module.HEAPU8)
      return {
        url: location.href,
        title: document.title,
        memories: memories.map((x, i) => {
          let bytes = 0;
          try {
            bytes = x.m.buffer.byteLength;
          } catch {
            // Detached.
          }
          return { index: i, label: x.label, bytes };
        }),
        locks: locks.size,
      };
    },
    find({ value, kind }) {
      if (typeof value !== "number" || !Number.isFinite(value)) throw new Error("type a number");
      return firstSearch(value, kind);
    },
    next({ how, value }) {
      return nextSearch(how, value);
    },
    results() {
      return search ? summary(false) : { count: 0, tooMany: false, results: [] };
    },
    read({ targets }) {
      return targets.map((t) => {
        try {
          return { value: readTarget(t) };
        } catch (e) {
          return { error: String(e && e.message ? e.message : e) };
        }
      });
    },
    set({ target, value }) {
      writeTarget(target, value);
      return { value: readTarget(target) };
    },
    lock({ id, target, value }) {
      writeTarget(target, value);
      locks.set(String(id), { target, value });
      return { locked: locks.size };
    },
    unlock({ id }) {
      if (id == null) locks.clear();
      else locks.delete(String(id));
      return { locked: locks.size };
    },
    /** The cheats' keys: [{ id, name, key, does, target, value }], and a tag to tell the list apart. */
    keys({ list, tag }) {
      const next = new Map();
      for (const k of Array.isArray(list) ? list : []) {
        if (!k || typeof k.key !== "string" || !k.target) continue;
        const entry = { id: String(k.id), name: String(k.name), does: k.does === "set" ? "set" : "lock", target: k.target, value: k.value };
        if (entry.does === "set" && typeof entry.value !== "number") continue;
        if (!next.has(k.key)) next.set(k.key, []);
        next.get(k.key).push(entry);
      }
      keys = next;
      keysTag = String(tag || "");
      return { keys: keys.size };
    },
    /** Numbers whose name (or their object's name and theirs, e.g. "player.hp") contains one of
     * the words in `query` (split by | or ,), for the AI helper (v1.20.0). */
    names({ query, limit }) {
      const words = String(query == null ? "" : query)
        .toLowerCase()
        .split(/[|,]/)
        .map((w) => w.trim())
        .filter(Boolean)
        .slice(0, 10);
      if (!words.length) throw new Error("say which name to look for");
      const max = Math.min(Math.max(Number(limit) || 50, 1), 200);
      const hits = walk((v, key, path) => {
        const text = (path.length ? String(path[path.length - 1]) + "." : "") + String(key);
        const low = text.toLowerCase();
        return words.some((w) => low.includes(w));
      });
      return {
        count: hits.length,
        results: hits.slice(0, max).map((h) => {
          const target = { k: "j", path: h.path.map(String) };
          return { target, where: where(target), value: h.obj[h.key] };
        }),
      };
    },
    /** What's locked (also by a key) and which keys list is in use. */
    state() {
      return { locks: [...locks.keys()], keysTag };
    },
  };

  const agent = {
    version: 3,
    callJson(method, args) {
      try {
        const f = Object.prototype.hasOwnProperty.call(methods, method) ? methods[method] : null;
        if (!f) return stringify({ error: "unknown call " + method });
        return stringify({ ok: f(args || {}) });
      } catch (e) {
        return stringify({ error: String(e && e.message ? e.message : e) });
      }
    },
  };
  Object.freeze(agent);
  Object.defineProperty(window, "__otAgent", { value: agent, writable: false, enumerable: false, configurable: false });
})();
