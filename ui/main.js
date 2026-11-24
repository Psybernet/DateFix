"use strict";

// ---------- Backend ----------
// Inside the app this talks to the Rust side. Opened in a normal browser
// (for design work or screenshots) it uses made-up demo data instead.
const tauri = window.__TAURI__;
const backend = tauri ? {
  invoke: (cmd, args) => tauri.core.invoke(cmd, args),
  pickFolder: () => tauri.dialog.open({ directory: true, multiple: false, title: "Choose a folder to tidy" }),
  onDrop: (handler) => tauri.webview.getCurrentWebview().onDragDropEvent((e) => handler(e.payload)),
  window: tauri.window.getCurrentWindow(),
} : demoBackend();

// ---------- State ----------
const state = {
  folder: null,
  items: [],          // from the scan, each with a `selected` flag
  onClash: "number",
  filter: "",
  busy: false,
};

const $ = (id) => document.getElementById(id);
const els = {
  dropzone: $("dropzone"), chip: $("folder-chip"), path: $("folder-path"), rescan: $("rescan"),
  recursive: $("opt-recursive"), monthFirst: $("opt-month-first"),
  summary: $("summary"), filter: $("filter"), selectAll: $("select-all"),
  list: $("list"), empty: $("empty"), apply: $("apply"), applyLabel: $("apply-label"),
  undo: $("undo"), undoLabel: $("undo-label"), overlay: $("drop-overlay"), toasts: $("toasts"),
};

// ---------- Helpers ----------
function el(tag, props = {}, children = []) {
  const node = document.createElement(tag);
  Object.assign(node, props);
  for (const child of [].concat(children)) {
    if (child != null) node.append(child);
  }
  return node;
}

function svg(markup, cls) {
  const wrap = document.createElement("span");
  wrap.innerHTML = markup;
  const node = wrap.firstElementChild;
  if (cls) node.setAttribute("class", cls);
  return node;
}

const ICON_FILE = '<svg viewBox="0 0 24 24"><path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/></svg>';
const ICON_DIR = '<svg viewBox="0 0 24 24"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/></svg>';

/** Splits a name around the first occurrence of `part`, wrapping it in <mark>. */
function highlighted(name, part) {
  const at = part ? name.indexOf(part) : -1;
  if (at < 0) return [name];
  return [name.slice(0, at), el("mark", { textContent: part }), name.slice(at + part.length)];
}

function plural(n, word) {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}

function timeAgo(seconds) {
  const diffS = Math.max(0, Date.now() / 1000 - seconds);
  if (diffS < 60) return "just now";
  if (diffS < 3600) return `${Math.round(diffS / 60)} min ago`;
  if (diffS < 86400) return `${Math.round(diffS / 3600)} h ago`;
  return new Date(seconds * 1000).toLocaleDateString();
}

function toast(kind, title, lines = []) {
  const node = el("div", { className: `toast ${kind}` }, [
    el("strong", { textContent: title }),
    lines.length ? el("ul", {}, lines.slice(0, 5).map((l) => el("li", { textContent: l }))) : null,
  ]);
  els.toasts.append(node);
  setTimeout(() => {
    node.classList.add("leaving");
    setTimeout(() => node.remove(), 300);
  }, lines.length ? 7000 : 3500);
}

// ---------- Rendering ----------
function visibleItems() {
  const q = state.filter.toLowerCase();
  return q ? state.items.filter((i) => (i.oldName + i.newName + i.location).toLowerCase().includes(q)) : state.items;
}

