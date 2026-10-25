//! Void Browser — a modern-only web runtime for Windows.
//!
//! usage: void [path/to/main.tsx] [--auto "click 120 80; wait 200; type hi; key Enter; shot out.png; quit"]

mod capture;

use notify::{RecursiveMode, Watcher};
use parley::{FontContext, LayoutContext};
use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use vello::kurbo::{Affine, Point};
use vello::peniko::Color;
use vello::util::{RenderContext, RenderSurface};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};
use void_css::{cascade, uses_hover, UA_CSS};
use void_dom::{NodeId, ROOT};
use void_js::Engine;
use void_layout::{Frame, TextBrush};
use void_paint::{error_overlay, HitBox, Painter};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorIcon, Window, WindowId};

const CARET_BLINK: Duration = Duration::from_millis(530);

#[derive(Debug)]
enum UserEvent {
    FileChanged,
}

/// One step of an `--auto` script. Coordinates are CSS pixels.
#[derive(Debug, Clone)]
enum Step {
    Click(f64, f64),
    Move(f64, f64),
    Type(String),
    Key(String),
    Wait(u64),
    Shot(PathBuf),
    Quit,
}

fn parse_script(src: &str) -> VecDeque<Step> {
    src.split(';')
        .filter_map(|cmd| {
            let cmd = cmd.trim();
            let (op, rest) = cmd.split_once(' ').unwrap_or((cmd, ""));
            let nums: Vec<f64> = rest.split_whitespace().filter_map(|n| n.parse().ok()).collect();
            Some(match op {
                "click" => Step::Click(*nums.first()?, *nums.get(1)?),
                "move" => Step::Move(*nums.first()?, *nums.get(1)?),
                "type" => Step::Type(rest.to_string()),
                "key" => Step::Key(rest.trim().to_string()),
                "wait" => Step::Wait(*nums.first()? as u64),
                "shot" => Step::Shot(PathBuf::from(rest.trim())),
                "quit" => Step::Quit,
                "" => return None,
                _ => {
                    eprintln!("[void] unknown --auto step: {cmd}");
                    return None;
                }
            })
        })
        .collect()
}

struct App {
    entry: PathBuf,
    script: VecDeque<Step>,
    script_at: Instant,
    _watcher: Option<notify::RecommendedWatcher>,

    window: Option<Arc<Window>>,
    render_cx: RenderContext,
    surface: Option<RenderSurface<'static>>,
    renderer: Option<Renderer>,
    scene: Scene,
    fcx: FontContext,
    lcx: LayoutContext<TextBrush>,

    engine: Option<Engine>,
    error: Option<String>,
    frame: Option<Frame>,
    hits: Vec<HitBox>,
    hover_sensitive: bool,
    cursor: Point,
    pressed: Option<NodeId>,
    caret_on: bool,
    caret_at: Instant,
    reload_at: Option<Instant>,

    boot_ms: f64,
    layout_ms: f64,
    paint_ms: f64,
}

impl App {
    fn reload(&mut self) {
        let t0 = Instant::now();
        // Drop the old world first: timers hold handles into the old runtime.
        self.frame = None;
        self.engine = None;
        self.hits.clear();
        let (engine, error) = match Engine::boot(&self.entry, UA_CSS) {
            Ok(e) => (e, None),
            Err((e, msg)) => (e, Some(msg)),
        };
        if let Some(err) = &error {
            eprintln!("[void] {err}");
        }
        self.hover_sensitive = uses_hover(&engine.shared.borrow().css);
        let stats = engine.shared.borrow().stats;
        self.engine = Some(engine);
        self.error = error;
        self.boot_ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!(
            "[void] booted {} in {:.1} ms ({} modules, transpile {:.2} ms)",
            self.entry.display(),
            self.boot_ms,
            stats.modules,
            stats.transpile_ms
        );
        self.request_redraw();
    }

    fn request_redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn mark_if_dirty(&self) {
        if self.engine.as_ref().is_some_and(|e| e.dom.borrow().dirty) {
            self.request_redraw();
        }
    }

    fn logical_size(&self) -> (f32, f32, f64) {
        let w = self.window.as_ref().unwrap();
        let scale = w.scale_factor();
        let size = w.inner_size();
        ((size.width as f64 / scale) as f32, (size.height as f64 / scale) as f32, scale)
    }

