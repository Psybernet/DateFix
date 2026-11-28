"use strict";

// ================= Backend =================
// Inside the app this talks to the Rust side. Opened in a normal browser
// (for design work or screenshots) it uses made-up demo data instead.
const tauri = window.__TAURI__;
const api = tauri ? {
  invoke: (cmd, args) => tauri.core.invoke(cmd, args),
  listen: (event, fn) => tauri.event.listen(event, (e) => fn(e.payload)),
  pickFolders: () => tauri.dialog.open({ directory: true, multiple: true, title: "Choose folders to tidy" }),
  onDrop: (fn) => tauri.webview.getCurrentWebview().onDragDropEvent((e) => fn(e.payload)),
  window: tauri.window.getCurrentWindow(),
  version: () => tauri.app.getVersion(),
} : demoBackend();

// ================= State =================
const ROW = 30; // px, matches --row in styles.css
const S = {
  sources: [],      // folders the user added
  scanned: [],      // [{path, name}] from the last scan
  items: [],
  rows: [],         // items shown, filtered and sorted
  tab: "all",
  filter: "",
  view: "list",
  viewChosen: false,
  sort: { key: "status", dir: -1 },
  tree: null,       // { roots, nodes: Map }
  node: null,       // selected tree node key
  closed: new Set(),// collapsed tree nodes
  busy: false,
  scanSeq: 0,
  batches: [],
  batchId: null,
  checks: [],
  checkSeq: 0,
  stale: false,     // files changed by an undo: scan again on return
};

const $ = (id) => document.getElementById(id);
const els = {
  chips: $("chips"), addFolder: $("add-folder"), rescan: $("rescan"),
  recursive: $("opt-recursive"), folders: $("opt-folders"), order: $("opt-order"), clash: $("opt-clash"),
  filter: $("filter"), tabs: $("tabs"), view: $("view"),
  content: $("content"), tree: $("tree"), crumb: $("crumb"), thead: $("thead"), tbody: $("tbody"), vspace: $("vspace"),
  empty: $("empty"), dropzone: $("dropzone"), nothing: $("nothing"), nothingText: $("nothing-text"),
  busy: $("busy"), busyTitle: $("busy-title"), busyBar: $("busy-bar"), busyText: $("busy-text"), cancelScan: $("cancel-scan"),
  info: $("info"), undoLast: $("undo-last"), undoLabel: $("undo-label"), apply: $("apply"),
  pageMain: $("page-main"), pageHistory: $("page-history"), navHistory: $("nav-history"), back: $("back"),
  batches: $("batches"), dhead: $("dhead"), htable: $("htable"), hthead: $("hthead"), htbody: $("htbody"), hvspace: $("hvspace"),
  hstate: $("hstate"), hinfo: $("hinfo"), forget: $("forget"), resume: $("resume"), hundo: $("hundo"),
  theme: $("theme"), themeLabel: $("theme-label"), version: $("version"),
  overlay: $("drop-overlay"), modal: $("modal"), toasts: $("toasts"),
};

// ================= Helpers =================
const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const fmt = (n) => n.toLocaleString("en-AU");
const plural = (n, word, many = word + "s") => `${fmt(n)} ${n === 1 ? word : many}`;
const baseName = (p) => String(p).split(/[\\/]/).filter(Boolean).pop() || String(p);
const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
const MON = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

function longDate(iso) {
  const [y, m, d] = iso.split("-").map(Number);
  return y ? `${d} ${MON[m - 1]} ${y}` : "";
}

function when(seconds) {
  const date = new Date(seconds * 1000);
  const time = date.toLocaleTimeString("en-AU", { hour: "numeric", minute: "2-digit" });
  const day = new Date(); day.setHours(0, 0, 0, 0);
  const diff = (day - new Date(date).setHours(0, 0, 0, 0)) / 86400000;
  if (diff === 0) return `Today, ${time}`;
  if (diff === 1) return `Yesterday, ${time}`;
  return `${date.toLocaleDateString("en-AU", { day: "numeric", month: "short", year: "numeric" })}, ${time}`;
}

