// JuiceBox Trainer phone remote. Shows the loaded pack's cheats and sends the same
// actions as the trainer's own buttons. Text is always set with textContent.
"use strict";

const main = document.getElementById("cheats");
const statusLine = document.getElementById("status");
const title = document.getElementById("game");
let shape = "";
let rows = new Map();

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

async function send(body) {
  try {
    const r = await fetch("/api/action", {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-JuiceBox": "1" },
      body: JSON.stringify(body),
    });
    const data = await r.json();
    if (!r.ok) statusLine.textContent = data.error || "That didn't work.";
    else update(data);
  } catch {
    statusLine.textContent = "Can't reach JuiceBox Trainer.";
  }
}

function build(view) {
  main.replaceChildren();
  rows = new Map();
  let category = null;
  for (const c of view.cheats) {
    if (c.category !== category) {
      category = c.category;
      main.append(el("h2", "", category || "Cheats"));
    }
    const row = el("div", "row");
    const name = el("div", "name", c.name);
    const msg = el("span", "msg");
    name.append(msg);
    row.append(name);
    const parts = { row, msg };
    if (c.ui === "toggle") {
      const b = el("button", "", "Off");
      b.onclick = () => send({ id: c.id, op: "toggle", on: !parts.active });
      parts.button = b;
      row.append(b);
    } else if (c.ui === "button") {
      const b = el("button", "", c.label || "Go");
      b.onclick = () => send({ id: c.id, op: "press" });
      parts.button = b;
      row.append(b);
    } else {
      const input = el("input");
      input.type = "number";
      input.inputMode = "decimal";
      if (c.min !== null) input.min = c.min;
      if (c.max !== null) input.max = c.max;
      if (c.step !== null) input.step = c.step;
      const b = el("button", "", "Set");
      b.onclick = () => input.value !== "" && send({ id: c.id, op: "set", value: input.value });
      parts.input = input;
      parts.button = b;
      row.append(input, b);
    }
    rows.set(c.id, parts);
    main.append(row);
  }
}

function update(view) {
  if (!view.pack) {
    shape = "";
    title.textContent = "JuiceBox Trainer";
    statusLine.textContent = "Connected";
    main.replaceChildren(el("p", "empty", "Load a pack in JuiceBox Trainer on your PC, then its cheats show here."));
    return;
  }
  title.textContent = view.pack.game;
  statusLine.textContent = "Connected";
  const s = view.pack.game + "|" + view.cheats.map((c) => c.id).join(",");
  if (s !== shape) {
    shape = s;
    build(view);
  }
  for (const c of view.cheats) {
    const p = rows.get(c.id);
    if (!p) continue;
    p.active = c.active;
    p.msg.textContent = c.status === "failed" ? c.message || "Not available" : "";
    p.button.disabled = c.status === "failed";
    if (c.ui === "toggle") {
      p.button.textContent = c.active ? "On" : "Off";
      p.button.className = c.active ? "on" : "";
    }
    if (p.input && document.activeElement !== p.input && c.value !== null) {
      p.input.placeholder = c.value;
    }
  }
}

async function poll() {
  try {
    const r = await fetch("/api/state", { cache: "no-store" });
    if (r.status === 401) {
      statusLine.textContent = "Not paired any more. Scan the QR code in JuiceBox Trainer again.";
      main.replaceChildren();
      shape = "";
      return;
    }
    update(await r.json());
  } catch {
    statusLine.textContent = "Can't reach JuiceBox Trainer. Is it still running?";
  }
}

poll();
setInterval(poll, 1000);
