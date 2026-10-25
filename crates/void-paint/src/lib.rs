//! Paint: walk the laid-out tree and record a Vello scene (GPU vector graphics).

use parley::{FontContext, Layout, LayoutContext, PositionedLayoutItem};
use vello::kurbo::{Affine, BezPath, Point, Rect, RoundedRect, Shape, Stroke, Vec2};
use vello::peniko::{BlendMode, Color, Fill, Gradient};
use vello::Scene;
use void_css::{Background, Rgba, Style};
use void_dom::{Dom, NodeId};
use void_layout::{text_layout, Ctx, Frame, TextBrush, TextRun};

pub fn color(c: Rgba) -> Color {
    Color::new([c.r, c.g, c.b, c.a])
}

/// A painted box that can receive pointer input.
pub struct HitBox {
    pub id: NodeId,
    pub rect: Rect,
    /// The part of the page where this box is visible (ancestor overflow clips).
    pub clip: Rect,
}

impl HitBox {
    /// Hit only where the box is actually visible: scrolled-away content is not clickable.
    pub fn contains(&self, p: Point) -> bool {
        self.rect.contains(p) && self.clip.contains(p)
    }
}

/// A scroll container with somewhere to scroll, as painted this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scroller {
    pub id: NodeId,
    /// Visible padding box (already clipped by ancestors).
    pub clip: Rect,
    /// Maximum scroll offset per axis.
    pub max: Vec2,
    /// Offset used for this frame (the stored offset, clamped to `max`).
    pub offset: Vec2,
}

impl Scroller {
    /// Clamp a wanted offset to what this container allows.
    pub fn clamp(&self, offset: Vec2) -> Vec2 {
        Vec2::new(offset.x.clamp(0.0, self.max.x), offset.y.clamp(0.0, self.max.y))
    }
}

pub struct Painter<'a> {
    pub scene: &'a mut Scene,
    pub fcx: &'a mut FontContext,
    pub lcx: &'a mut LayoutContext<TextBrush>,
    pub dom: &'a Dom,
    pub transform: Affine,
    pub hits: Vec<HitBox>,
    /// Scroll containers, outermost first.
    pub scrollers: Vec<Scroller>,
    pub caret_on: bool,
}

const EVERYWHERE: Rect = Rect::new(-1e9, -1e9, 1e9, 1e9);
const MIN_THUMB: f64 = 24.0;