function ago(seconds) {
  const s = Math.max(0, Date.now() / 1000 - seconds);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.round(s / 60)} min ago`;
  if (s < 86400) return `${Math.round(s / 3600)} h ago`;
  if (s < 172800) return "yesterday";
  return new Date(seconds * 1000).toLocaleDateString("en-AU", { day: "numeric", month: "short" });
}

/** The name with the first occurrence of `part` wrapped in a span. */
function marked(name, part, cls) {
  const at = part ? name.indexOf(part) : -1;
  if (at < 0) return esc(name);
  return esc(name.slice(0, at)) + `<span class="${cls}">${esc(part)}</span>` + esc(name.slice(at + part.length));
}

const ICON = {
  file: '<svg viewBox="0 0 16 16"><path d="M9.5 1.5H4a1 1 0 0 0-1 1v11a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V5z"/><path d="M9.5 1.5V5H13"/></svg>',
  folder: '<svg viewBox="0 0 16 16"><path d="M1.5 4.5a1 1 0 0 1 1-1h3.5l1.5 1.5h6a1 1 0 0 1 1 1v6.5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1z"/></svg>',
  warn: '<svg viewBox="0 0 16 16"><path d="M8 2l6.5 11.5h-13z"/><path d="M8 6.5v3M8 11.5v.01"/></svg>',
  swap: '<svg viewBox="0 0 16 16"><path d="M4 2.5v11M1.5 11L4 13.5 6.5 11M12 13.5v-11M9.5 5L12 2.5 14.5 5"/></svg>',
  lock: '<svg viewBox="0 0 16 16"><rect x="3" y="7" width="10" height="7" rx="1.5"/><path d="M5.5 7V5a2.5 2.5 0 0 1 5 0v2"/></svg>',
  block: '<svg viewBox="0 0 16 16"><circle cx="8" cy="8" r="6"/><path d="M3.8 12.2l8.4-8.4"/></svg>',
  check: '<svg viewBox="0 0 16 16"><path d="M3 8.5l3 3 7-7"/></svg>',
  fp: '<svg viewBox="0 0 16 16"><path d="M4.5 13.5c1-1.6 1.5-3.4 1.5-5.5a2 2 0 0 1 4 0c0 1.2-.1 2.4-.4 3.5M8 8c0 2.6-.7 4.6-2 6.5M3 10.5c.5-1 .7-2 .7-3a4.3 4.3 0 0 1 8.6 0c0 1.1-.1 2.2-.3 3.2M12 13c.2-.5.4-1 .5-1.6"/></svg>',
  shield: '<svg viewBox="0 0 16 16"><path d="M8 1.5l5.5 2v4c0 3.4-2.4 5.9-5.5 7-3.1-1.1-5.5-3.6-5.5-7v-4z"/><path d="M5.5 8l1.8 1.8L10.8 6"/></svg>',
  down: '<svg viewBox="0 0 12 12"><path d="M3 4.5l3 3 3-3"/></svg>',
  sortDown: '<svg viewBox="0 0 12 12"><path d="M3 7.5l3 3 3-3M6 1.5v9"/></svg>',
  sortUp: '<svg viewBox="0 0 12 12"><path d="M3 4.5l3-3 3 3M6 1.5v9"/></svg>',
  x: '<svg viewBox="0 0 10 10"><path d="M2 2l6 6M8 2l-6 6"/></svg>',
};
const badge = (cls, icon, text, tip = "") =>
  `<span class="badge ${cls}"${tip ? ` title="${esc(tip)}"` : ""}>${icon ? ICON[icon] : ""}${esc(text)}</span>`;

function toast(kind, title, lines = []) {
  const node = document.createElement("div");
  node.className = `toast ${kind}`;
  node.innerHTML = `<strong>${esc(title)}</strong>` +
    (lines.length ? `<ul>${lines.slice(0, 5).map((l) => `<li>${esc(l)}</li>`).join("")}${lines.length > 5 ? `<li>and ${lines.length - 5} more</li>` : ""}</ul>` : "");
  els.toasts.append(node);
  setTimeout(() => { node.classList.add("leaving"); setTimeout(() => node.remove(), 300); }, lines.length ? 8000 : 4000);
}

function summaryToast(r, verb) {
  const extra = [];
  if (r.skipped) extra.push(`${fmt(r.skipped)} skipped`);
  if (r.failed) extra.push(`${fmt(r.failed)} failed`);
  toast(r.failed ? "bad" : r.skipped ? "warn" : "good",
    `${verb} ${plural(r.renamed, "item")}${extra.length ? ` (${extra.join(", ")})` : ""}`, r.problems);
}

/** A dialog. Resolves with the chosen button's value (null if closed). */
function modal({ title, warn = false, body, buttons }) {
  return new Promise((resolve) => {
    $("modal-title").innerHTML = (warn ? ICON.warn : "") + esc(title);
    $("modal-body").innerHTML = body;
    const foot = $("modal-foot");
    foot.replaceChildren();
    const close = (value) => { els.modal.hidden = true; document.removeEventListener("keydown", onKey, true); resolve(value); };
    const onKey = (e) => { if (e.key === "Escape") { e.stopPropagation(); close(null); } };
    for (const b of buttons) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = `btn ${b.cls || ""}`;
      btn.textContent = b.label;
      btn.addEventListener("click", () => close(b.value));
      foot.append(btn);
    }
    document.addEventListener("keydown", onKey, true);
    els.modal.hidden = false;
    foot.lastElementChild?.focus();
  });
}

// ================= Virtual table =================
/** Draws only the rows that are on screen, so 100,000 rows scroll smoothly. */
class VList {
  constructor(body, space, draw) {
    this.body = body; this.space = space; this.drawRow = draw; this.rows = []; this.cols = ""; this.queued = false;
    body.addEventListener("scroll", () => this.schedule());
    new ResizeObserver(() => this.schedule()).observe(body);
  }
  set(rows, cols) {
    this.rows = rows; this.cols = cols;
    this.space.style.height = `${rows.length * ROW}px`;
    this.draw();
  }
  schedule() {
    if (this.queued) return;
    this.queued = true;
    requestAnimationFrame(() => { this.queued = false; this.draw(); });
  }
  draw() {
    const top = this.body.scrollTop, height = this.body.clientHeight || 600;
    const first = Math.max(0, Math.floor(top / ROW) - 8);
    const last = Math.min(this.rows.length, Math.ceil((top + height) / ROW) + 8);
    let html = "";
    for (let i = first; i < last; i++) {
      const [cls, cells] = this.drawRow(this.rows[i]);
      html += `<div class="tr ${cls}" data-row="${i}" style="top:${i * ROW}px;grid-template-columns:${this.cols}">${cells}</div>`;
    }
    this.space.innerHTML = html;
  }
  rowAt(target) {
    const tr = target.closest(".tr");
    return tr ? this.rows[Number(tr.dataset.row)] : null;
  }
}

// ================= Items =================
function prepare(it, i) {
  it.i = i;
  it.selected = !it.problem;
  const issues = [];
  if (it.problem === "in_use") issues.push({ sev: 3, cls: "bad", icon: "lock", text: "In use by another program", tip: "Close the program using it, then scan again." });
  if (it.problem === "no_permission") issues.push({ sev: 3, cls: "bad", icon: "block", text: "No permission to rename", tip: "Windows won't let you rename this item." });
  if (it.clash) issues.push({ sev: 2, cls: "warn", icon: "warn", clash: true, text: "", tip: "Something already has the new name." });
  if (it.dupe) issues.push({ sev: 2, cls: "warn", icon: "warn", text: "Same new name as another item", tip: "Another item here gets the same name, so one of them gets _2 or is skipped." });
  if (it.longPath) issues.push({ sev: 2, cls: "warn", icon: "warn", text: "Path over 260 characters", tip: "Some programs can't open files with very long paths." });
  if (it.ambiguous) {
    const [y, m, d] = it.iso.split("-");
    issues.push({ sev: 1, cls: "warn", icon: "swap", text: "Day or month?", tip: `Read as ${longDate(it.iso)}. Could also be ${longDate(`${y}-${d}-${m}`)}.` });
  }
  it.issues = issues;
  it.sev = issues.reduce((max, x) => Math.max(max, x.sev), 0);
  it.parts = it.location ? it.location.split(/[\\/]/) : [];
  it.text = `${it.oldName} ${it.newName} ${it.location}`.toLowerCase();
  return it;
}

function issueText(x) {
  if (!x.clash) return x.text;
  return els.clash.value === "skip" ? "Name taken · will be skipped" : "Name taken · adds _2";
}

function statusCell(it) {
  if (!it.issues.length) return `<span class="faint">Ready</span>`;
  const top = it.issues.reduce((a, b) => (b.sev > a.sev ? b : a));
  const tip = it.issues.map((x) => `${issueText(x)}: ${x.tip}`).join("\n");
  return badge(top.cls, top.icon, issueText(top) + (it.issues.length > 1 ? ` +${it.issues.length - 1}` : ""), tip);
}

function folderLabel(it) {
  const src = S.scanned[it.source];
  const base = S.scanned.length > 1 ? src.name : "";
  return [base, it.location].filter(Boolean).join("\\") || src.name;
}

const inTab = {
  all: () => true,
  bad: (it) => it.sev === 3,
  check: (it) => it.sev === 1 || it.sev === 2,
  off: (it) => !it.selected,
};

function inNode(it, node) {
  if (it.source !== node.source) return false;
  return node.parts.every((p, k) => it.parts[k] === p);
}

const SORTS = {
  status: (a, b) => a.sev - b.sev,
  name: (a, b) => collator.compare(a.oldName, b.oldName),
  new: (a, b) => collator.compare(a.newName, b.newName),
  date: (a, b) => (a.iso < b.iso ? -1 : a.iso > b.iso ? 1 : 0),
  folder: (a, b) => collator.compare(folderLabel(a), folderLabel(b)),
};

function computeRows() {
  const q = S.filter.trim().toLowerCase();
  const node = S.view === "tree" && S.tree ? S.tree.nodes.get(S.node) : null;
  const rows = S.items.filter((it) => inTab[S.tab](it) && (!q || it.text.includes(q)) && (!node || inNode(it, node)));
  const cmp = SORTS[S.sort.key];
  rows.sort((a, b) => cmp(a, b) * S.sort.dir || a.i - b.i);
  S.rows = rows;
}

// ================= Folder tree =================
function buildTree() {
  const nodes = new Map();
  const roots = [];
  const node = (source, parts) => {
    const key = `${source}|${parts.join("\\")}`;
    let n = nodes.get(key);
    if (!n) {
      n = { key, source, parts, name: parts.length ? parts[parts.length - 1] : S.scanned[source].name, children: [], items: [] };
      nodes.set(key, n);
      if (parts.length) node(source, parts.slice(0, -1)).children.push(n);
      else roots.push(n);
    }
    return n;
  };
  S.scanned.forEach((_, source) => node(source, []));
  for (const it of S.items) {
    for (let k = 0; k <= it.parts.length; k++) node(it.source, it.parts.slice(0, k)).items.push(it);
  }
  for (const n of nodes.values()) n.children.sort((a, b) => collator.compare(a.name, b.name));
  S.tree = { roots, nodes };
  if (!nodes.has(S.node)) S.node = roots[0]?.key ?? null;
}

function renderTree() {
  if (S.view !== "tree" || !S.tree) return;
  let html = `<div class="label">${plural(S.scanned.length, "source")}</div>`;
  const walk = (n, depth) => {
    const sel = n.items.filter((it) => it.selected).length;
    const worst = n.items.reduce((m, it) => Math.max(m, it.sev), 0);
    const open = !S.closed.has(n.key);
    html += `<div class="node${n.key === S.node ? " on" : ""}${depth === 0 ? " root" : ""}" data-key="${esc(n.key)}" style="padding-left:${6 + depth * 16}px" title="${esc(n.name)}">
      ${n.children.length ? `<button class="chev${open ? "" : " closed"}" type="button" data-act="toggle" aria-label="${open ? "Collapse" : "Expand"}">${ICON.down}</button>` : `<span class="chev"></span>`}
      <input type="checkbox" data-act="check" aria-label="Rename everything in ${esc(n.name)}"${sel && sel === n.items.length ? " checked" : ""}${n.items.length ? "" : " disabled"} data-mixed="${sel > 0 && sel < n.items.length}">
      <span class="fold">${ICON.folder}</span><span class="nm">${esc(n.name)}</span>
      ${worst === 3 ? '<span class="dot bad"></span>' : worst ? '<span class="dot warn"></span>' : ""}
      <span class="count">${fmt(n.items.length)}</span></div>`;
    if (open) n.children.forEach((c) => walk(c, depth + 1));
  };
  S.tree.roots.forEach((r) => walk(r, 0));
  els.tree.innerHTML = html;
  els.tree.querySelectorAll('input[data-mixed="true"]').forEach((b) => { b.indeterminate = true; });
}

els.tree.addEventListener("click", (e) => {
  const row = e.target.closest(".node");
  if (!row) return;
  const n = S.tree.nodes.get(row.dataset.key);
  const act = e.target.closest("[data-act]")?.dataset.act;
  if (act === "toggle") {
    S.closed.has(n.key) ? S.closed.delete(n.key) : S.closed.add(n.key);
  } else if (act === "check") {
    const on = e.target.checked;
    n.items.forEach((it) => { it.selected = on; });
  } else {
    S.node = n.key;
  }
  refresh();
});

// ================= Main page rendering =================
const list = new VList(els.tbody, els.vspace, (it) => {
  const cells = [
    `<input type="checkbox" aria-label="Rename ${esc(it.oldName)}"${it.selected ? " checked" : ""}>`,
    `<span class="ic${it.isDir ? " dir" : ""}">${it.isDir ? ICON.folder : ICON.file}</span>`,
    `<span title="${esc(it.oldName)}">${marked(it.oldName, it.oldDate, "d-old")}</span>`,
    `<span class="arrow">→</span>`,
    `<span title="${esc(it.newName)}">${marked(it.newName, it.iso, "d-new")}</span>`,
    `<span class="muted">${longDate(it.iso)}</span>`,
    S.view === "list" ? `<span class="muted" title="${esc(S.scanned[it.source].path)}">${esc(folderLabel(it))}</span>` : "",
    statusCell(it),
  ];
  return [`${it.sev === 3 ? "bad" : ""}${it.selected ? "" : " off"}`, cells.join("")];
});

function columns() {
  return S.view === "list"
    ? "24px 18px minmax(0,1.25fr) 14px minmax(0,1.25fr) 92px minmax(0,.75fr) 220px"
    : "24px 18px minmax(0,1.2fr) 14px minmax(0,1.2fr) 92px 220px";
}

function renderHead() {
  const sel = S.rows.filter((it) => it.selected).length;
  const head = (key, label) => {
    const on = S.sort.key === key;
    const extra = on && key === "status" && S.sort.dir < 0 ? ": most urgent first" : "";
    return `<button type="button" class="sort${on ? " on" : ""}" data-sort="${key}">${label}${extra} ${on ? (S.sort.dir < 0 ? ICON.sortDown : ICON.sortUp) : ""}</button>`;
  };
  els.thead.style.gridTemplateColumns = columns();
  els.thead.innerHTML = `<input type="checkbox" id="check-all" aria-label="Tick everything shown"${sel && sel === S.rows.length ? " checked" : ""}${S.rows.length ? "" : " disabled"}>
    <span></span>${head("name", "Current name")}<span></span>${head("new", "New name")}${head("date", "Date")}
    ${S.view === "list" ? head("folder", "Folder") : ""}${head("status", "Status")}`;
  $("check-all").indeterminate = sel > 0 && sel < S.rows.length;
}

function renderCounts() {
  const n = { all: S.items.length, bad: 0, check: 0, off: 0 };
  for (const it of S.items) {
    if (it.sev === 3) n.bad++; else if (it.sev) n.check++;
    if (!it.selected) n.off++;
  }
  for (const el of els.tabs.querySelectorAll("[data-count]")) {
    const v = n[el.dataset.count];
    el.textContent = fmt(v);
    el.toggleAttribute("data-zero", v === 0);
  }
  els.tabs.querySelectorAll(".tab").forEach((t) => t.classList.toggle("on", t.dataset.tab === S.tab));
  const go = S.items.filter((it) => it.selected).length;
  els.info.innerHTML = S.items.length
    ? `<span>${plural(S.items.length, "name")} to fix · ${fmt(go)} selected</span>` +
      (n.bad ? `<span><span class="dot bad"></span>${fmt(n.bad)} can't be renamed</span>` : "") +
      (n.check ? `<span><span class="dot warn"></span>${fmt(n.check)} to check</span>` : "")
    : (S.scanned.length && !S.busy ? `<span>Looked at ${plural(S.checked || 0, "item")}</span>` : "");
  els.apply.disabled = S.busy || go === 0;
  els.apply.textContent = go ? `Rename ${plural(go, "item")}` : "Rename";
}

