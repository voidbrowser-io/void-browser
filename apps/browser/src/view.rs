//! A View is one page: its own JS engine, DOM, styles and layout. The
//! browser UI is a View, and so is every tab.

use crate::pages;
use parley::{FontContext, LayoutContext};
use std::path::PathBuf;
use std::time::Instant;
use url::Url;
use vello::kurbo::{Affine, Point};
use vello::Scene;
use void_css::{cascade, uses_hover, UA_CSS};
use void_dom::{NodeId, NodeKind};
use void_js::{Engine, HostCmd};
use void_layout::{Frame, TextBrush};
use void_paint::{error_overlay, HitBox, Painter};
use winit::keyboard::{Key, NamedKey};

pub struct View {
    pub url: Url,
    pub title: String,
    pub engine: Option<Engine>,
    pub error: Option<String>,
    pub back: Vec<Url>,
    pub forward: Vec<Url>,
    frame: Option<Frame>,
    frame_size: (f32, f32),
    hits: Vec<HitBox>,
    hover_sensitive: bool,
    pub boot_ms: f64,
    pub layout_ms: f64,
    pub paint_ms: f64,
}

impl View {
    pub fn new(url: Url) -> View {
        View {
            title: default_title(&url),
            url,
            engine: None,
            error: None,
            back: Vec::new(),
            forward: Vec::new(),
            frame: None,
            frame_size: (0.0, 0.0),
            hits: Vec::new(),
            hover_sensitive: false,
            boot_ms: 0.0,
            layout_ms: 0.0,
            paint_ms: 0.0,
        }
    }

    pub fn load(&mut self, host_info: &str) {
        let t0 = Instant::now();
        // Drop the old world first: timers hold handles into the old runtime.
        self.frame = None;
        self.engine = None;
        self.hits.clear();
        let privileged = pages::privileged(&self.url);
        let (engine, error) = match Engine::boot(&self.url, UA_CSS, pages::builtin, privileged, host_info.to_string()) {
            Ok(e) => (e, None),
            Err((e, msg)) => (e, Some(msg)),
        };
        if let Some(err) = &error {
            eprintln!("[void] {}: {err}", self.url);
        }
        self.hover_sensitive = uses_hover(&engine.shared.borrow().css);
        self.title = engine.html_title.clone().unwrap_or_else(|| default_title(&self.url));
        let stats = engine.shared.borrow().stats;
        self.engine = Some(engine);
        self.error = error;
        self.boot_ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!(
            "[void] {} in {:.1} ms ({} modules, fetch {:.1} ms, transpile {:.2} ms)",
            self.url, self.boot_ms, stats.modules, stats.fetch_ms, stats.transpile_ms
        );
    }

    /// Navigate to a new URL, recording history.
    pub fn go(&mut self, url: Url, host_info: &str) {
        let old = std::mem::replace(&mut self.url, url);
        self.back.push(old);
        self.forward.clear();
        self.load(host_info);
    }

    pub fn go_back(&mut self, host_info: &str) -> bool {
        let Some(url) = self.back.pop() else { return false };
        let old = std::mem::replace(&mut self.url, url);
        self.forward.push(old);
        self.load(host_info);
        true
    }

    pub fn go_forward(&mut self, host_info: &str) -> bool {
        let Some(url) = self.forward.pop() else { return false };
        let old = std::mem::replace(&mut self.url, url);
        self.back.push(old);
        self.load(host_info);
        true
    }

    /// Directory to watch for hot reload (local pages only).
    pub fn watch_dir(&self) -> Option<PathBuf> {
        let path = self.url.to_file_path().ok()?;
        Some(if path.is_dir() { path } else { path.parent()?.to_path_buf() })
    }

    pub fn dirty(&self) -> bool {
        self.engine.as_ref().is_some_and(|e| e.dom.borrow().dirty)
    }

    fn relayout(&mut self, size: (f32, f32), fcx: &mut FontContext, lcx: &mut LayoutContext<TextBrush>) {
        let Some(engine) = &self.engine else { return };
        let t0 = Instant::now();
        let mut dom = engine.dom.borrow_mut();
        let styles = cascade(&dom, &engine.shared.borrow().css);
        self.frame = Some(void_layout::build(&dom, styles, size, fcx, lcx));
        self.frame_size = size;
        dom.dirty = false;
        self.layout_ms = t0.elapsed().as_secs_f64() * 1000.0;
    }

