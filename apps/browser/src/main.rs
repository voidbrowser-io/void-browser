//! Void Browser — a modern-only web runtime for Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//!
//! usage: void [url-or-path] [--auto "click 120 80; wait 200; type hi; key Enter; shot out.png; quit"]

mod capture;
mod pages;
mod view;

use notify::{RecursiveMode, Watcher};
use parley::{FontContext, LayoutContext};
use std::collections::{HashSet, VecDeque};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use url::Url;
use vello::kurbo::{Affine, Point, Rect};
use vello::peniko::{BlendMode, Color, Fill};
use vello::util::{RenderContext, RenderSurface};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};
use view::View;
use void_dom::NodeId;
use void_js::{parse_input, HostCmd};
use void_layout::TextBrush;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Window, WindowId};

const CARET_BLINK: Duration = Duration::from_millis(530);
/// Height of the browser UI (tab strip + toolbar), in CSS pixels.
const CHROME_H: f32 = 84.0;
const NEW_TAB: &str = "void://newtab";

#[derive(Debug)]
enum UserEvent {
    FileChanged(Vec<PathBuf>),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Area {
    Chrome,
    Content,
}

/// One step of an `--auto` script. Coordinates are CSS pixels of the window.
#[derive(Debug, Clone)]
enum Step {
    Click(f64, f64),
    ClickText(String),
    Move(f64, f64),
    Type(String),
    Key(String),
    Nav(String),
    Wait(u64),
    Shot(PathBuf),
    Expect(String),
    ExpectTitle(String),
    ExpectTabs(usize),
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
                "click-text" => Step::ClickText(rest.trim().to_string()),
                "expect" => Step::Expect(rest.trim().to_string()),
                "expect-title" => Step::ExpectTitle(rest.trim().to_string()),
                "expect-tabs" => Step::ExpectTabs(*nums.first()? as usize),
                "move" => Step::Move(*nums.first()?, *nums.get(1)?),
                "type" => Step::Type(rest.to_string()),
                "key" => Step::Key(rest.trim().to_string()),
                "nav" => Step::Nav(rest.trim().to_string()),
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

/// "ctrl+shift+t" → (modifiers, key)
fn parse_key(spec: &str) -> (ModifiersState, Key) {
    let mut mods = ModifiersState::empty();
    let mut key = Key::Named(NamedKey::Enter);
    for part in spec.split('+') {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" => mods |= ModifiersState::CONTROL,
            "shift" => mods |= ModifiersState::SHIFT,
            "alt" => mods |= ModifiersState::ALT,
            "enter" => key = Key::Named(NamedKey::Enter),
            "backspace" => key = Key::Named(NamedKey::Backspace),
            "escape" => key = Key::Named(NamedKey::Escape),
            "tab" => key = Key::Named(NamedKey::Tab),
            "left" => key = Key::Named(NamedKey::ArrowLeft),
            "right" => key = Key::Named(NamedKey::ArrowRight),
            "f5" => key = Key::Named(NamedKey::F5),
            "space" => key = Key::Character(" ".into()),
            other => key = Key::Character(other.into()),
        }
    }
    (mods, key)
}

struct App {
    script: VecDeque<Step>,
    script_at: Instant,
    proxy: EventLoopProxy<UserEvent>,
    watcher: Option<notify::RecommendedWatcher>,
    watched: HashSet<PathBuf>,
    host_info: String,

    window: Option<Arc<Window>>,
    render_cx: RenderContext,
    surface: Option<RenderSurface<'static>>,
    renderer: Option<Renderer>,
    scene: Scene,
    fcx: FontContext,
    lcx: LayoutContext<TextBrush>,
    clipboard: Option<arboard::Clipboard>,

    chrome: View,
    tabs: Vec<View>,
    active: usize,
    initial: Vec<Url>,

    focus: Area,
    cursor: Point,
    pressed: Option<(Area, NodeId)>,
    mods: ModifiersState,
    caret_on: bool,
    caret_at: Instant,
    reload_at: Option<Instant>,
    reload_dirs: Vec<PathBuf>,
    /// An `--auto` check failed; exit with code 1.
    failed: bool,
}

impl App {
    fn check(&mut self, ok: bool, what: String) {
        if ok {
            println!("[void] ok   {what}");
        } else {
            eprintln!("[void] FAIL {what}");
            self.failed = true;
        }
    }