function renderChips() {
  if (!S.sources.length) {
    els.chips.innerHTML = '<span class="placeholder">No folder chosen yet</span>';
  } else {
    els.chips.innerHTML = S.sources.map((p, k) => {
      const scanned = S.scanned.findIndex((s) => s.path === p);
      const n = scanned < 0 ? null : S.items.filter((it) => it.source === scanned).length;
      const label = S.sources.length === 1 ? p : baseName(p);
      return `<span class="chip" title="${esc(p)}"><span class="label">${esc(label)}</span>${n == null ? "" : `<span class="n">${fmt(n)}</span>`}
        <button type="button" data-remove="${k}" aria-label="Remove ${esc(baseName(p))}">${ICON.x}</button></span>`;
    }).join("");
  }
  els.rescan.disabled = !S.sources.length || S.busy;
}

function renderState() {
  const show = (el, on) => { el.hidden = !on; };
  const has = S.items.length > 0;
  show(els.busy, S.busy);
  show(els.empty, !S.busy && !S.sources.length);
  show(els.nothing, !S.busy && S.sources.length > 0 && !has);
  show(els.content, !S.busy && has);
  els.tree.hidden = S.view !== "tree";
  els.crumb.hidden = S.view !== "tree";
  els.view.querySelectorAll("button").forEach((b) => b.setAttribute("aria-checked", String(b.dataset.view === S.view)));
}

