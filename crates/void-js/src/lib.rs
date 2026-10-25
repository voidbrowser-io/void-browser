//! JS engine: QuickJS + oxc. TSX is transpiled per-module on import —
//! no bundler, no node_modules. `void` and `void/jsx-runtime` are built in.

use void_css::Stylesheet;
use void_dom::{Dom, NodeId};
use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::{CatchResultExt, Context, Ctx, Function, Module, Object, Persistent, Runtime};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

const VOID_JS: &str = include_str!("../runtime/void.js");
const PRELUDE_JS: &str = include_str!("../runtime/prelude.js");

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
}

#[derive(Default, Clone, Copy)]
pub struct Stats {
    pub transpile_ms: f64,
    pub modules: u32,
}

pub struct Engine {
    pub runtime: Runtime,
    pub context: Context,
    pub dom: Rc<RefCell<Dom>>,
    pub shared: Rc<RefCell<Shared>>,
    started: Instant,
}

pub fn transpile(path: &Path, src: &str) -> Result<String, String> {
    use oxc::allocator::Allocator;
    use oxc::codegen::Codegen;
    use oxc::parser::Parser;
    use oxc::semantic::SemanticBuilder;
    use oxc::span::SourceType;
    use oxc::transformer::{JsxRuntime, TransformOptions, Transformer};

    let alloc = Allocator::default();
    let source_type = SourceType::from_path(path).map_err(|e| e.to_string())?;
    let parsed = Parser::new(&alloc, src, source_type).parse();
    if parsed.diagnostics.has_errors() {
        return Err(format_errors(path, parsed.diagnostics.into_vec()));
    }
    let mut program = parsed.program;
    let scoping = SemanticBuilder::new().build(&program).semantic.into_scoping();
    let mut opts = TransformOptions::default();
    opts.jsx.runtime = JsxRuntime::Automatic;
    opts.jsx.import_source = Some("void".into());
    let out = Transformer::new(&alloc, path, &opts).build_with_scoping(scoping, &mut program);
    if out.diagnostics.has_errors() {
        return Err(format_errors(path, out.diagnostics.into_vec()));
    }
    Ok(Codegen::new().build(&program).code)
}

fn format_errors(path: &Path, errors: Vec<oxc::diagnostics::OxcDiagnostic>) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    errors
        .into_iter()
        .map(|e| {
            let labels: Vec<&str> = e.labels.iter().filter_map(|l| l.label()).collect();
            if labels.is_empty() {
                format!("{name}: {}", e.message)
            } else {
                format!("{name}: {} ({})", e.message, labels.join(", "))
            }
        })
        .collect::<Vec<_>>()
        .join("
")
}

struct VoidResolver;

impl Resolver for VoidResolver {
    fn resolve<'js>(&mut self, _ctx: &Ctx<'js>, base: &str, name: &str, _attrs: Option<ImportAttributes<'js>>) -> rquickjs::Result<String> {
        if name == "void" || name == "void/jsx-runtime" || name == "void/jsx-dev-runtime" {
            return Ok("void".into());
        }
        let dir = Path::new(base).parent().unwrap_or(Path::new("."));
        let mut p = normalize(&dir.join(name));
        if p.extension().is_none() {
            for ext in ["tsx", "ts", "jsx", "js"] {
                let cand = p.with_extension(ext);
                if cand.exists() {
                    p = cand;
                    break;
                }
            }
        }
        Ok(p.to_string_lossy().replace('\\', "/"))
    }
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

struct VoidLoader {
    shared: Rc<RefCell<Shared>>,
}

impl Loader for VoidLoader {
    fn load<'js>(&mut self, ctx: &Ctx<'js>, name: &str, _attrs: Option<ImportAttributes<'js>>) -> rquickjs::Result<Module<'js, rquickjs::module::Declared>> {
        if name == "void" {
            return Module::declare(ctx.clone(), "void", VOID_JS);
        }
        let path = PathBuf::from(name);
        let src = std::fs::read_to_string(&path)
            .map_err(|e| rquickjs::Error::new_loading_message(name, format!("{e}")))?;
        if path.extension().is_some_and(|e| e == "css") {
            // `import "./app.css"` — styles go straight into the Rust cascade.
            self.shared.borrow_mut().css.append(&src);
            return Module::declare(ctx.clone(), name, "export default undefined;");
        }
        let t0 = Instant::now();
        let js = transpile(&path, &src).map_err(|e| rquickjs::Error::new_loading_message(name, e))?;
        let mut sh = self.shared.borrow_mut();
        sh.stats.transpile_ms += t0.elapsed().as_secs_f64() * 1000.0;
        sh.stats.modules += 1;
        drop(sh);
        Module::declare(ctx.clone(), name, js)
    }
}

impl Engine {
    pub fn boot(entry: &Path, ua_css: &str) -> Result<Engine, (Engine, String)> {
        let runtime = Runtime::new().expect("quickjs runtime");
        let shared = Rc::new(RefCell::new(Shared { css: Stylesheet::parse(ua_css), ..Default::default() }));
        runtime.set_loader(VoidResolver, VoidLoader { shared: shared.clone() });
        let context = Context::full(&runtime).expect("quickjs context");
        let dom = Rc::new(RefCell::new(Dom::new()));
        let engine = Engine { runtime, context, dom, shared, started: Instant::now() };
        let entry_name = entry.to_string_lossy().replace('\\', "/");
        let result = engine.context.with(|ctx| -> Result<(), String> {
            engine.install_natives(&ctx).map_err(|e| e.to_string())?;
            ctx.eval::<(), _>(PRELUDE_JS).catch(&ctx).map_err(|e| e.to_string())?;
            let src = format!("import {:?};", entry_name);
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

    fn install_natives<'js>(&self, ctx: &Ctx<'js>) -> rquickjs::Result<()> {
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
        let started = self.started;
        o.set("now", Function::new(ctx.clone(), move || started.elapsed().as_secs_f64() * 1000.0)?)?;
        o.set(
            "log",
            Function::new(ctx.clone(), |level: String, msg: String| println!("[js:{level}] {msg}"))?,
        )?;
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
        ctx.globals().set("__dom", o)?;
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

    pub fn dispatch(&self, path: Vec<NodeId>, ty: &str, value: &str, key: &str) -> Result<(), String> {
        let r = self.context.with(|ctx| -> Result<(), String> {
            let f: Function = ctx.globals().get("__void_dispatch").map_err(|e| e.to_string())?;
            f.call::<_, ()>((path, ty, value, key)).catch(&ctx).map_err(|e| e.to_string())
        });
        self.pump();
        r
    }

    /// Fire due timers; returns the next deadline.
    pub fn run_timers(&self) -> (Option<Instant>, Option<String>) {
        let now = Instant::now();
        let due: Vec<(u32, Persistent<Function<'static>>)> = {
            let mut s = self.shared.borrow_mut();
            let mut fired = Vec::new();
            s.timers.retain_mut(|t| {
                if t.due > now {
                    return true;
                }
                fired.push((t.id, t.callback.clone()));
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
        for (_, cb) in due {
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
