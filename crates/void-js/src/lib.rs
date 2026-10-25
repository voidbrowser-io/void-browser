//! JS engine: QuickJS + oxc. TSX is transpiled per-module on import —
//! no bundler, no node_modules. Modules are addressed by URL:
//! `file://`, `http(s)://` and built-in `void://` pages.

use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::{CatchResultExt, Context, Ctx, Function, Module, Object, Persistent, Runtime};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};
use url::Url;
use void_css::Stylesheet;
use void_dom::{Dom, NodeId};

const VOID_JS: &str = include_str!("../runtime/void.js");
const PRELUDE_JS: &str = include_str!("../runtime/prelude.js");

/// Built-in `void://` sources, provided by the host app.
pub type BuiltinPages = fn(&str) -> Option<&'static str>;

/// Requests from a page to the browser that hosts it.
#[derive(Debug, Clone)]
pub enum HostCmd {
    Navigate(String),
    Title(String),
    // Privileged (`void://` pages only):
    NewTab(Option<String>),
    CloseTab(usize),
    Activate(usize),
    Back,
    Forward,
    Reload,
}

pub struct Timer {
    pub id: u32,
    pub due: Instant,
    pub every: Option<Duration>,
    pub callback: Persistent<Function<'static>>,
}

#[derive(Default)]
pub struct Shared {
    pub timers: Vec<Timer>,
    pub next_timer: u32,
    pub css: Stylesheet,
    pub stats: Stats,
    pub cmds: Vec<HostCmd>,
    /// Extra JSON the host exposes to privileged pages (`__browser.info()`).
    pub host_info: String,
}

#[derive(Default, Clone, Copy)]
pub struct Stats {
    pub transpile_ms: f64,
    pub fetch_ms: f64,
    pub modules: u32,
}

pub struct Engine {
    pub runtime: Runtime,
    pub context: Context,
    pub dom: Rc<RefCell<Dom>>,
    pub shared: Rc<RefCell<Shared>>,
    /// The page URL (an HTML document or a module).
    pub url: Url,
    /// `<title>` from the HTML document, if any.
    pub html_title: Option<String>,
    started: Instant,
}

// ───────────────────────────── Fetching ─────────────────────────────

pub fn fetch(url: &Url, builtin: BuiltinPages) -> Result<String, String> {
    match url.scheme() {
        "file" => {
            let path = url.to_file_path().map_err(|_| format!("bad file URL: {url}"))?;
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
        }
        "http" | "https" => {
            let mut resp = ureq::get(url.as_str()).call().map_err(|e| format!("{url}: {e}"))?;
            resp.body_mut().read_to_string().map_err(|e| format!("{url}: {e}"))
        }
        "void" => {
            let key = format!("{}{}", url.host_str().unwrap_or_default(), url.path());
            builtin(&key).map(str::to_string).ok_or_else(|| format!("no such page: {url}"))
        }
        other => Err(format!("unsupported scheme: {other}:")),
    }
}

fn is_module(url: &Url) -> bool {
    let p = url.path();
    [".tsx", ".ts", ".jsx", ".js", ".mjs"].iter().any(|e| p.ends_with(e))
}