function renderCrumb() {
  if (S.view !== "tree" || !S.tree) return;
  const n = S.tree.nodes.get(S.node);
  if (!n) { els.crumb.textContent = ""; return; }
  const trail = [S.scanned[n.source].name, ...n.parts];
  els.crumb.innerHTML = trail.slice(0, -1).map((p) => `<span class="muted">${esc(p)} ›</span>`).join(" ") +
    ` ${esc(trail[trail.length - 1])} <span class="muted">· ${plural(n.items.length, "name")}${S.rows.length !== n.items.length ? `, ${fmt(S.rows.length)} shown` : ""}</span>`;
}

/** Recomputes what's shown and redraws everything on the main page. */
function refresh() {
  computeRows();
  renderState();
  renderChips();
  renderCounts();
  renderTree();
  renderCrumb();
  renderHead();
  list.set(S.rows, columns());
}

// ================= Main page actions =================
function addSources(paths) {
  const added = paths.filter((p) => p && !S.sources.includes(p));
  if (!added.length) return;
  S.sources.push(...added);
  scan();
}

async function chooseFolders() {
  const picked = await api.pickFolders();
  if (picked) addSources([].concat(picked));
}

function setBusy(title, { bar = null, cancel = false, text = "" } = {}) {
  els.busyTitle.textContent = title;
  els.busyText.textContent = text;
  els.busyBar.classList.toggle("endless", bar == null);
  els.busyBar.firstElementChild.style.width = bar == null ? "" : `${Math.round(bar * 100)}%`;
  els.cancelScan.hidden = !cancel;
}

async function scan() {
  const seq = ++S.scanSeq;
  if (!S.sources.length) {
    S.items = []; S.scanned = []; S.tree = null; S.busy = false;
    refresh();
    return;
  }
  S.busy = true;
  setBusy("Scanning…", { cancel: true, text: "Starting…" });
  refresh();
  try {
    const result = await api.invoke("scan", {
      folders: S.sources,
      recursive: els.recursive.checked,
      monthFirst: els.order.value === "month",
      includeDirs: els.folders.checked,
    });
    if (seq !== S.scanSeq) return;
    S.scanned = result.sources;
    S.checked = result.checked;
    S.items = result.items.map(prepare);
    buildTree();
    // One folder: a list. Several: the folder tree. Either can be switched.
    if (!S.viewChosen) S.view = S.scanned.length > 1 ? "tree" : "list";
    els.nothingText.textContent = `Looked at ${plural(result.checked, "item")}. Every date in these names is already YYYY-MM-DD.`;
  } catch (err) {
    if (seq !== S.scanSeq) return;
    if (String(err) === "cancelled") {
      toast("warn", "Scan cancelled");
      S.items = []; S.tree = null;
    } else {
      toast("bad", "Couldn't scan", [String(err)]);
    }
  } finally {
    if (seq === S.scanSeq) {
      S.busy = false;
      els.tbody.scrollTop = 0;
      refresh();
    }
  }
}