impl Painter<'_> {
    pub fn paint(&mut self, frame: &Frame) {
        self.node(frame, frame.root, Point::ZERO, EVERYWHERE);
    }

    fn node(&mut self, frame: &Frame, n: taffy::NodeId, origin: Point, clip: Rect) {
        let Ok(l) = frame.tree.layout(n) else { return };
        let pos = origin + (l.location.x as f64, l.location.y as f64);
        let rect = Rect::new(pos.x, pos.y, pos.x + l.size.width as f64, pos.y + l.size.height as f64);
        match frame.tree.get_node_context(n) {
            Some(Ctx::Element(id)) => {
                let style = &frame.styles[id];
                let fade = style.opacity < 1.0;
                self.box_decoration(style, rect, fade);
                self.hits.push(HitBox { id: *id, rect, clip });

                let (mut child_origin, mut child_clip) = (pos, clip);
                let mut thumbs = None;
                if style.clips() {
                    let inner = Rect::new(
                        rect.x0 + l.border.left as f64,
                        rect.y0 + l.border.top as f64,
                        rect.x1 - l.border.right as f64,
                        rect.y1 - l.border.bottom as f64,
                    );
                    let r = (radius(style, rect) - style.border_width as f64).max(0.0);
                    let shape = BoxShape::new(inner, r, style.squircle).path();
                    self.scene.push_layer(Fill::NonZero, BlendMode::default(), 1.0, self.transform, &shape);
                    child_clip = clip.intersect(inner);
                    let max = Vec2::new(
                        if style.overflow_x.scrolls() { l.scroll_width() as f64 } else { 0.0 },
                        if style.overflow_y.scrolls() { l.scroll_height() as f64 } else { 0.0 },
                    );
                    if max.x > 0.0 || max.y > 0.0 {
                        let stored = self.dom.get(*id).map(|n| n.scroll).unwrap_or_default();
                        let offset = Vec2::new((stored.0 as f64).clamp(0.0, max.x), (stored.1 as f64).clamp(0.0, max.y));
                        child_origin = pos - offset;
                        self.scrollers.push(Scroller { id: *id, clip: child_clip, max, offset });
                        thumbs = Some((inner, max, offset));
                    }
                }
                for c in frame.tree.children(n).unwrap_or_default() {
                    self.node(frame, c, child_origin, child_clip);
                }
                if style.clips() {
                    self.scene.pop_layer();
                }
                if let Some((inner, max, offset)) = thumbs {
                    self.scrollbars(inner, max, offset);
                }
                if fade {
                    self.scene.pop_layer();
                }
            }
            Some(Ctx::Text { owner, runs, align }) => {
                let layout = text_layout(self.fcx, self.lcx, runs, Some(l.size.width + 0.5), align);
                draw_text(self.scene, self.transform * Affine::translate(pos.to_vec2()), &layout);
                self.hits.push(HitBox { id: *owner, rect, clip });
            }
            Some(Ctx::Input(id)) => {
                let style = &frame.styles[id];
                self.box_decoration(style, rect, false);
                self.hits.push(HitBox { id: *id, rect, clip });
                let content = Point::new(
                    pos.x + (l.padding.left + l.border.left) as f64,
                    pos.y + (l.padding.top + l.border.top) as f64,
                );
                let value = self.dom.get(*id).map(|n| n.value.clone()).unwrap_or_default();
                let placeholder = match &self.dom.get(*id).map(|n| &n.kind) {
                    Some(void_dom::NodeKind::Element { attrs, .. }) => attrs.get("placeholder").cloned(),
                    _ => None,
                };
                let mut s = style.clone();
                let text = if value.is_empty() {
                    s.color = Rgba { a: 0.45, ..s.color };
                    placeholder.unwrap_or_else(|| " ".into())
                } else {
                    value.clone()
                };
                let layout = text_layout(self.fcx, self.lcx, &[TextRun { text, style: s }], None, "start");
                let focused = self.dom.focus == Some(*id);
                if focused && self.dom.get(*id).is_some_and(|n| n.selected) && !value.is_empty() {
                    let h = style.font_size as f64 * style.line_height as f64;
                    let sel = Rect::new(content.x, content.y, content.x + layout.width() as f64, content.y + h);
                    self.scene.fill(Fill::NonZero, self.transform, Color::new([0.38, 0.42, 1.0, 0.45]), None, &sel);
                }
                draw_text(self.scene, self.transform * Affine::translate(content.to_vec2()), &layout);
                if focused && self.caret_on {
                    let x = if value.is_empty() { 0.0 } else { layout.width() as f64 } + content.x + 1.0;
                    let h = style.font_size as f64 * style.line_height as f64;
                    let caret = Rect::new(x, content.y, x + 1.5, content.y + h);
                    self.scene.fill(Fill::NonZero, self.transform, color(style.color), None, &caret);
                }
            }
            None => {}
        }
    }

    /// Overlay scrollbar thumbs: no track, never take layout space.
    fn scrollbars(&mut self, inner: Rect, max: Vec2, offset: Vec2) {
        let thumb = Color::new([0.5, 0.5, 0.55, 0.55]);
        let t = self.transform;
        // Too small to hold a thumb: skip rather than draw an inverted rect.
        if inner.width() < MIN_THUMB / 2.0 || inner.height() < MIN_THUMB / 2.0 {
            return;
        }
        if max.y > 0.0 {
            let view = inner.height();
            let len = (view * view / (view + max.y)).max(MIN_THUMB).min(view);
            let y = inner.y0 + (view - len) * offset.y / max.y;
            let r = Rect::new(inner.x1 - 7.0, y + 2.0, inner.x1 - 2.0, y + len - 2.0);
            self.scene.fill(Fill::NonZero, t, thumb, None, &RoundedRect::from_rect(r, 2.5));
        }
        if max.x > 0.0 {
            let view = inner.width();
            let len = (view * view / (view + max.x)).max(MIN_THUMB).min(view);
            let x = inner.x0 + (view - len) * offset.x / max.x;
            let r = Rect::new(x + 2.0, inner.y1 - 7.0, x + len - 2.0, inner.y1 - 2.0);
            self.scene.fill(Fill::NonZero, t, thumb, None, &RoundedRect::from_rect(r, 2.5));
        }
    }

    /// Shadow, background and border. With `fade`, also opens the opacity layer
    /// (the caller pops it after painting children).
    fn box_decoration(&mut self, s: &Style, rect: Rect, fade: bool) {
        let t = self.transform;
        let radius = radius(s, rect);
        let shape = BoxShape::new(rect, radius, s.squircle);
        if let Some(sh) = s.shadow {
            let r = rect + Vec2::new(sh.x as f64, sh.y as f64);
            self.scene.draw_blurred_rounded_rect(t, r, color(sh.color), radius, (sh.blur as f64 / 2.0).max(0.1));
        }
        if fade {
            self.scene.push_layer(Fill::NonZero, BlendMode::default(), s.opacity, t, &EVERYWHERE);
        }
        match &s.background {
            Some(Background::Color(c)) => self.scene.fill(Fill::NonZero, t, color(*c), None, &shape.path()),
            Some(Background::Linear { angle_deg, stops }) => {
                let a = (*angle_deg as f64).to_radians();
                let (dx, dy) = (a.sin(), -a.cos());
                let half = (rect.width() * dx.abs() + rect.height() * dy.abs()) / 2.0;
                let c = rect.center();
                let start = Point::new(c.x - dx * half, c.y - dy * half);
                let end = Point::new(c.x + dx * half, c.y + dy * half);
                let colors: Vec<Color> = stops.iter().map(|c| color(*c)).collect();
                let g = Gradient::new_linear(start, end).with_stops(colors.as_slice());
                self.scene.fill(Fill::NonZero, t, &g, None, &shape.path());
            }
            None => {}
        }
        if s.border_width > 0.0 {
            let bw = s.border_width as f64;
            let inner = BoxShape::new(rect.inset(-bw / 2.0), (radius - bw / 2.0).max(0.0), s.squircle);
            self.scene.stroke(&Stroke::new(bw), t, color(s.border_color), None, &inner.path());
        }
    }
}