    fn relayout(&mut self) {
        let Some(engine) = &self.engine else { return };
        let (w, h, _) = self.logical_size();
        let t0 = Instant::now();
        let mut dom = engine.dom.borrow_mut();
        let styles = cascade(&dom, &engine.shared.borrow().css);
        let frame = void_layout::build(&dom, styles, (w, h), &mut self.fcx, &mut self.lcx);
        dom.dirty = false;
        drop(dom);
        self.frame = Some(frame);
        self.layout_ms = t0.elapsed().as_secs_f64() * 1000.0;
    }

    /// Restyle + layout if needed, then record the scene and hit boxes.
    fn paint_scene(&mut self) {
        let Some(engine) = &self.engine else { return };
        if self.frame.is_none() || engine.dom.borrow().dirty {
            self.relayout();
        }
        let (w, h, scale) = self.logical_size();
        let transform = Affine::scale(scale);
        let t0 = Instant::now();
        self.scene.reset();
        let engine = self.engine.as_ref().unwrap();
        let dom = engine.dom.borrow();
        let mut painter = Painter {
            scene: &mut self.scene,
            fcx: &mut self.fcx,
            lcx: &mut self.lcx,
            dom: &dom,
            transform,
            hits: Vec::new(),
            caret_on: self.caret_on,
        };
        if let Some(frame) = &self.frame {
            painter.paint(frame);
        }
        self.hits = std::mem::take(&mut painter.hits);
        drop(dom);
        if let Some(err) = &self.error {
            error_overlay(&mut self.scene, &mut self.fcx, &mut self.lcx, transform, (w, h), err);
        }
        self.paint_ms = t0.elapsed().as_secs_f64() * 1000.0;
    }

    fn redraw(&mut self) {
        if self.surface.is_none() || self.engine.is_none() {
            return;
        }
        self.paint_scene();
        let surface = self.surface.as_ref().unwrap();
        let dev = &self.render_cx.devices[surface.dev_id];
        let (width, height) = (surface.config.width, surface.config.height);
        let texture = match surface.surface.get_current_texture() {
            vello::wgpu::CurrentSurfaceTexture::Success(t) | vello::wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            _ => {
                self.request_redraw();
                return;
            }
        };
        self.renderer
            .as_mut()
            .unwrap()
            .render_to_texture(
                &dev.device,
                &dev.queue,
                &self.scene,
                &surface.target_view,
                &RenderParams { base_color: Color::WHITE, width, height, antialiasing_method: AaConfig::Area },
            )
            .expect("render");
        let mut encoder = dev.device.create_command_encoder(&Default::default());
        let view = texture.texture.create_view(&Default::default());
        surface.blitter.copy(&dev.device, &mut encoder, &surface.target_view, &view);
        dev.queue.submit([encoder.finish()]);
        dev.queue.present(texture);

        if let Some(win) = &self.window {
            let nodes = self.engine.as_ref().map(|e| e.dom.borrow().nodes.len()).unwrap_or(0);
            win.set_title(&format!(
                "Void — {}  ·  boot {:.0} ms · layout {:.2} ms · paint {:.2} ms · {} nodes",
                self.entry.parent().and_then(|p| p.parent()).and_then(|p| p.file_name()).map(|n| n.to_string_lossy()).unwrap_or_default(),
                self.boot_ms,
                self.layout_ms,
                self.paint_ms,
                nodes
            ));
        }
    }

    fn hit_test(&self, p: Point) -> Option<NodeId> {
        self.hits.iter().rev().find(|h| h.rect.contains(p)).map(|h| h.id)
    }

    fn dispatch(&mut self, target: NodeId, ty: &str, value: &str, key: &str) {
        let Some(engine) = &self.engine else { return };
        let path = engine.dom.borrow().path(target);
        if let Err(e) = engine.dispatch(path, ty, value, key) {
            eprintln!("[void] {e}");
            self.error = Some(e);
            self.request_redraw();
        }
        self.mark_if_dirty();
    }

    fn move_to(&mut self, p: Point) {
        self.cursor = p;
        let hit = self.hit_test(p);
        let Some(engine) = &self.engine else { return };
        let mut dom = engine.dom.borrow_mut();
        if dom.hover != hit {
            dom.hover = hit;
            if self.hover_sensitive {
                dom.dirty = true;
            }
        }
        let clickable = hit.is_some_and(|h| {
            dom.path(h).iter().any(|&n| dom.nodes[n as usize].listeners.contains("click"))
                || dom.nodes[h as usize].tag() == Some("input")
        });
        drop(dom);
        if let Some(w) = &self.window {
            w.set_cursor(if clickable { CursorIcon::Pointer } else { CursorIcon::Default });
        }
        self.mark_if_dirty();
    }