async function rename() {
  const chosen = S.items.filter((it) => it.selected); // kept in plan order: contents before folders
  if (!chosen.length || S.busy) return;
  S.busy = true;
  setBusy("Renaming…", { bar: 0, text: `0 of ${fmt(chosen.length)}` });
  refresh();
  try {
    const result = await api.invoke("rename", {
      folders: S.scanned.map((s) => s.path),
      changes: chosen.map(({ from, to, clash, isDir }) => ({ from, to, clash, is_dir: isDir })),
      onClash: els.clash.value,
    });
    summaryToast(result, "Renamed");
  } catch (err) {
    toast("bad", "Renaming didn't work", [String(err)]);
  } finally {
    S.busy = false;
    await loadHistory();
    await scan();
  }
}

els.addFolder.addEventListener("click", chooseFolders);
els.dropzone.addEventListener("click", chooseFolders);
els.rescan.addEventListener("click", () => scan());
els.cancelScan.addEventListener("click", () => api.invoke("cancel_scan"));
els.chips.addEventListener("click", (e) => {
  const btn = e.target.closest("[data-remove]");
  if (!btn) return;
  S.sources.splice(Number(btn.dataset.remove), 1);
  scan();
});
for (const opt of [els.recursive, els.folders, els.order]) opt.addEventListener("change", () => { savePrefs(); scan(); });
els.clash.addEventListener("change", () => { savePrefs(); refresh(); });
els.filter.addEventListener("input", () => { S.filter = els.filter.value; refresh(); });
els.tabs.addEventListener("click", (e) => {
  const tab = e.target.closest("[data-tab]");
  if (tab) { S.tab = tab.dataset.tab; els.tbody.scrollTop = 0; refresh(); }
});
els.view.addEventListener("click", (e) => {
  const b = e.target.closest("[data-view]");
  if (!b) return;
  S.view = b.dataset.view; S.viewChosen = true;
  refresh();
});
els.thead.addEventListener("click", (e) => {
  if (e.target.id === "check-all") {
    S.rows.forEach((it) => { it.selected = e.target.checked; });
    refresh();
    return;
  }
  const b = e.target.closest("[data-sort]");
  if (!b) return;
  const key = b.dataset.sort;
  S.sort = S.sort.key === key ? { key, dir: -S.sort.dir } : { key, dir: key === "status" ? -1 : 1 };
  refresh();
});
els.vspace.addEventListener("click", (e) => {
  const it = list.rowAt(e.target);
  if (!it) return;
  it.selected = e.target.matches("input") ? e.target.checked : !it.selected;
  refresh();
});
els.apply.addEventListener("click", rename);

// ================= History page =================
const STATUS = {
  missing: { sev: 4, cls: "bad", icon: "warn", text: "Missing", tip: "It has been moved or deleted since." },
  old_name_taken: { sev: 4, cls: "bad", icon: "warn", text: "Old name is taken", tip: "Something else now has the old name, so it can't be put back." },
  contents_changed: { sev: 3, cls: "bad", icon: "warn", text: "Contents changed since", pick: true, tip: "Same name, different contents. Only undone if you tick it." },
  renamed_since: { sev: 2, cls: "warn", icon: "fp", text: "Renamed since · found by fingerprint", pick: true, on: true, tip: "It was renamed again after DateFix, and found by its fingerprint." },
  matches: { sev: 1, cls: "ok", icon: "check", text: "Matches", pick: true, on: true },
  already_back: { sev: 0, cls: "plain", text: "Already has its old name" },
  undone: { sev: 0, cls: "plain", text: "Put back" },
};
const STATE = {
  interrupted: (b) => badge("bad", "warn", "Interrupted"),
  complete: () => badge("purple", "shield", "Can be undone"),
  partly_undone: (b) => badge("warn", "", `${fmt(b.undone)} put back`),
  undone: () => badge("", "check", "Undone"),
};

const hlist = new VList(els.htbody, els.hvspace, (c) => {
  const s = STATUS[c.status];
  const cells = [
    `<input type="checkbox" aria-label="Undo ${esc(c.nowName)}"${c.selected ? " checked" : ""}${s.pick ? "" : " disabled"}>`,
    `<span class="ic${c.isDir ? " dir" : ""}">${c.isDir ? ICON.folder : ICON.file}</span>`,
    `<span title="${esc(c.now || c.nowName)}">${esc(c.nowName)}</span>`,
    `<span class="arrow">→</span>`,
    `<span title="${esc(c.backTo)}">${esc(c.backName)}</span>`,
    `<span class="muted" title="${esc(c.folder)}">${esc(baseName(c.folder))}</span>`,
    badge(s.cls, s.icon, s.text, s.tip),
  ];
  return [`${s.sev >= 3 ? "bad" : ""}${c.selected ? "" : " off"}`, cells.join("")];
});
const HCOLS = "24px 18px minmax(0,1.2fr) 14px minmax(0,1.2fr) minmax(0,.6fr) 240px";

async function loadHistory() {
  S.batches = await api.invoke("history_list").catch(() => []);
  const last = S.batches.find((b) => b.renamed > b.undone);
  els.undoLast.hidden = !last;
  if (last) {
    els.undoLabel.innerHTML = `Undo last rename <span class="faint">· ${plural(last.renamed - last.undone, "item")}, ${ago(last.started)}</span>`;
    els.undoLast.dataset.id = last.id;
  }
  if (!els.pageHistory.hidden) renderBatches();
}

function batchWhere(b) {
  return b.folders.map(baseName).join(", ");
}