fn radius(s: &Style, rect: Rect) -> f64 {
    (s.radius as f64).min(rect.width() / 2.0).min(rect.height() / 2.0)
}

/// A rounded rect, or an Apple-style continuous-corner squircle (`border-radius: continuous`).
struct BoxShape {
    rect: Rect,
    radius: f64,
    squircle: bool,
}

impl BoxShape {
    fn new(rect: Rect, radius: f64, squircle: bool) -> Self {
        BoxShape { rect, radius, squircle }
    }

    fn path(&self) -> BezPath {
        if !self.squircle || self.radius <= 0.0 {
            return RoundedRect::from_rect(self.rect, self.radius).to_path(0.1);
        }
        // Continuous corners: the curve starts ~1.28r from the corner and
        // uses flatter handles, so curvature ramps up smoothly.
        let r = &self.rect;
        let k = (self.radius * 1.28).min(r.width() / 2.0).min(r.height() / 2.0);
        let h = k * 0.64;
        let mut p = BezPath::new();
        p.move_to((r.x0 + k, r.y0));
        p.line_to((r.x1 - k, r.y0));
        p.curve_to((r.x1 - k + h, r.y0), (r.x1, r.y0 + k - h), (r.x1, r.y0 + k));
        p.line_to((r.x1, r.y1 - k));
        p.curve_to((r.x1, r.y1 - k + h), (r.x1 - k + h, r.y1), (r.x1 - k, r.y1));
        p.line_to((r.x0 + k, r.y1));
        p.curve_to((r.x0 + k - h, r.y1), (r.x0, r.y1 - k + h), (r.x0, r.y1 - k));
        p.line_to((r.x0, r.y0 + k));
        p.curve_to((r.x0, r.y0 + k - h), (r.x0 + k - h, r.y0), (r.x0 + k, r.y0));
        p.close_path();
        p
    }
}

pub fn draw_text(scene: &mut Scene, transform: Affine, layout: &Layout<TextBrush>) {
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else { continue };
            let mut x = glyph_run.offset();
            let y = glyph_run.baseline();
            let run = glyph_run.run();
            let font = run.font();
            let font_size = run.font_size();
            let synthesis = run.synthesis();
            let glyph_xform = synthesis.skew().map(|angle| Affine::skew(angle.to_radians().tan() as f64, 0.0));
            let b = glyph_run.style().brush.0;
            let coords: Vec<i16> = run.normalized_coords().iter().map(|c| c.to_bits()).collect();
            scene
                .draw_glyphs(font)
                .brush(Color::new(b))
                .transform(transform)
                .glyph_transform(glyph_xform)
                .font_size(font_size)
                .normalized_coords(&coords)
                .draw(
                    Fill::NonZero,
                    glyph_run.glyphs().map(|g| {
                        let gx = x + g.x;
                        let gy = y - g.y;
                        x += g.advance;
                        vello::Glyph { id: g.id as _, x: gx, y: gy }
                    }),
                );
        }
    }
}

/// Paint a fullscreen error overlay (compile / runtime errors).
pub fn error_overlay(
    scene: &mut Scene,
    fcx: &mut FontContext,
    lcx: &mut LayoutContext<TextBrush>,
    transform: Affine,
    size: (f32, f32),
    message: &str,
) {
    let bg = Rect::new(0.0, 0.0, size.0 as f64, size.1 as f64);
    scene.fill(Fill::NonZero, transform, Color::new([0.09, 0.02, 0.03, 0.96]), None, &bg);
    let mut title = Style::root();
    title.color = Rgba::rgb8(255, 99, 99);
    title.font_size = 20.0;
    title.font_weight = 700.0;
    let mut body = Style::root();
    body.color = Rgba::rgb8(255, 220, 220);
    body.font_size = 14.0;
    body.font_family = "Cascadia Code, Consolas, monospace".into();
    let t = text_layout(fcx, lcx, &[TextRun { text: "Void — error".into(), style: title }], None, "start");
    draw_text(scene, transform * Affine::translate((32.0, 28.0)), &t);
    let b = text_layout(fcx, lcx, &[TextRun { text: message.into(), style: body }], Some(size.0 - 64.0), "start");
    draw_text(scene, transform * Affine::translate((32.0, 70.0)), &b);
}