function render() {
  const items = visibleItems();
  const selected = state.items.filter((i) => i.selected).length;

  els.list.replaceChildren();
  if (!state.items.length) {
    els.list.append(els.empty);
  } else if (!items.length) {
    els.list.append(el("div", { className: "empty" }, el("p", { textContent: "No names match the filter." })));
  }

  items.forEach((item, index) => {
    const tags = [];
    if (item.location) tags.push(el("span", { className: "tag", textContent: `in ${item.location}` }));
    if (item.clash) {
      tags.push(el("span", {
        className: "tag warn",
        textContent: state.onClash === "number" ? "Name taken · a number will be added" : "Name taken · will be skipped",
      }));
    }
    const box = el("input", { type: "checkbox", checked: item.selected, ariaLabel: `Rename ${item.oldName}` });
    box.addEventListener("change", () => { item.selected = box.checked; render(); });
    const row = el("label", { className: `row${item.selected ? "" : " off"}` }, [
      el("span", { className: "check" }, [box, el("span", { className: "box" })]),
      svg(item.isDir ? ICON_DIR : ICON_FILE, `kind${item.isDir ? " dir" : ""}`),
      el("div", { className: "names" }, [
        el("span", { className: "old", title: item.oldName }, highlighted(item.oldName, item.oldDate)),
        el("span", { className: "new", title: item.newName }, [
          el("span", { className: "arrow", textContent: "→" }),
          el("span", { className: "text" }, highlighted(item.newName, item.newDate)),
        ]),
        tags.length ? el("div", { className: "meta" }, tags) : null,
      ]),
    ]);
    row.setAttribute("role", "listitem");
    row.style.animationDelay = `${Math.min(index, 20) * 18}ms`;
    els.list.append(row);
  });

  // Header, select-all and buttons
  if (state.folder) {
    els.summary.textContent = state.items.length
      ? `${plural(state.items.length, "name")} to fix · ${selected} selected`
      : "Every date in this folder is already YYYY-MM-DD. 🎉";
  }
  els.filter.disabled = els.selectAll.disabled = !state.items.length;
  els.selectAll.checked = selected > 0 && selected === state.items.length;
  els.selectAll.indeterminate = selected > 0 && selected < state.items.length;
  els.apply.disabled = state.busy || selected === 0;
  els.applyLabel.textContent = selected ? `Rename ${plural(selected, "item")}` : "Rename";
}

function showFolder(folder) {
  els.chip.hidden = !folder;
  els.path.textContent = folder || "";
  els.path.title = folder || "";
}

// ---------- Actions ----------
async function scan(folder = state.folder) {
  if (!folder) return;
  try {
    const result = await backend.invoke("scan", {
      folder,
      recursive: els.recursive.checked,
      monthFirst: els.monthFirst.checked,
    });
    state.folder = result.folder;
    state.items = result.items.map((i) => ({ ...i, selected: true }));
    state.filter = els.filter.value = "";
    showFolder(state.folder);
    render();
  } catch (err) {
    toast("bad", "Couldn't scan that folder", [String(err)]);
  }
}

async function browse() {
  const folder = await backend.pickFolder();
  if (folder) scan(folder);
}

async function applyChanges() {
  const changes = state.items.filter((i) => i.selected).map(({ from, to, clash }) => ({ from, to, clash }));
  if (!changes.length) return;
  state.busy = true;
  els.apply.classList.add("busy");
  render();
  try {
    const result = await backend.invoke("rename", { folder: state.folder, changes, onClash: state.onClash });
    const extra = [];
    if (result.skipped) extra.push(`${result.skipped} skipped`);
    if (result.failed) extra.push(`${result.failed} failed`);
    toast(result.failed ? "bad" : "good",
      `Renamed ${plural(result.renamed, "item")}${extra.length ? ` (${extra.join(", ")})` : ""}`,
      result.problems);
  } catch (err) {
    toast("bad", "Renaming didn't work", [String(err)]);
  } finally {
    state.busy = false;
    els.apply.classList.remove("busy");
    await refreshUndo();
    await scan();
  }
}

async function undo() {
  try {
    const result = await backend.invoke("undo");
    toast(result.failed ? "bad" : "good", `Put back ${plural(result.renamed, "name")}`, result.problems);
  } catch (err) {
    toast("bad", "Couldn't undo", [String(err)]);
  }
  await refreshUndo();
  await scan();
}