    fn mouse(&mut self, state: ElementState) {
        let hit = self.hit_test(self.cursor);
        let Some(engine) = &self.engine else { return };
        match state {
            ElementState::Pressed => {
                let mut dom = engine.dom.borrow_mut();
                dom.active = hit;
                dom.focus = hit.filter(|&h| dom.nodes[h as usize].tag() == Some("input"));
                dom.dirty = true;
                self.pressed = hit;
                self.caret_on = true;
                self.caret_at = Instant::now();
            }
            ElementState::Released => {
                {
                    let mut dom = engine.dom.borrow_mut();
                    dom.active = None;
                    dom.dirty = true;
                }
                if let (Some(down), Some(up)) = (self.pressed.take(), hit) {
                    let common = {
                        let dom = engine.dom.borrow();
                        dom.path(up).into_iter().find(|&n| dom.is_ancestor(n, down))
                    };
                    if let Some(target) = common {
                        self.dispatch(target, "click", "", "");
                    }
                }
            }
        }
        self.request_redraw();
    }

    fn on_key(&mut self, key: &Key, text: Option<&str>) {
        let Some(engine) = &self.engine else { return };
        let focus = engine.dom.borrow().focus;
        let key_name = match key {
            Key::Named(n) => format!("{n:?}"),
            Key::Character(c) => c.to_string(),
            _ => String::new(),
        };
        if let Some(id) = focus {
            let mut changed = false;
            {
                let mut dom = engine.dom.borrow_mut();
                let node = &mut dom.nodes[id as usize];
                match key {
                    Key::Named(NamedKey::Backspace) => changed = node.value.pop().is_some(),
                    Key::Named(NamedKey::Escape) => dom.focus = None,
                    Key::Named(NamedKey::Enter) => {}
                    _ => {
                        if let Some(t) = text.filter(|t| t.chars().all(|c| !c.is_control())) {
                            node.value.push_str(t);
                            changed = true;
                        }
                    }
                }
                dom.dirty = true;
            }
            self.caret_on = true;
            self.caret_at = Instant::now();
            let value = engine.dom.borrow().nodes[id as usize].value.clone();
            if changed {
                self.dispatch(id, "input", &value, &key_name);
            }
            self.dispatch(id, "keydown", &value, &key_name);
        } else {
            self.dispatch(ROOT, "keydown", "", &key_name);
        }
        self.request_redraw();
    }