    /// Paint into `scene`. `transform` maps this view's CSS pixels to the window.
    pub fn paint(
        &mut self,
        scene: &mut Scene,
        fcx: &mut FontContext,
        lcx: &mut LayoutContext<TextBrush>,
        transform: Affine,
        size: (f32, f32),
        caret_on: bool,
    ) {
        if self.engine.is_none() {
            return;
        }
        if self.frame.is_none() || self.dirty() || self.frame_size != size {
            self.relayout(size, fcx, lcx);
        }
        let t0 = Instant::now();
        let engine = self.engine.as_ref().unwrap();
        let dom = engine.dom.borrow();
        let mut painter = Painter { scene, fcx, lcx, dom: &dom, transform, hits: Vec::new(), caret_on };
        if let Some(frame) = &self.frame {
            painter.paint(frame);
        }
        self.hits = std::mem::take(&mut painter.hits);
        drop(dom);
        if let Some(err) = &self.error {
            error_overlay(scene, fcx, lcx, transform, size, err);
        }
        self.paint_ms = t0.elapsed().as_secs_f64() * 1000.0;
    }

    pub fn hit(&self, p: Point) -> Option<NodeId> {
        self.hits.iter().rev().find(|h| h.rect.contains(p)).map(|h| h.id)
    }

    /// Update `:hover`. Returns whether the node under the pointer is clickable.
    pub fn hover(&mut self, hit: Option<NodeId>) -> bool {
        let Some(engine) = &self.engine else { return false };
        let mut dom = engine.dom.borrow_mut();
        if dom.hover != hit {
            dom.hover = hit;
            if self.hover_sensitive {
                dom.dirty = true;
            }
        }
        hit.is_some_and(|h| {
            dom.path(h).iter().any(|&n| {
                let node = &dom.nodes[n as usize];
                node.listeners.contains("click") || matches!(node.tag(), Some("input") | Some("a"))
            })
        })
    }

    pub fn press(&mut self, hit: Option<NodeId>) {
        let Some(engine) = &self.engine else { return };
        let mut dom = engine.dom.borrow_mut();
        dom.active = hit;
        match hit.filter(|&h| dom.nodes[h as usize].tag() == Some("input")) {
            Some(input) if dom.focus != Some(input) => dom.focus_input(input),
            Some(_) => {
                let f = dom.focus.unwrap();
                dom.nodes[f as usize].selected = false;
            }
            None => dom.focus = None,
        }
        dom.dirty = true;
    }

    pub fn release(&mut self) {
        if let Some(engine) = &self.engine {
            let mut dom = engine.dom.borrow_mut();
            dom.active = None;
            dom.dirty = true;
        }
    }

    pub fn blur(&mut self) {
        if let Some(engine) = &self.engine {
            let mut dom = engine.dom.borrow_mut();
            if dom.focus.take().is_some() {
                dom.dirty = true;
            }
        }
    }

    pub fn has_focus(&self) -> bool {
        self.engine.as_ref().is_some_and(|e| e.dom.borrow().focus.is_some())
    }

    /// Click from `down` to `up`. Returns the link to follow, if any.
    pub fn click(&mut self, down: NodeId, up: NodeId) -> Option<Url> {
        let engine = self.engine.as_ref()?;
        let (target, path) = {
            let dom = engine.dom.borrow();
            let target = dom.path(up).into_iter().find(|&n| dom.is_ancestor(n, down))?;
            (target, dom.path(target))
        };
        if !self.dispatch(target, "click", "", "") {
            return None;
        }
        // Default action: follow `<a href>`.
        let engine = self.engine.as_ref()?;
        let dom = engine.dom.borrow();
        path.iter().find_map(|&n| match &dom.nodes[n as usize].kind {
            NodeKind::Element { tag, attrs, .. } if tag == "a" => attrs.get("href").and_then(|h| self.url.join(h).ok()),
            _ => None,
        })
    }

    /// Returns `true` unless the page called `preventDefault()`.
    pub fn dispatch(&mut self, target: NodeId, ty: &str, value: &str, key: &str) -> bool {
        let Some(engine) = &self.engine else { return false };
        let path = engine.dom.borrow().path(target);
        match engine.dispatch(path, ty, value, key) {
            Ok(allowed) => allowed,
            Err(e) => {
                eprintln!("[void] {e}");
                self.error = Some(e);
                false
            }
        }
    }