function renderBatches() {
  if (!S.batches.length) {
    els.batches.innerHTML = '<div class="state"><span class="muted">Nothing renamed yet.</span></div>';
    return;
  }
  els.batches.innerHTML = S.batches.map((b) => {
    const sub = b.state === "interrupted"
      ? `${batchWhere(b)} · stopped after ${fmt(b.renamed)} of ${fmt(b.planned)}`
      : `${batchWhere(b)} · ${plural(b.renamed, "item")} renamed`;
    return `<button type="button" class="batch${b.id === S.batchId ? " on" : ""}${b.state === "undone" ? " done" : ""}" data-id="${esc(b.id)}">
      <span class="t">${esc(when(b.started))}</span>${STATE[b.state](b)}<span class="s" title="${esc(b.folders.join("\n"))}">${esc(sub)}</span></button>`;
  }).join("");
}

function renderHistoryDetail() {
  const b = S.batches.find((x) => x.id === S.batchId);
  els.forget.hidden = els.resume.hidden = true;
  els.hundo.disabled = true;
  els.hundo.textContent = "Undo";
  els.hinfo.textContent = "";
  if (!b) {
    els.dhead.innerHTML = "";
    els.htable.hidden = true;
    els.hstate.hidden = false;
    els.hstate.innerHTML = '<span class="muted">Choose a rename on the left.</span>';
    return;
  }
  const counts = {};
  for (const c of S.checks) counts[c.status] = (counts[c.status] || 0) + 1;
  const pills = S.checks.length ? Object.entries(STATUS).filter(([k]) => counts[k]).map(([k, s]) =>
    badge(s.cls === "plain" ? "" : s.cls, s.icon, `${fmt(counts[k])} ${s.text.toLowerCase()}`)).join("") : "";
  els.dhead.innerHTML = `<h2>${esc(when(b.started))} · ${plural(b.renamed, "item")} renamed</h2>
    <div class="where" title="${esc(b.folders.join("\n"))}">${esc(b.folders.join(" · "))}</div>
    ${pills ? `<div class="checks">${pills}</div>` : ""}
    <div class="note">${ICON.shield}<div>Before renaming, DateFix saved each file's size and fingerprint and wrote down every step, so this list stays right after a crash or restart. Files renamed again since are found by their fingerprint. Files whose contents changed are left alone unless you tick them.</div></div>`;
  els.forget.hidden = false;
  if (b.state === "interrupted") {
    els.resume.hidden = false;
    els.resume.textContent = `Finish the other ${fmt(b.planned - b.renamed)}`;
  }
  if (S.checking) {
    els.htable.hidden = true;
    els.hstate.hidden = false;
    els.hstate.innerHTML = '<strong>Checking files…</strong><div class="progress endless"><span></span></div>';
    return;
  }
  els.hstate.hidden = S.checks.length > 0;
  els.hstate.innerHTML = '<span class="muted">Nothing in this rename can be undone.</span>';
  els.htable.hidden = !S.checks.length;
  const sel = S.checks.filter((c) => c.selected).length;
  const can = S.checks.filter((c) => STATUS[c.status].pick).length;
  els.hthead.style.gridTemplateColumns = HCOLS;
  els.hthead.innerHTML = `<input type="checkbox" id="hcheck-all" aria-label="Tick everything"${sel && sel === can ? " checked" : ""}${can ? "" : " disabled"}>
    <span></span><span>Name now</span><span></span><span>Goes back to</span><span>Folder</span><span class="sort on">Check ${ICON.sortDown}</span>`;
  $("hcheck-all").indeterminate = sel > 0 && sel < can;
  hlist.set(S.checks, HCOLS);
  els.hinfo.textContent = can ? `${fmt(can)} of ${plural(b.renamed - b.undone, "name")} can go back` : "";
  els.hundo.disabled = sel === 0 || S.busy;
  els.hundo.textContent = sel ? `Undo ${plural(sel, "rename")}` : "Undo";
}

async function selectBatch(id) {
  S.batchId = id;
  S.checks = [];
  S.checking = true;
  renderBatches();
  renderHistoryDetail();
  const seq = ++S.checkSeq;
  try {
    const checks = await api.invoke("history_check", { id });
    if (seq !== S.checkSeq) return;
    S.checks = checks.filter((c) => c.status !== "not_renamed").map((c) => ({ ...c, selected: !!STATUS[c.status].on }));
    S.checks.sort((a, b) => STATUS[b.status].sev - STATUS[a.status].sev || b.index - a.index);
  } catch (err) {
    if (seq === S.checkSeq) toast("bad", "Couldn't check the files", [String(err)]);
  } finally {
    if (seq === S.checkSeq) { S.checking = false; els.htbody.scrollTop = 0; renderHistoryDetail(); }
  }
}

async function openHistory(id) {
  els.pageMain.hidden = true;
  els.pageHistory.hidden = false;
  els.navHistory.classList.add("on");
  await loadHistory();
  const pick = id || S.batchId || S.batches[0]?.id;
  if (pick) selectBatch(pick); else { S.batchId = null; renderBatches(); renderHistoryDetail(); }
}

function closeHistory() {
  els.pageHistory.hidden = true;
  els.pageMain.hidden = false;
  els.navHistory.classList.remove("on");
  if (S.stale) { S.stale = false; scan(); }
}

async function historyAction(work) {
  S.busy = true;
  renderHistoryDetail();
  try {
    await work();
  } catch (err) {
    toast("bad", "That didn't work", [String(err)]);
  } finally {
    S.busy = false;
    S.stale = true;
    await loadHistory();
    if (S.batches.some((b) => b.id === S.batchId)) await selectBatch(S.batchId);
    else { S.batchId = S.batches[0]?.id ?? null; S.batchId ? await selectBatch(S.batchId) : renderHistoryDetail(); }
  }
}