/// Turn a page URL into its entry module. HTML documents point at their
/// module with `<script type="module" src="…">` — exactly like Vite.
fn resolve_entry(url: &Url, builtin: BuiltinPages) -> Result<(Url, Option<String>), String> {
    let mut url = url.clone();
    if url.scheme() == "void" && (url.path().is_empty() || url.path() == "/") {
        url.set_path("/index.html");
    }
    if url.scheme() == "file" {
        if let Ok(p) = url.to_file_path() {
            if p.is_dir() {
                url = Url::from_file_path(p.join("index.html")).map_err(|_| "bad path".to_string())?;
            }
        }
    }
    if is_module(&url) {
        return Ok((url, None));
    }
    let html = fetch(&url, builtin)?;
    if !html.trim_start().starts_with('<') {
        return Err(format!("{url} is not a Void page: expected HTML or a .tsx/.js module"));
    }
    let title = between(&html, "<title>", "</title>").map(|t| t.trim().to_string());
    let lower = html.to_ascii_lowercase();
    let mut at = 0;
    while let Some(i) = lower[at..].find("<script") {
        let start = at + i;
        let end = lower[start..].find('>').map(|e| start + e).unwrap_or(lower.len());
        let tag = &html[start..end];
        if tag.contains("type=\"module\"") || tag.contains("type='module'") {
            if let Some(src) = attr(tag, "src") {
                let entry = url.join(&src).map_err(|e| e.to_string())?;
                return Ok((entry, title));
            }
        }
        at = end;
    }
    Err(format!(
        "{url} is not a Void page.\n\nVoid only runs modern apps: an HTML page with\n<script type=\"module\" src=\"main.tsx\">, or a .tsx module.\nThe legacy web is not supported — on purpose."
    ))
}

fn between<'a>(s: &'a str, a: &str, b: &str) -> Option<&'a str> {
    let i = s.find(a)? + a.len();
    let j = s[i..].find(b)? + i;
    Some(&s[i..j])
}

fn attr(tag: &str, name: &str) -> Option<String> {
    for q in ['"', '\''] {
        if let Some(v) = between(tag, &format!("{name}={q}"), &q.to_string()) {
            return Some(v.to_string());
        }
    }
    None
}

/// Parse what a user typed into the address bar.
pub fn parse_input(input: &str) -> Result<Url, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("empty address".into());
    }
    if let Ok(u) = Url::parse(s) {
        if matches!(u.scheme(), "http" | "https" | "file" | "void") {
            return Ok(u);
        }
    }
    let p = Path::new(s);
    if p.exists() {
        let abs = std::fs::canonicalize(p).map_err(|e| e.to_string())?;
        let abs = abs.to_string_lossy().trim_start_matches(r"\\?\").to_string();
        return Url::from_file_path(&abs).map_err(|_| format!("bad path: {s}"));
    }
    if !s.contains(' ') && (s.contains('.') || s.starts_with("localhost")) {
        let scheme = if s.starts_with("localhost") || s.starts_with("127.") { "http" } else { "https" };
        return Url::parse(&format!("{scheme}://{s}")).map_err(|e| e.to_string());
    }
    Err(format!("“{s}” is not a URL. Void has no search engine — type an address."))
}

// ───────────────────────────── Transpiling ─────────────────────────────

pub fn transpile(name: &str, src: &str) -> Result<String, String> {
    use oxc::allocator::Allocator;
    use oxc::codegen::Codegen;
    use oxc::parser::Parser;
    use oxc::semantic::SemanticBuilder;
    use oxc::span::SourceType;
    use oxc::transformer::{JsxRuntime, TransformOptions, Transformer};

    let path = Path::new(name.rsplit('/').next().unwrap_or(name));
    let alloc = Allocator::default();
    let source_type = SourceType::from_path(path).unwrap_or_else(|_| SourceType::mjs());
    let parsed = Parser::new(&alloc, src, source_type).parse();
    if parsed.diagnostics.has_errors() {
        return Err(format_errors(name, parsed.diagnostics.into_vec()));
    }
    let mut program = parsed.program;
    let scoping = SemanticBuilder::new().build(&program).semantic.into_scoping();
    let mut opts = TransformOptions::default();
    opts.jsx.runtime = JsxRuntime::Automatic;
    opts.jsx.import_source = Some("void".into());
    let out = Transformer::new(&alloc, path, &opts).build_with_scoping(scoping, &mut program);
    if out.diagnostics.has_errors() {
        return Err(format_errors(name, out.diagnostics.into_vec()));
    }
    Ok(Codegen::new().build(&program).code)
}

