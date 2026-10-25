// Globals available to every Void app.
(() => {
  const dom = globalThis.__dom;
  const fmt = (v) => {
    if (typeof v === "string") return v;
    try { return JSON.stringify(v); } catch { return String(v); }
  };
  globalThis.console = {
    log: (...a) => dom.log("log", a.map(fmt).join(" ")),
    info: (...a) => dom.log("info", a.map(fmt).join(" ")),
    warn: (...a) => dom.log("warn", a.map(fmt).join(" ")),
    error: (...a) => dom.log("error", a.map(fmt).join(" ")),
  };
  globalThis.setTimeout = (fn, ms = 0) => dom.setTimer(fn, ms, false);
  globalThis.setInterval = (fn, ms = 0) => dom.setTimer(fn, ms, true);
  globalThis.clearTimeout = globalThis.clearInterval = (id) => dom.clearTimer(id);
  globalThis.requestAnimationFrame = (fn) => dom.setTimer(() => fn(dom.now()), 16, false);
  globalThis.performance = { now: () => dom.now() };
})();