    fn request_redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn view(&mut self, area: Area) -> &mut View {
        match area {
            Area::Chrome => &mut self.chrome,
            Area::Content => &mut self.tabs[self.active],
        }
    }

    fn logical_size(&self) -> (f32, f32, f64) {
        let w = self.window.as_ref().unwrap();
        let scale = w.scale_factor();
        let size = w.inner_size();
        ((size.width as f64 / scale) as f32, (size.height as f64 / scale) as f32, scale)
    }

    fn area_at(&self, p: Point) -> (Area, Point) {
        if p.y < CHROME_H as f64 {
            (Area::Chrome, p)
        } else {
            (Area::Content, Point::new(p.x, p.y - CHROME_H as f64))
        }
    }

    // ───────────── Tabs & navigation ─────────────

    fn watch(&mut self, i: usize) {
        let Some(dir) = self.tabs[i].watch_dir() else { return };
        if self.watched.contains(&dir) {
            return;
        }
        if self.watcher.is_none() {
            let proxy = self.proxy.clone();
            self.watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(ev) = res {
                    if ev.kind.is_modify() || ev.kind.is_create() || ev.kind.is_remove() {
                        let _ = proxy.send_event(UserEvent::FileChanged(ev.paths));
                    }
                }
            })
            .ok();
        }
        if let Some(w) = &mut self.watcher {
            if w.watch(&dir, RecursiveMode::Recursive).is_ok() {
                self.watched.insert(dir);
            }
        }
    }

    fn open_tab(&mut self, url: Option<Url>) {
        let url = url.unwrap_or_else(|| Url::parse(NEW_TAB).unwrap());
        let mut v = View::new(url);
        v.load(&self.host_info);
        self.content_blur();
        self.tabs.push(v);
        self.active = self.tabs.len() - 1;
        self.watch(self.active);
        if self.tabs[self.active].url.as_str().starts_with(NEW_TAB) {
            self.focus_address_bar();
        }
        self.sync_chrome();
    }

    fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        self.tabs.remove(i);
        if self.tabs.is_empty() {
            self.open_tab(None);
            return;
        }
        if self.active > i || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
        self.sync_chrome();
    }

    fn activate(&mut self, i: usize) {
        if i < self.tabs.len() && i != self.active {
            self.content_blur();
            self.active = i;
            self.sync_chrome();
        }
    }

    fn navigate(&mut self, input: &str) {
        let i = self.active;
        match parse_input(input) {
            Ok(url) => {
                self.tabs[i].go(url, &self.host_info);
                self.watch(i);
                self.focus = Area::Content;
                self.chrome.blur();
            }
            Err(e) => self.tabs[i].error = Some(e),
        }
        self.sync_chrome();
    }

    fn content_blur(&mut self) {
        if let Some(t) = self.tabs.get_mut(self.active) {
            t.blur();
        }
    }

    fn focus_address_bar(&mut self) {
        self.chrome.call("__browser_focus_url", "");
        self.focus = Area::Chrome;
        self.caret_on = true;
        self.caret_at = Instant::now();
    }

    /// Push tab state into the browser UI page and the window title.
    fn sync_chrome(&mut self) {
        let tab = &self.tabs[self.active];
        let mut json = String::from("{\"tabs\":[");
        for (i, t) in self.tabs.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                "{{\"title\":{},\"url\":{},\"error\":{}}}",
                json_str(&t.title),
                json_str(t.url.as_str()),
                t.error.is_some()
            ));
        }
        json.push_str(&format!(
            "],\"active\":{},\"canBack\":{},\"canForward\":{}}}",
            self.active,
            !tab.back.is_empty(),
            !tab.forward.is_empty()
        ));
        self.chrome.call("__browser_state", &json);
        if let Some(w) = &self.window {
            w.set_title(&format!("{} — Void", tab.title));
        }
        self.request_redraw();
    }

    /// Apply requests that pages made to the browser.
    fn process_cmds(&mut self) {
        for _ in 0..8 {
            let mut work: Vec<(Option<usize>, HostCmd)> = self.chrome.take_cmds().into_iter().map(|c| (None, c)).collect();
            for (i, t) in self.tabs.iter().enumerate() {
                work.extend(t.take_cmds().into_iter().map(|c| (Some(i), c)));
            }
            if work.is_empty() {
                return;
            }
            for (from, cmd) in work {
                match cmd {
                    HostCmd::Navigate(to) => {
                        if let Some(i) = from {
                            self.active = i;
                        }
                        self.navigate(&to);
                    }
                    HostCmd::Title(t) => {
                        if let Some(i) = from {
                            self.tabs[i].title = t;
                            self.sync_chrome();
                        }
                    }
                    HostCmd::NewTab(url) => {
                        let url = url.and_then(|u| parse_input(&u).ok());
                        self.open_tab(url);
                    }
                    HostCmd::CloseTab(i) => self.close_tab(i),
                    HostCmd::Activate(i) => self.activate(i),
                    HostCmd::Back => {
                        let i = self.active;
                        if self.tabs[i].go_back(&self.host_info) {
                            self.watch(i);
                        }
                        self.sync_chrome();
                    }
                    HostCmd::Forward => {
                        let i = self.active;
                        if self.tabs[i].go_forward(&self.host_info) {
                            self.watch(i);
                        }
                        self.sync_chrome();
                    }
                    HostCmd::Reload => {
                        self.tabs[self.active].load(&self.host_info);
                        self.sync_chrome();
                    }
                }
            }
        }
    }

    // ───────────── Rendering ─────────────

    fn paint_scene(&mut self) {
        let (w, h, scale) = self.logical_size();
        self.scene.reset();
        let base = Affine::scale(scale);
        // Content first, clipped below the browser UI.
        let content_t = base * Affine::translate((0.0, CHROME_H as f64));
        let clip = Rect::new(0.0, 0.0, w as f64, (h - CHROME_H).max(0.0) as f64);
        self.scene.push_layer(Fill::NonZero, BlendMode::default(), 1.0, content_t, &clip);
        let caret = self.caret_on;
        let focus = self.focus;
        let tab = &mut self.tabs[self.active];
        tab.paint(&mut self.scene, &mut self.fcx, &mut self.lcx, content_t, (w, (h - CHROME_H).max(1.0)), caret && focus == Area::Content);
        self.scene.pop_layer();
        self.chrome.paint(&mut self.scene, &mut self.fcx, &mut self.lcx, base, (w, CHROME_H), caret && focus == Area::Chrome);
    }

    fn redraw(&mut self) {
        if self.surface.is_none() || self.tabs.is_empty() {
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
    }

    fn any_dirty(&self) -> bool {
        self.chrome.dirty() || self.tabs.get(self.active).is_some_and(|t| t.dirty())
    }

    // ───────────── Input ─────────────

    fn move_to(&mut self, p: Point) {
        self.cursor = p;
        let (area, local) = self.area_at(p);
        let other = if area == Area::Chrome { Area::Content } else { Area::Chrome };
        self.view(other).hover(None);
        let v = self.view(area);
        let hit = v.hit(local);
        let clickable = v.hover(hit);
        if let Some(w) = &self.window {
            w.set_cursor(if clickable { CursorIcon::Pointer } else { CursorIcon::Default });
        }
        if self.any_dirty() {
            self.request_redraw();
        }
    }

    fn mouse(&mut self, state: ElementState) {
        let (area, local) = self.area_at(self.cursor);
        let hit = self.view(area).hit(local);
        match state {
            ElementState::Pressed => {
                if self.focus != area {
                    let old = self.focus;
                    self.view(old).blur();
                }
                self.focus = area;
                self.view(area).press(hit);
                self.pressed = hit.map(|h| (area, h));
                self.caret_on = true;
                self.caret_at = Instant::now();
            }
            ElementState::Released => {
                self.view(area).release();
                if let (Some((down_area, down)), Some(up)) = (self.pressed.take(), hit) {
                    if down_area == area {
                        if let Some(link) = self.view(area).click(down, up) {
                            if area == Area::Content {
                                self.navigate(link.as_str());
                            }
                        }
                    }
                }
                self.process_cmds();
            }
        }
        self.request_redraw();
    }

    fn on_key(&mut self, key: &Key, text: Option<&str>, mods: ModifiersState) {
        let ctrl = mods.control_key();
        let ch = match key {
            Key::Character(c) => c.to_lowercase(),
            _ => String::new(),
        };
        // Browser shortcuts.
        match (key, ctrl, mods.alt_key(), ch.as_str()) {
            (_, true, _, "t") => return self.open_tab(None),
            (_, true, _, "w") => return self.close_tab(self.active),
            (_, true, _, "l") => {
                self.focus_address_bar();
                return self.request_redraw();
            }
            (_, true, _, "r") | (Key::Named(NamedKey::F5), _, _, _) => {
                self.tabs[self.active].load(&self.host_info);
                return self.sync_chrome();
            }
            (Key::Named(NamedKey::Tab), true, _, _) => {
                let n = self.tabs.len();
                let next = if mods.shift_key() { (self.active + n - 1) % n } else { (self.active + 1) % n };
                return self.activate(next);
            }
            (Key::Named(NamedKey::ArrowLeft), false, true, _) => {
                self.tabs[self.active].go_back(&self.host_info);
                return self.sync_chrome();
            }
            (Key::Named(NamedKey::ArrowRight), false, true, _) => {
                self.tabs[self.active].go_forward(&self.host_info);
                return self.sync_chrome();
            }
            (_, true, _, d) if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() && d != "0" => {
                let n: usize = d.parse().unwrap();
                return self.activate(if n == 9 { self.tabs.len() - 1 } else { n - 1 });
            }
            _ => {}
        }
        let area = self.focus;
        if ctrl {
            match ch.as_str() {
                "a" => self.view(area).select_all(),
                "c" => {
                    if let (Some(v), Some(cb)) = (self.view(area).focused_value(), self.clipboard.as_mut()) {
                        let _ = cb.set_text(v);
                    }
                }
                "v" => {
                    if let Some(t) = self.clipboard.as_mut().and_then(|cb| cb.get_text().ok()) {
                        self.view(area).insert_text(&t);
                    }
                }
                _ => {}
            }
        } else {
            self.view(area).key(key, text);
        }
        self.caret_on = true;
        self.caret_at = Instant::now();
        self.process_cmds();
        self.request_redraw();
    }

    /// Run due `--auto` steps once the first frame exists.
    fn run_script(&mut self, event_loop: &ActiveEventLoop) -> Option<Instant> {
        if self.script.is_empty() || self.surface.is_none() {
            return None;
        }
        while Instant::now() >= self.script_at {
            let Some(step) = self.script.pop_front() else { return None };
            // Hit boxes must reflect the latest DOM before synthetic input.
            self.paint_scene();
            match step {
                Step::Move(x, y) => self.move_to(Point::new(x, y)),
                Step::ClickText(text) => {
                    let content = self.tabs[self.active].find_text(&text).map(|p| Point::new(p.x, p.y + CHROME_H as f64));
                    match content.or_else(|| self.chrome.find_text(&text)) {
                        Some(p) => {
                            self.move_to(p);
                            self.mouse(ElementState::Pressed);
                            self.mouse(ElementState::Released);
                        }
                        None => self.check(false, format!("click-text {text:?}: not found")),
                    }
                }
                Step::Expect(text) => {
                    self.paint_scene();
                    let page = self.tabs[self.active].page_text();
                    self.check(page.contains(&text), format!("expect {text:?}"));
                }
                Step::ExpectTitle(t) => {
                    let title = self.tabs[self.active].title.clone();
                    self.check(title == t, format!("expect-title {t:?} (got {title:?})"));
                }
                Step::ExpectTabs(n) => {
                    let got = self.tabs.len();
                    self.check(got == n, format!("expect-tabs {n} (got {got})"));
                }
                Step::Click(x, y) => {
                    self.move_to(Point::new(x, y));
                    self.mouse(ElementState::Pressed);
                    self.mouse(ElementState::Released);
                }
                Step::Type(text) => {
                    for ch in text.chars() {
                        let s = ch.to_string();
                        self.on_key(&Key::Character(s.as_str().into()), Some(&s), ModifiersState::empty());
                    }
                }
                Step::Key(spec) => {
                    let (mods, key) = parse_key(&spec);
                    self.on_key(&key, None, mods);
                }
                Step::Nav(url) => self.navigate(&url),
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

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Void")
            .with_inner_size(winit::dpi::LogicalSize::new(1200.0, 820.0));
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

        self.chrome.load(&self.host_info);
        for url in std::mem::take(&mut self.initial) {
            self.open_tab(Some(url));
        }
        if self.tabs.is_empty() {
            self.open_tab(None);
        }
        self.activate(0);
        self.sync_chrome();
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::FileChanged(paths) => {
                self.reload_dirs.extend(paths);
                self.reload_at = Some(Instant::now() + Duration::from_millis(30));
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(s) = &mut self.surface {
                    self.render_cx.resize_surface(s, size.width.max(1), size.height.max(1));
                }
                self.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => self.request_redraw(),
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.window.as_ref().map(|w| w.scale_factor()).unwrap_or(1.0);
                self.move_to(Point::new(position.x / scale, position.y / scale));
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => self.mouse(state),
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Back, .. } => {
                self.on_key(&Key::Named(NamedKey::ArrowLeft), None, ModifiersState::ALT)
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Forward, .. } => {
                self.on_key(&Key::Named(NamedKey::ArrowRight), None, ModifiersState::ALT)
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let mods = self.mods;
                self.on_key(&event.logical_key, event.text.as_deref(), mods);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if self.reload_at.is_some_and(|t| t <= now) {
            self.reload_at = None;
            let changed = std::mem::take(&mut self.reload_dirs);
            for i in 0..self.tabs.len() {
                let Some(dir) = self.tabs[i].watch_dir() else { continue };
                if changed.iter().any(|p| p.starts_with(&dir)) {
                    self.tabs[i].load(&self.host_info);
                }
            }
            self.sync_chrome();
        }
        let mut next = self.reload_at;
        next = min_opt(next, self.run_script(event_loop));
        next = min_opt(next, self.chrome.run_timers());
        for t in &mut self.tabs {
            next = min_opt(next, t.run_timers());
        }
        self.process_cmds();
        let focus = self.focus;
        if self.window.is_some() && !self.tabs.is_empty() && self.view(focus).has_focus() {
            if now >= self.caret_at + CARET_BLINK {
                self.caret_on = !self.caret_on;
                self.caret_at = now;
                self.request_redraw();
            }
            next = min_opt(next, Some(self.caret_at + CARET_BLINK));
        }
        if self.any_dirty() {
            self.request_redraw();
        }
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

/// Demo links for the new-tab page.
fn host_info() -> String {
    let mut demos = vec![(
        "Demo app · built in",
        "The demo ships inside Void — works offline, no files needed.",
        "void://demo".to_string(),
    )];
    // Running from the repo: offer the on-disk copy (hot reload) and the HTTP setup.
    if let Ok(p) = std::fs::canonicalize("apps/web-void/index.html") {
        let p = p.to_string_lossy().trim_start_matches(r"\\?\").to_string();
        if let Ok(u) = Url::from_file_path(p) {
            demos.push(("Demo app · local", "Plain .tsx from disk. Edit a file — it hot-reloads.", u.to_string()));
            demos.push((
                "Demo app · over HTTP",
                "Run: python -m http.server 8080 -d apps/web-void",
                "http://localhost:8080/".to_string(),
            ));
        }
    }
    let items: Vec<String> = demos
        .iter()
        .map(|(t, d, u)| format!("{{\"title\":{},\"desc\":{},\"url\":{}}}", json_str(t), json_str(d), json_str(u)))
        .collect();
    format!("{{\"demos\":[{}]}}", items.join(","))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut initial = Vec::new();
    let mut script = VecDeque::new();
    while let Some(a) = args.next() {
        if a == "--auto" {
            script = parse_script(&args.next().unwrap_or_default());
        } else {
            match parse_input(&a) {
                Ok(u) => initial.push(u),
                Err(e) => eprintln!("[void] {e}"),
            }
        }
    }

    let event_loop = EventLoop::<UserEvent>::with_user_event().build().expect("event loop");
    let mut app = App {
        script,
        script_at: Instant::now(),
        proxy: event_loop.create_proxy(),
        watcher: None,
        watched: HashSet::new(),
        host_info: host_info(),
        window: None,
        render_cx: RenderContext::new(),
        surface: None,
        renderer: None,
        scene: Scene::new(),
        fcx: FontContext::new(),
        lcx: LayoutContext::new(),
        clipboard: arboard::Clipboard::new().ok(),
        chrome: View::new(Url::parse("void://chrome").unwrap()),
        tabs: Vec::new(),
        active: 0,
        initial,
        focus: Area::Content,
        cursor: Point::ZERO,
        pressed: None,
        mods: ModifiersState::empty(),
        caret_on: true,
        caret_at: Instant::now(),
        reload_at: None,
        reload_dirs: Vec::new(),
        failed: false,
    };
    event_loop.run_app(&mut app).expect("run");
    if app.failed {
        std::process::exit(1);
    }
}