fn format_errors(name: &str, errors: Vec<oxc::diagnostics::OxcDiagnostic>) -> String {
    let short = name.rsplit('/').next().unwrap_or(name);
    errors
        .into_iter()
        .map(|e| {
            let labels: Vec<&str> = e.labels.iter().filter_map(|l| l.label()).collect();
            if labels.is_empty() {
                format!("{short}: {}", e.message)
            } else {
                format!("{short}: {} ({})", e.message, labels.join(", "))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ───────────────────────────── Module loading ─────────────────────────────

struct VoidResolver;

impl Resolver for VoidResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attrs: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        if name == "void" || name.starts_with("void/") {
            return Ok("void".into());
        }
        let base = Url::parse(base).map_err(|e| rquickjs::Error::new_resolving_message(base, name, e.to_string()))?;
        let url = base.join(name).map_err(|e| rquickjs::Error::new_resolving_message(base.as_str(), name, e.to_string()))?;
        Ok(url.to_string())
    }
}

struct VoidLoader {
    shared: Rc<RefCell<Shared>>,
    builtin: BuiltinPages,
}

impl Loader for VoidLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attrs: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js, rquickjs::module::Declared>> {
        if name == "void" {
            return Module::declare(ctx.clone(), "void", VOID_JS);
        }
        let url = Url::parse(name).map_err(|e| rquickjs::Error::new_loading_message(name, e.to_string()))?;
        let t0 = Instant::now();
        let src = fetch(&url, self.builtin).map_err(|e| rquickjs::Error::new_loading_message(name, e))?;
        self.shared.borrow_mut().stats.fetch_ms += t0.elapsed().as_secs_f64() * 1000.0;
        if url.path().ends_with(".css") {
            // `import "./app.css"` — styles go straight into the Rust cascade.
            self.shared.borrow_mut().css.append(&src);
            return Module::declare(ctx.clone(), name, "export default undefined;");
        }
        let t0 = Instant::now();
        let js = transpile(name, &src).map_err(|e| rquickjs::Error::new_loading_message(name, e))?;
        let mut sh = self.shared.borrow_mut();
        sh.stats.transpile_ms += t0.elapsed().as_secs_f64() * 1000.0;
        sh.stats.modules += 1;
        drop(sh);
        Module::declare(ctx.clone(), name, js)
    }
}

// ───────────────────────────── Engine ─────────────────────────────

impl Engine {
    /// Boot a page. `privileged` pages (the browser UI, `void://`) get `__browser`.
    pub fn boot(url: &Url, ua_css: &str, builtin: BuiltinPages, privileged: bool, host_info: String) -> Result<Engine, (Engine, String)> {
        let runtime = Runtime::new().expect("quickjs runtime");
        let shared = Rc::new(RefCell::new(Shared { css: Stylesheet::parse(ua_css), host_info, ..Default::default() }));
        runtime.set_loader(VoidResolver, VoidLoader { shared: shared.clone(), builtin });
        let context = Context::full(&runtime).expect("quickjs context");
        let dom = Rc::new(RefCell::new(Dom::new()));
        let mut engine = Engine { runtime, context, dom, shared, url: url.clone(), html_title: None, started: Instant::now() };

        let t0 = Instant::now();
        let entry = match resolve_entry(url, builtin) {
            Ok((entry, title)) => {
                engine.html_title = title;
                entry
            }
            Err(e) => return Err((engine, e)),
        };
        engine.shared.borrow_mut().stats.fetch_ms += t0.elapsed().as_secs_f64() * 1000.0;

        let result = engine.context.with(|ctx| -> Result<(), String> {
            engine.install_natives(&ctx, privileged).map_err(|e| e.to_string())?;
            ctx.eval::<(), _>(PRELUDE_JS).catch(&ctx).map_err(|e| e.to_string())?;
            let src = format!("import {:?};", entry.as_str());
            Module::evaluate(ctx.clone(), "void:main", src)
                .catch(&ctx)
                .map_err(|e| e.to_string())?
                .finish::<()>()
                .catch(&ctx)
                .map_err(|e| e.to_string())
        });
        engine.pump();
        match result {
            Ok(()) => Ok(engine),
            Err(e) => Err((engine, e)),
        }
    }

    fn install_natives<'js>(&self, ctx: &Ctx<'js>, privileged: bool) -> rquickjs::Result<()> {
        let o = Object::new(ctx.clone())?;
        let d = self.dom.clone();
        o.set("createElement", Function::new(ctx.clone(), move |tag: String| d.borrow_mut().create_element(&tag))?)?;
        let d = self.dom.clone();
        o.set("createText", Function::new(ctx.clone(), move |text: String| d.borrow_mut().create_text(&text))?)?;
        let d = self.dom.clone();
        o.set("setText", Function::new(ctx.clone(), move |id: NodeId, text: String| d.borrow_mut().set_text(id, &text))?)?;
        let d = self.dom.clone();
        o.set(
            "insert",
            Function::new(ctx.clone(), move |parent: NodeId, child: NodeId, before: i32| {
                d.borrow_mut().insert(parent, child, (before >= 0).then_some(before as NodeId))
            })?,
        )?;
        let d = self.dom.clone();
        o.set("remove", Function::new(ctx.clone(), move |id: NodeId| d.borrow_mut().remove(id))?)?;
        let d = self.dom.clone();
        o.set("clearRange", Function::new(ctx.clone(), move |a: NodeId, b: NodeId| d.borrow_mut().clear_between(a, b))?)?;
        let d = self.dom.clone();
        o.set(
            "setAttr",
            Function::new(ctx.clone(), move |id: NodeId, k: String, v: String| d.borrow_mut().set_attr(id, &k, &v))?,
        )?;
        let d = self.dom.clone();
        o.set("listen", Function::new(ctx.clone(), move |id: NodeId, ev: String| d.borrow_mut().listen(id, &ev))?)?;
        let d = self.dom.clone();
        o.set(
            "getValue",
            Function::new(ctx.clone(), move |id: NodeId| d.borrow().get(id).map(|n| n.value.clone()).unwrap_or_default())?,
        )?;
        let d = self.dom.clone();
        o.set("focus", Function::new(ctx.clone(), move |id: NodeId| d.borrow_mut().focus_input(id))?)?;
        let started = self.started;
        o.set("now", Function::new(ctx.clone(), move || started.elapsed().as_secs_f64() * 1000.0)?)?;
        o.set("log", Function::new(ctx.clone(), |level: String, msg: String| println!("[js:{level}] {msg}"))?)?;
        let sh = self.shared.clone();
        o.set(
            "setTimer",
            Function::new(ctx.clone(), move |ctx: Ctx<'js>, f: Function<'js>, ms: f64, repeat: bool| {
                let mut s = sh.borrow_mut();
                s.next_timer += 1;
                let id = s.next_timer;
                let d = Duration::from_secs_f64(ms.max(0.0) / 1000.0);
                let callback = Persistent::save(&ctx, f);
                s.timers.push(Timer { id, due: Instant::now() + d, every: repeat.then_some(d.max(Duration::from_millis(1))), callback });
                id
            })?,
        )?;
        let sh = self.shared.clone();
        o.set("clearTimer", Function::new(ctx.clone(), move |id: u32| sh.borrow_mut().timers.retain(|t| t.id != id))?)?;
        let sh = self.shared.clone();
        o.set("setTitle", Function::new(ctx.clone(), move |t: String| sh.borrow_mut().cmds.push(HostCmd::Title(t)))?)?;
        let sh = self.shared.clone();
        let base = self.url.clone();
        o.set(
            "navigate",
            Function::new(ctx.clone(), move |to: String| {
                let target = base.join(&to).map(|u| u.to_string()).unwrap_or(to);
                sh.borrow_mut().cmds.push(HostCmd::Navigate(target));
            })?,
        )?;
        ctx.globals().set("__dom", o)?;

        if privileged {
            let b = Object::new(ctx.clone())?;
            let push = |cmd: fn(Option<String>, usize) -> HostCmd| {
                let sh = self.shared.clone();
                move |s: rquickjs::function::Opt<rquickjs::Value<'js>>| {
                    let v = s.0;
                    let as_str = v.as_ref().and_then(|v| v.as_string()).and_then(|s| s.to_string().ok());
                    let as_num = v.as_ref().and_then(|v| v.as_number()).unwrap_or(0.0) as usize;
                    sh.borrow_mut().cmds.push(cmd(as_str, as_num));
                }
            };
            b.set("newTab", Function::new(ctx.clone(), push(|s, _| HostCmd::NewTab(s)))?)?;
            b.set("closeTab", Function::new(ctx.clone(), push(|_, n| HostCmd::CloseTab(n)))?)?;
            b.set("activate", Function::new(ctx.clone(), push(|_, n| HostCmd::Activate(n)))?)?;
            b.set("navigate", Function::new(ctx.clone(), push(|s, _| HostCmd::Navigate(s.unwrap_or_default())))?)?;
            b.set("back", Function::new(ctx.clone(), push(|_, _| HostCmd::Back))?)?;
            b.set("forward", Function::new(ctx.clone(), push(|_, _| HostCmd::Forward))?)?;
            b.set("reload", Function::new(ctx.clone(), push(|_, _| HostCmd::Reload))?)?;
            let sh = self.shared.clone();
            b.set("info", Function::new(ctx.clone(), move || sh.borrow().host_info.clone())?)?;
            ctx.globals().set("__browser", b)?;
        }
        Ok(())
    }

    /// Drain promise jobs (microtasks).
    pub fn pump(&self) {
        while self.runtime.is_job_pending() {
            if self.runtime.execute_pending_job().is_err() {
                break;
            }
        }
    }

    /// Dispatch an event along `path`. Returns `true` unless a handler called `preventDefault()`.
    pub fn dispatch(&self, path: Vec<NodeId>, ty: &str, value: &str, key: &str) -> Result<bool, String> {
        let r = self.context.with(|ctx| -> Result<bool, String> {
            let f: Function = ctx.globals().get("__void_dispatch").map_err(|e| e.to_string())?;
            f.call::<_, bool>((path, ty, value, key)).catch(&ctx).map_err(|e| e.to_string())
        });
        self.pump();
        r
    }

    /// Call a global JS function with one string argument, if it exists.
    pub fn call_global(&self, name: &str, arg: &str) -> Result<(), String> {
        let r = self.context.with(|ctx| -> Result<(), String> {
            let Ok(f) = ctx.globals().get::<_, Function>(name) else { return Ok(()) };
            f.call::<_, ()>((arg,)).catch(&ctx).map_err(|e| e.to_string())
        });
        self.pump();
        r
    }

    pub fn take_cmds(&self) -> Vec<HostCmd> {
        std::mem::take(&mut self.shared.borrow_mut().cmds)
    }

    /// Fire due timers; returns the next deadline.
    pub fn run_timers(&self) -> (Option<Instant>, Option<String>) {
        let now = Instant::now();
        let due: Vec<Persistent<Function<'static>>> = {
            let mut s = self.shared.borrow_mut();
            let mut fired = Vec::new();
            s.timers.retain_mut(|t| {
                if t.due > now {
                    return true;
                }
                fired.push(t.callback.clone());
                match t.every {
                    Some(every) => {
                        t.due = now + every;
                        true
                    }
                    None => false,
                }
            });
            fired
        };
        let mut err = None;
        for cb in due {
            let r = self.context.with(|ctx| -> Result<(), String> {
                let f = cb.restore(&ctx).map_err(|e| e.to_string())?;
                f.call::<_, ()>(()).catch(&ctx).map_err(|e| e.to_string())
            });
            if let Err(e) = r {
                err = Some(e);
            }
        }
        self.pump();
        (self.shared.borrow().timers.iter().map(|t| t.due).min(), err)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // Persistent handles must die before the runtime.
        self.shared.borrow_mut().timers.clear();
    }
}