async function refreshUndo() {
  const last = await backend.invoke("last_rename").catch(() => null);
  els.undo.disabled = !last;
  els.undoLabel.textContent = last
    ? `Undo last rename · ${plural(last.count, "item")}, ${timeAgo(last.when)}`
    : "Undo last rename";
}

// ---------- Wiring ----------
els.dropzone.addEventListener("click", browse);
els.rescan.addEventListener("click", () => scan());
els.recursive.addEventListener("change", () => scan());
els.monthFirst.addEventListener("change", () => scan());
els.filter.addEventListener("input", () => { state.filter = els.filter.value; render(); });
els.selectAll.addEventListener("change", () => {
  const on = els.selectAll.checked;
  visibleItems().forEach((i) => { i.selected = on; });
  render();
});
els.apply.addEventListener("click", applyChanges);
els.undo.addEventListener("click", undo);

document.querySelectorAll(".segmented button").forEach((button) => {
  button.addEventListener("click", () => {
    state.onClash = button.dataset.clash;
    document.querySelectorAll(".segmented button").forEach((b) => b.setAttribute("aria-checked", String(b === button)));
    render();
  });
});

document.addEventListener("keydown", (e) => {
  if (!(e.ctrlKey || e.metaKey)) return;
  if (e.key.toLowerCase() === "o") { e.preventDefault(); browse(); }
  if (e.key.toLowerCase() === "z" && !els.undo.disabled && document.activeElement !== els.filter) { e.preventDefault(); undo(); }
});

$("win-min").addEventListener("click", () => backend.window.minimize());
$("win-max").addEventListener("click", () => backend.window.toggleMaximize());
$("win-close").addEventListener("click", () => backend.window.close());

backend.onDrop((event) => {
  if (event.type === "enter" || event.type === "over") {
    els.overlay.hidden = false;
    els.dropzone.classList.add("over");
  } else {
    els.overlay.hidden = true;
    els.dropzone.classList.remove("over");
    if (event.type === "drop" && event.paths && event.paths.length) scan(event.paths[0]);
  }
});

(async () => {
  const glass = await backend.invoke("glass").catch(() => "none");
  if (glass !== "none") document.body.classList.add("native-glass");
  await refreshUndo();
  render();
})();

// ---------- Demo data for browser previews ----------
function demoBackend() {
  const folder = "C:\\Users\\Martin\\Documents\\Scans";
  const sample = [
    ["11 November 2022 notes.txt", "11 November 2022", "2022-11-11"],
    ["Report Nov 22 2022.pdf", "Nov 22 2022", "2022-11-22"],
    ["scan 05.11.2023.jpg", "05.11.2023", "2023-11-05", true],
    ["IMG_20231105_123456.jpg", "20231105", "2023-11-05"],
    ["Invoice June 5 2023.pdf", "June 5 2023", "2023-06-05"],
    ["3rd July 2021 trip", "3rd July 2021", "2021-07-03", false, true],
    ["Meeting 11Nov23.docx", "11Nov23", "2023-11-11"],
    ["Budget Sept 3 2021.xlsx", "Sept 3 2021", "2021-09-03"],
  ];
  const items = sample.map(([oldName, oldDate, newDate, clash = false, isDir = false]) => {
    const newName = oldName.replace(oldDate, newDate);
    return {
      from: `${folder}\\${oldName}`, to: `${folder}\\${newName}`,
      oldName, newName, oldDate, newDate, location: "", isDir, clash,
    };
  });
  const noop = () => Promise.resolve();
  const responses = {
    glass: "none",
    scan: { folder, items },
    last_rename: { count: 12, folder, when: Date.now() / 1000 - 300 },
    rename: { renamed: 7, skipped: 0, failed: 0, problems: [] },
    undo: { renamed: 12, skipped: 0, failed: 0, problems: [] },
  };
  setTimeout(() => scan(folder), 50);
  return {
    invoke: (cmd) => Promise.resolve(responses[cmd]),
    pickFolder: () => Promise.resolve(folder),
    onDrop: noop,
    window: { minimize: noop, toggleMaximize: noop, close: noop },
  };
}
