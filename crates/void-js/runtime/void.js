// Void runtime: fine-grained signals + direct DOM binding. No virtual DOM,
// no diffing — a signal write updates exactly the nodes that read it.

const dom = globalThis.__dom;

// ───────────── Reactivity ─────────────

let Listener = null; // computation currently tracking reads
let Owner = null; // computation that owns newly created effects
let batchDepth = 0;
const queue = new Set();

class Computation {
  constructor(fn) {
    this.fn = fn;
    this.sources = new Set();
    this.children = [];
    this.cleanups = [];
    this.disposed = false;
  }
  run() {
    if (this.disposed) return;
    this.clean();
    const pl = Listener, po = Owner;
    Listener = Owner = this;
    try { this.fn(); } finally { Listener = pl; Owner = po; }
  }
  clean() {
    for (const s of this.sources) s.delete(this);
    this.sources.clear();
    for (const c of this.children) c.dispose();
    this.children = [];
    for (const f of this.cleanups) f();
    this.cleanups = [];
  }
  dispose() { this.clean(); this.disposed = true; }
}

function schedule(c) {
  if (batchDepth > 0) queue.add(c);
  else c.run();
}

/** A reactive value: `count()` reads, `count.set(v)` / `count.update(f)` writes. */
export function signal(initial) {
  let value = initial;
  const subs = new Set();
  const read = () => {
    if (Listener) { subs.add(Listener); Listener.sources.add(subs); }
    return value;
  };
  read.set = (next) => {
    if (Object.is(next, value)) return;
    value = next;
    batch(() => { for (const c of [...subs]) schedule(c); });
  };
  read.update = (fn) => read.set(fn(value));
  read.peek = () => value;
  return read;
}

export function effect(fn) {
  const c = new Computation(fn);
  if (Owner) Owner.children.push(c);
  c.run();
  return () => c.dispose();
}

export function computed(fn) {
  const s = signal(undefined);
  effect(() => s.set(fn()));
  return () => s();
}

export function batch(fn) {
  batchDepth++;
  try { return fn(); } finally {
    if (--batchDepth === 0) {
      while (queue.size) {
        const list = [...queue];
        queue.clear();
        for (const c of list) c.run();
      }
    }
  }
}

export function untrack(fn) {
  const pl = Listener;
  Listener = null;
  try { return fn(); } finally { Listener = pl; }
}

export function root(fn) {
  const r = new Computation(null);
  const pl = Listener, po = Owner;
  Listener = null; Owner = r;
  try { return fn(() => r.dispose()); } finally { Listener = pl; Owner = po; }
}

export function onCleanup(fn) {
  if (Owner) Owner.cleanups.push(fn);
}

// ───────────── DOM binding ─────────────

export class VNode {
  constructor(id) { this.id = id; }
}

const handlers = new Map();

function camelToKebab(k) {
  return k.replace(/[A-Z]/g, (m) => "-" + m.toLowerCase());
}

function applyProp(id, key, v) {
  if (key === "className") key = "class";
  if (key === "style" && v && typeof v === "object") {
    v = Object.entries(v).map(([k, x]) => `${k.startsWith("--") ? k : camelToKebab(k)}:${x}`).join(";");
  }
  if (key === "class" && v && typeof v === "object" && !Array.isArray(v)) {
    v = Object.keys(v).filter((k) => (typeof v[k] === "function" ? v[k]() : v[k])).join(" ");
  }
  if (Array.isArray(v)) v = v.filter(Boolean).join(" ");
  dom.setAttr(id, key, v == null || v === false ? "" : String(v));
}

function setProp(id, key, value) {
  if (key === "children" || key === "ref") return;
  if (key.startsWith("on") && typeof value === "function") {
    const type = key.slice(2).toLowerCase();
    let h = handlers.get(id);
    if (!h) handlers.set(id, (h = {}));
    h[type] = value;
    dom.listen(id, type);
    return;
  }
  const dynamic = typeof value === "function" ||
    (key === "class" && value && typeof value === "object" && Object.values(value).some((x) => typeof x === "function"));
  if (dynamic) effect(() => applyProp(id, key, typeof value === "function" ? value() : value));
  else applyProp(id, key, value);
}

/** Insert any JSX child value into `parent` before `before` (-1 = append). */
function insert(parent, value, before) {
  if (value == null || value === false || value === true) return;
  if (Array.isArray(value)) { for (const v of value) insert(parent, v, before); return; }
  if (value instanceof VNode) { dom.insert(parent, value.id, before); return; }
  if (typeof value === "function") { region(parent, value, before); return; }
  dom.insert(parent, dom.createText(String(value)), before);
}

/** A reactive region between two anchors, re-rendered when its signals change. */
function region(parent, fn, before) {
  const start = dom.createText("");
  const end = dom.createText("");
  dom.insert(parent, start, before);
  dom.insert(parent, end, before);
  let textNode = -1;
  effect(() => {
    const v = fn();
    const primitive = v == null || typeof v !== "object" && typeof v !== "function";
    if (primitive && textNode >= 0) {
      dom.setText(textNode, v == null || v === false || v === true ? "" : String(v));
      return;
    }
    dom.clearRange(start, end);
    textNode = -1;
    if (primitive) {
      if (v == null || v === false || v === true) return;
      textNode = dom.createText(String(v));
      dom.insert(parent, textNode, end);
    } else {
      insert(parent, v, end);
    }
  });
}

export function jsx(type, props) {
  props = props || {};
  if (typeof type === "function") return untrack(() => type(props));
  const id = dom.createElement(type);
  const node = new VNode(id);
  for (const k in props) setProp(id, k, props[k]);
  if ("children" in props) insert(id, props.children, -1);
  if (typeof props.ref === "function") props.ref(node);
  return node;
}

export const jsxs = jsx;
export const jsxDEV = jsx;

export function Fragment(props) {
  return props.children;
}

// ───────────── Control flow ─────────────

/** Keyed list: each item renders once; reorders move nodes, never re-render. */
export function For(props) {
  const render = Array.isArray(props.children) ? props.children[0] : props.children;
  let cache = new Map();
  return () => {
    const items = props.each() || [];
    const next = new Map();
    const out = [];
    items.forEach((item, i) => {
      let entry = cache.get(item);
      if (entry) cache.delete(item);
      else entry = root((dispose) => ({ node: untrack(() => render(item, i)), dispose }));
      next.set(item, entry);
      out.push(entry.node);
    });
    for (const e of cache.values()) e.dispose();
    cache = next;
    return out;
  };
}

export function Show(props) {
  return () => (props.when() ? props.children : props.fallback ?? null);
}

// ───────────── App entry ─────────────

export function render(App) {
  root(() => insert(0, typeof App === "function" ? jsx(App, {}) : App, -1));
}

globalThis.__void_dispatch = (path, type, value, key) => {
  const ev = {
    type, key, value,
    target: { id: path[0], value },
    stopped: false,
    stopPropagation() { this.stopped = true; },
  };
  batch(() => {
    for (const id of path) {
      const h = handlers.get(id)?.[type];
      if (h) { ev.currentTarget = id; h(ev); if (ev.stopped) break; }
    }
  });
};

export const now = () => dom.now();