    /// Text editing for the focused `<input>`, then `input`/`keydown` events.
    pub fn key(&mut self, key: &Key, text: Option<&str>) {
        let Some(engine) = &self.engine else { return };
        let key_name = match key {
            Key::Named(n) => format!("{n:?}"),
            Key::Character(c) => c.to_string(),
            _ => String::new(),
        };
        let Some(id) = engine.dom.borrow().focus else {
            self.dispatch(void_dom::ROOT, "keydown", "", &key_name);
            return;
        };
        let changed = {
            let mut dom = engine.dom.borrow_mut();
            dom.dirty = true;
            let node = &mut dom.nodes[id as usize];
            let typed = text.filter(|t| !t.is_empty() && t.chars().all(|c| !c.is_control()));
            let changed = match (key, typed) {
                (Key::Named(NamedKey::Backspace), _) if node.selected => {
                    node.value.clear();
                    true
                }
                (Key::Named(NamedKey::Backspace), _) => node.value.pop().is_some(),
                (Key::Named(NamedKey::Escape), _) => {
                    dom.focus = None;
                    false
                }
                (_, Some(t)) => {
                    if node.selected {
                        node.value.clear();
                    }
                    node.value.push_str(t);
                    true
                }
                _ => false,
            };
            if let Some(n) = dom.nodes.get_mut(id as usize) {
                if !matches!(key, Key::Named(NamedKey::Enter)) {
                    n.selected = false;
                }
            }
            changed
        };
        let value = engine.dom.borrow().nodes[id as usize].value.clone();
        if changed {
            self.dispatch(id, "input", &value, &key_name);
        }
        self.dispatch(id, "keydown", &value, &key_name);
    }

    /// Insert text (paste) into the focused input.
    pub fn insert_text(&mut self, text: &str) {
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        if !text.is_empty() {
            self.key(&Key::Character(text.as_str().into()), Some(&text));
        }
    }

    pub fn select_all(&mut self) {
        if let Some(engine) = &self.engine {
            let mut dom = engine.dom.borrow_mut();
            if let Some(f) = dom.focus {
                dom.nodes[f as usize].selected = true;
                dom.dirty = true;
            }
        }
    }

    pub fn focused_value(&self) -> Option<String> {
        let dom = self.engine.as_ref()?.dom.borrow();
        dom.focus.map(|f| dom.nodes[f as usize].value.clone())
    }

    /// All text on the page, plus the error message if the page failed.
    pub fn page_text(&self) -> String {
        let mut out = self.error.clone().unwrap_or_default();
        if let Some(e) = &self.engine {
            out.push_str(&text_of(&e.dom.borrow(), void_dom::ROOT));
        }
        out
    }

    /// Center of the smallest painted element whose text (or placeholder) is `text`.
    pub fn find_text(&self, text: &str) -> Option<Point> {
        let dom = self.engine.as_ref()?.dom.borrow();
        self.hits
            .iter()
            .filter(|h| {
                let node = &dom.nodes[h.id as usize];
                let placeholder = match &node.kind {
                    NodeKind::Element { attrs, .. } => attrs.get("placeholder").map(String::as_str),
                    _ => None,
                };
                text_of(&dom, h.id).trim() == text || placeholder == Some(text)
            })
            .min_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))
            .map(|h| h.rect.center())
    }

    pub fn take_cmds(&self) -> Vec<HostCmd> {
        self.engine.as_ref().map(|e| e.take_cmds()).unwrap_or_default()
    }

    pub fn call(&self, name: &str, arg: &str) {
        if let Some(e) = &self.engine {
            if let Err(err) = e.call_global(name, arg) {
                eprintln!("[void] {err}");
            }
        }
    }

    pub fn run_timers(&mut self) -> Option<Instant> {
        let engine = self.engine.as_ref()?;
        let (due, err) = engine.run_timers();
        if let Some(e) = err {
            eprintln!("[void] {e}");
            self.error = Some(e);
        }
        due
    }
}

fn text_of(dom: &void_dom::Dom, id: NodeId) -> String {
    match &dom.nodes[id as usize].kind {
        NodeKind::Text(t) => t.clone(),
        NodeKind::Element { .. } => dom.nodes[id as usize].children.iter().map(|&c| text_of(dom, c)).collect(),
    }
}

fn default_title(url: &Url) -> String {
    match url.scheme() {
        "void" => url.host_str().unwrap_or("void").to_string(),
        "file" => url
            .path_segments()
            .and_then(|s| s.filter(|p| !p.is_empty() && *p != "index.html" && *p != "src").last())
            .unwrap_or("file")
            .to_string(),
        _ => url.host_str().unwrap_or(url.as_str()).to_string(),
    }
}