els.navHistory.addEventListener("click", () => (els.pageHistory.hidden ? openHistory() : closeHistory()));
els.back.addEventListener("click", closeHistory);
els.undoLast.addEventListener("click", () => openHistory(els.undoLast.dataset.id));
els.batches.addEventListener("click", (e) => {
  const b = e.target.closest("[data-id]");
  if (b) selectBatch(b.dataset.id);
});
els.hthead.addEventListener("change", (e) => {
  if (e.target.id !== "hcheck-all") return;
  S.checks.forEach((c) => { if (STATUS[c.status].pick) c.selected = e.target.checked; });
  renderHistoryDetail();
});
els.hvspace.addEventListener("click", (e) => {
  const c = hlist.rowAt(e.target);
  if (!c || !STATUS[c.status].pick) return;
  c.selected = e.target.matches("input") ? e.target.checked : !c.selected;
  renderHistoryDetail();
});
els.hundo.addEventListener("click", () => historyAction(async () => {
  const indices = S.checks.filter((c) => c.selected).map((c) => c.index);
  summaryToast(await api.invoke("history_undo", { id: S.batchId, indices }), "Put back");
}));
els.resume.addEventListener("click", () => historyAction(async () => {
  summaryToast(await api.invoke("history_resume", { id: S.batchId, onClash: els.clash.value }), "Renamed");
}));
els.forget.addEventListener("click", async () => {
  const ok = await modal({
    title: "Forget this rename?",
    body: "<p>Its history is deleted. Your files aren't changed, but you won't be able to undo this rename any more.</p>",
    buttons: [{ label: "Cancel", value: false }, { label: "Forget it", cls: "danger", value: true }],
  });
  if (ok) historyAction(() => api.invoke("history_forget", { id: S.batchId }));
});

/** After a crash: offer to undo or finish the rename that was cut off. */
async function offerRecovery() {
  let b = S.batches.find((x) => x.state === "interrupted");
  if (!b) return;
  // Checking the files first records renames whose last step a crash cut
  // off, so the counts below are right.
  await api.invoke("history_check", { id: b.id }).catch(() => {});
  await loadHistory();
  b = S.batches.find((x) => x.id === b.id && x.state === "interrupted");
  if (!b) return;
  const left = b.planned - b.renamed;
  const day = when(b.started).replace(/^(Today|Yesterday)/, (w) => w.toLowerCase());
  const choice = await modal({
    title: "DateFix stopped while renaming",
    warn: true,
    body: `<p>A rename ${/^\d/.test(day) ? "on " : ""}${esc(day)} didn't finish, maybe because of a crash or power cut. Every step was saved, so nothing is lost.</p>
      <div class="progress"><span style="width:${Math.round((b.renamed / Math.max(1, b.planned)) * 100)}%"></span></div>
      <div class="facts"><span class="muted">Folders</span><b title="${esc(b.folders.join("\n"))}">${esc(b.folders.join(", "))}</b>
      <span class="muted">Renamed</span><b>${fmt(b.renamed)} of ${fmt(b.planned)}</b></div>`,
    buttons: [
      { label: "Decide later", cls: "subtle", value: null },
      { label: `Undo the ${fmt(b.renamed)}`, value: "undo" },
      // Finishing with nothing left just marks the rename as complete.
      left > 0
        ? { label: `Finish the other ${fmt(left)}`, cls: "primary", value: "finish" }
        : { label: "Keep the new names", cls: "primary", value: "finish" },
    ],
  });
  if (!choice) return;
  try {
    const r = choice === "undo"
      ? await api.invoke("history_undo", { id: b.id, indices: null })
      : await api.invoke("history_resume", { id: b.id, onClash: els.clash.value });
    summaryToast(r, choice === "undo" ? "Put back" : "Renamed");
  } catch (err) {
    toast("bad", "That didn't work", [String(err)]);
  }
  await loadHistory();
  if (S.sources.length) scan();
}

// ================= Theme, preferences, keys, window =================
function store(key, value) {
  try { if (value === undefined) return localStorage.getItem(key); localStorage.setItem(key, value); } catch { /* private mode */ }
  return null;
}

function setTheme(theme) {
  document.documentElement.dataset.theme = theme;
  els.themeLabel.textContent = theme === "dark" ? "Light" : "Dark";
  store("datefix.theme", theme);
}
els.theme.addEventListener("click", () => setTheme(document.documentElement.dataset.theme === "dark" ? "light" : "dark"));

function savePrefs() {
  store("datefix.prefs", JSON.stringify({
    recursive: els.recursive.checked, folders: els.folders.checked, order: els.order.value, clash: els.clash.value,
  }));
}

function loadPrefs() {
  try {
    const p = JSON.parse(store("datefix.prefs") || "{}");
    if (typeof p.recursive === "boolean") els.recursive.checked = p.recursive;
    if (typeof p.folders === "boolean") els.folders.checked = p.folders;
    if (p.order === "day" || p.order === "month") els.order.value = p.order;
    if (p.clash === "number" || p.clash === "skip") els.clash.value = p.clash;
  } catch { /* ignore */ }
}

document.addEventListener("keydown", (e) => {
  if (!els.modal.hidden) return;
  const ctrl = e.ctrlKey || e.metaKey;
  const key = e.key.toLowerCase();
  if (ctrl && key === "o") { e.preventDefault(); chooseFolders(); }
  else if (ctrl && key === "h") { e.preventDefault(); els.pageHistory.hidden ? openHistory() : closeHistory(); }
  else if (ctrl && key === "z" && document.activeElement !== els.filter) { e.preventDefault(); openHistory(els.undoLast.dataset.id); }
  else if (ctrl && key === "f" && !els.pageMain.hidden) { e.preventDefault(); els.filter.focus(); els.filter.select(); }
  else if (key === "f5") { e.preventDefault(); if (S.sources.length) scan(); }
  else if (key === "escape" && !els.pageHistory.hidden) closeHistory();
});

$("win-min").addEventListener("click", () => api.window.minimize());
$("win-max").addEventListener("click", () => api.window.toggleMaximize());
$("win-close").addEventListener("click", () => api.window.close());

api.onDrop((event) => {
  if (event.type === "enter" || event.type === "over") {
    els.overlay.hidden = false;
    els.dropzone.classList.add("over");
  } else {
    els.overlay.hidden = true;
    els.dropzone.classList.remove("over");
    if (event.type === "drop" && event.paths?.length) {
      if (!els.pageHistory.hidden) closeHistory();
      addSources(event.paths);
    }
  }
});

api.listen("scan-progress", (p) => {
  if (!S.busy) return;
  setBusy("Scanning…", { cancel: true, text: `Looked at ${plural(p.checked, "item")} in ${plural(p.folders, "folder")} · ${baseName(p.folder)}` });
});
api.listen("rename-progress", (p) => {
  if (!S.busy) return;
  setBusy("Renaming…", { bar: p.done / Math.max(1, p.total), text: `${fmt(p.done)} of ${fmt(p.total)}` });
});