    /// Run due `--auto` steps. Waits until the first frame exists.
    fn run_script(&mut self, event_loop: &ActiveEventLoop) -> Option<Instant> {
        if self.script.is_empty() || self.frame.is_none() {
            return None;
        }
        while Instant::now() >= self.script_at {
            let Some(step) = self.script.pop_front() else { return None };
            // Hit boxes must reflect the latest DOM before synthetic input.
            if self.engine.as_ref().is_some_and(|e| e.dom.borrow().dirty) {
                self.paint_scene();
            }
            match step {
                Step::Move(x, y) => self.move_to(Point::new(x, y)),
                Step::Click(x, y) => {
                    self.move_to(Point::new(x, y));
                    self.mouse(ElementState::Pressed);
                    self.mouse(ElementState::Released);
                }
                Step::Type(text) => {
                    for ch in text.chars() {
                        let s = ch.to_string();
                        self.on_key(&Key::Character(s.as_str().into()), Some(&s));
                    }
                }
                Step::Key(name) => {
                    let key = match name.as_str() {
                        "Enter" => Key::Named(NamedKey::Enter),
                        "Backspace" => Key::Named(NamedKey::Backspace),
                        "Escape" => Key::Named(NamedKey::Escape),
                        other => Key::Character(other.into()),
                    };
                    self.on_key(&key, None);
                }
                Step::Wait(ms) => self.script_at = Instant::now() + Duration::from_millis(ms),
                Step::Shot(path) => {
                    self.paint_scene();
                    let surface = self.surface.as_ref().unwrap();
                    let (w, h) = (surface.config.width, surface.config.height);
                    let dev = &self.render_cx.devices[surface.dev_id];
                    match capture::capture(dev, self.renderer.as_mut().unwrap(), &self.scene, w, h, &path) {
                        Ok(()) => println!("[void] shot {} ({w}x{h})", path.display()),
                        Err(e) => eprintln!("[void] shot failed: {e}"),
                    }
                }
                Step::Quit => {
                    event_loop.exit();
                    return None;
                }
            }
        }
        Some(self.script_at)
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Void")
            .with_inner_size(winit::dpi::LogicalSize::new(1200.0, 800.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        let size = window.inner_size();
        let surface = pollster::block_on(self.render_cx.create_surface(
            window.clone(),
            size.width.max(1),
            size.height.max(1),
            vello::wgpu::PresentMode::AutoVsync,
        ))
        .expect("surface");
        let renderer = Renderer::new(
            &self.render_cx.devices[surface.dev_id].device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: AaSupport::area_only(),
                num_init_threads: NonZeroUsize::new(1),
                ..Default::default()
            },
        )
        .expect("renderer");
        self.window = Some(window);
        self.surface = Some(surface);
        self.renderer = Some(renderer);
        self.reload();
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::FileChanged => self.reload_at = Some(Instant::now() + Duration::from_millis(30)),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(s) = &mut self.surface {
                    self.render_cx.resize_surface(s, size.width.max(1), size.height.max(1));
                }
                self.frame = None;
                self.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.frame = None;
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.window.as_ref().map(|w| w.scale_factor()).unwrap_or(1.0);
                self.move_to(Point::new(position.x / scale, position.y / scale));
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => self.mouse(state),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if event.logical_key == Key::Named(NamedKey::F5) {
                    self.reload();
                    return;
                }
                self.on_key(&event.logical_key, event.text.as_deref());
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if self.reload_at.is_some_and(|t| t <= now) {
            self.reload_at = None;
            self.reload();
        }
        let mut next = self.reload_at;
        if self.frame.is_none() && !self.script.is_empty() && self.surface.is_some() {
            self.paint_scene();
        }
        next = min_opt(next, self.run_script(event_loop));
        if let Some(engine) = &self.engine {
            let (due, err) = engine.run_timers();
            if let Some(e) = err {
                eprintln!("[void] {e}");
                self.error = Some(e);
                self.request_redraw();
            }
            next = min_opt(next, due);
            if engine.dom.borrow().focus.is_some() {
                if now >= self.caret_at + CARET_BLINK {
                    self.caret_on = !self.caret_on;
                    self.caret_at = now;
                    self.request_redraw();
                }
                next = min_opt(next, Some(self.caret_at + CARET_BLINK));
            }
        }
        self.mark_if_dirty();
        event_loop.set_control_flow(match next {
            Some(t) => ControlFlow::WaitUntil(t),
            None => ControlFlow::Wait,
        });
    }
}

fn min_opt(a: Option<Instant>, b: Option<Instant>) -> Option<Instant> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

fn watch(dir: &Path, proxy: EventLoopProxy<UserEvent>) -> Option<notify::RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            if ev.kind.is_modify() || ev.kind.is_create() || ev.kind.is_remove() {
                let _ = proxy.send_event(UserEvent::FileChanged);
            }
        }
    })
    .ok()?;
    watcher.watch(dir, RecursiveMode::Recursive).ok()?;
    Some(watcher)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut entry_arg = None;
    let mut script = VecDeque::new();
    while let Some(a) = args.next() {
        if a == "--auto" {
            script = parse_script(&args.next().unwrap_or_default());
        } else {
            entry_arg = Some(a);
        }
    }
    let arg = entry_arg.unwrap_or_else(|| "apps/web-void/src/main.tsx".into());
    let mut entry = std::fs::canonicalize(&arg).unwrap_or_else(|_| {
        eprintln!("[void] entry not found: {arg}");
        std::process::exit(1);
    });
    if entry.is_dir() {
        entry = entry.join("src").join("main.tsx");
    }
    // `\\?\` prefixes confuse module specifiers; keep plain paths.
    let entry = PathBuf::from(entry.to_string_lossy().trim_start_matches(r"\\?\").to_string());

    let event_loop = EventLoop::<UserEvent>::with_user_event().build().expect("event loop");
    let watcher = watch(entry.parent().unwrap(), event_loop.create_proxy());
    let mut app = App {
        entry,
        script,
        script_at: Instant::now(),
        _watcher: watcher,
        window: None,
        render_cx: RenderContext::new(),
        surface: None,
        renderer: None,
        scene: Scene::new(),
        fcx: FontContext::new(),
        lcx: LayoutContext::new(),
        engine: None,
        error: None,
        frame: None,
        hits: Vec::new(),
        hover_sensitive: false,
        cursor: Point::ZERO,
        pressed: None,
        caret_on: true,
        caret_at: Instant::now(),
        reload_at: None,
        boot_ms: 0.0,
        layout_ms: 0.0,
        paint_ms: 0.0,
    };
    event_loop.run_app(&mut app).expect("run");
}