(async () => {
  setTheme(store("datefix.theme") === "light" ? "light" : "dark");
  loadPrefs();
  api.version().then((v) => { els.version.textContent = v; }).catch(() => {});
  refresh();
  await loadHistory();
  await offerRecovery();
})();

// ================= Demo data for browser previews =================
// Open ui/index.html in a browser: #demo-empty, #history, #recover, #light.
function demoBackend() {
  const hash = location.hash;
  let seed = 11;
  const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  const pick = (a) => a[Math.floor(rnd() * a.length)];
  const pad = (n) => String(n).padStart(2, "0");
  const MONTH = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
  const SRC = [
    ["C:\\Users\\Martin\\Documents\\Archive", [["", 212], ["Receipts", 318], ["Tax 2019", 64]]],
    ["D:\\Scans", [["", 141], ["Letters", 51]]],
    ["E:\\Phone backup", [["Camera", 402], ["WhatsApp", 96]]],
  ];
  // The demo is built before the rest of this file runs, so it can't use MON.
  const SHORT = MONTH.map((m) => m.slice(0, 3));
  const STY = [(d, m, y) => `${pad(d)}.${pad(m)}.${y}`, (d, m, y) => `${pad(d)}-${pad(m)}-${y}`,
    (d, m, y) => `${d} ${MONTH[m - 1]} ${y}`, (d, m, y) => `${SHORT[m - 1]} ${d} ${y}`];
  const PRE = ["scan ", "Letter ", "Bank statement ", "Insurance ", "Receipt Bunnings ", "Invoice #1042 ", "Payment summary "];
  const items = [];
  SRC.forEach(([root, subs], source) => subs.forEach(([sub, n]) => {
    for (let i = 0; i < n; i++) {
      const y = 2016 + Math.floor(rnd() * 9), m = 1 + Math.floor(rnd() * 12), d = 1 + Math.floor(rnd() * 28);
      const phone = source === 2, si = Math.floor(rnd() * 4);
      const oldDate = phone ? `${y}${pad(m)}${pad(d)}` : STY[si](d, m, y);
      const pre = phone ? pick(["IMG_", "PXL_", "VID_"]) : pick(PRE);
      const post = phone ? `_${100000 + Math.floor(rnd() * 800000)}.jpg` : (rnd() < .3 ? ` (${1 + Math.floor(rnd() * 3)})` : "") + pick([".pdf", ".pdf", ".jpg", ".docx"]);
      const iso = `${y}-${pad(m)}-${pad(d)}`;
      const dir = sub ? `${root}\\${sub}` : root;
      items.push({
        from: `${dir}\\${pre}${oldDate}${post}`, to: `${dir}\\${pre}${iso}${post}`,
        oldName: pre + oldDate + post, newName: pre + iso + post, source, location: sub, isDir: false,
        clash: rnd() < .012, dupe: rnd() < .004, longPath: false, ambiguous: !phone && si < 2 && d <= 12 && d !== m,
        problem: rnd() < .006 ? pick(["in_use", "in_use", "no_permission"]) : null, oldDate, iso,
      });
    }
  }));
  const now = Date.now() / 1000;
  const batches = [
    { id: "b1", started: now - 600, finished: now - 590, folders: SRC.map((s) => s[0]), planned: 1247, renamed: 1247, undone: 0, state: "complete" },
    { id: "b2", started: now - 86400 - 3000, finished: now - 86400, folders: ["D:\\Scans"], planned: 342, renamed: 342, undone: 6, state: "partly_undone" },
    { id: "b3", started: now - 3 * 86400, finished: null, folders: ["E:\\Phone backup"], planned: 1180, renamed: 412, undone: 0, state: "interrupted" },
    { id: "b4", started: now - 4 * 86400, finished: now - 4 * 86400, folders: ["C:\\Users\\Martin\\Documents\\Archive\\Receipts"], planned: 318, renamed: 318, undone: 318, state: "undone" },
  ];
  if (!hash.includes("recover")) batches.splice(2, 1);
  const kinds = ["matches", "matches", "matches", "renamed_since", "matches", "contents_changed", "matches", "matches", "missing", "matches", "matches", "matches"];
  const checks = items.slice(0, 400).map((it, index) => {
    const status = kinds[index % kinds.length];
    const nowName = status === "renamed_since" ? it.newName.replace(/(\.\w+)$/, " final$1") : it.newName;
    const folder = it.from.slice(0, it.from.lastIndexOf("\\"));
    return { index, status, isDir: false, nowName, now: `${folder}\\${nowName}`, backName: it.oldName, backTo: it.from, folder };
  });
  const sources = hash.includes("demo-empty") ? [] : hash.includes("single") ? [SRC[0][0]] : SRC.map((s) => s[0]);
  const responses = {
    scan: ({ folders }) => {
      const scanned = folders.map((p) => ({ path: p, name: baseName(p) }));
      const keep = items.filter((it) => folders.includes(SRC[it.source][0]))
        .map((it) => ({ ...it, source: folders.indexOf(SRC[it.source][0]) }));
      return { sources: scanned, items: keep, checked: keep.length * 7 };
    },
    history_list: () => batches,
    history_check: () => checks,
    history_undo: () => ({ renamed: 1236, skipped: 3, failed: 2, problems: ["Insurance 2016-12-28.docx: its contents changed after it was renamed"] }),
    history_resume: () => ({ renamed: 768, skipped: 0, failed: 0, problems: [] }),
    rename: () => ({ renamed: 1243, skipped: 0, failed: 7, problems: [] }),
  };
  if (hash.includes("light")) try { localStorage.setItem("datefix.theme", "light"); } catch { /* ignore */ }
  setTimeout(async () => {
    if (sources.length) addSources(sources);
    if (hash.includes("history")) openHistory();
  }, 30);
  const noop = () => Promise.resolve();
  return {
    invoke: (cmd, args) => new Promise((ok) => setTimeout(() => ok(responses[cmd] ? responses[cmd](args || {}) : null), cmd === "history_check" ? 300 : 60)),
    listen: noop,
    pickFolders: () => Promise.resolve(["C:\\Users\\Martin\\Pictures"]),
    onDrop: noop,
    window: { minimize: noop, toggleMaximize: noop, close: noop },
    version: () => Promise.resolve("2.2.0"),
  };
}
