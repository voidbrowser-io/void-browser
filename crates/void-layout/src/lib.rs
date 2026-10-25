//! Layout: Void styles → Taffy (flex / grid / block) with Parley text leaves.

use void_css::{Cascade, Display, Len, Style, Track};
use void_dom::{Dom, NodeId, NodeKind, ROOT};
use parley::{FontContext, Layout, LayoutContext};
use std::collections::HashMap;
use taffy::prelude::*;

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct TextBrush(pub [f32; 4]);

#[derive(Clone)]
pub struct TextRun {
    pub text: String,
    pub style: Style,
}

pub enum Ctx {
    Element(NodeId),
    /// Anonymous inline formatting context owned by `owner`.
    Text { owner: NodeId, runs: Vec<TextRun>, align: String },
    Input(NodeId),
}

pub struct Frame {
    pub tree: TaffyTree<Ctx>,
    pub root: taffy::NodeId,
    pub styles: HashMap<NodeId, Style>,
}

pub fn build(
    dom: &Dom,
    cascade: Cascade,
    viewport: (f32, f32),
    fcx: &mut FontContext,
    lcx: &mut LayoutContext<TextBrush>,
) -> Frame {
    let mut tree: TaffyTree<Ctx> = TaffyTree::new();
    let styles = cascade.styles;
    let root = build_node(dom, &styles, ROOT, &mut tree).expect("root");
    tree.compute_layout_with_measure(
        root,
        Size { width: AvailableSpace::Definite(viewport.0), height: AvailableSpace::Definite(viewport.1) },
        |input, _id, ctx, style| {
            taffy::compute_leaf_layout(input, style, |_, _| 0.0, |known, available| {
                measure(known, available, ctx, dom, &styles, fcx, lcx)
            })
        },
    )
    .ok();
    Frame { tree, root, styles }
}

fn build_node(dom: &Dom, styles: &HashMap<NodeId, Style>, id: NodeId, tree: &mut TaffyTree<Ctx>) -> Option<taffy::NodeId> {
    if matches!(dom.get(id)?.kind, NodeKind::Text(_)) {
        return None;
    }
    let node = dom.get(id)?;
    let style = styles.get(&id)?;
    if style.display == Display::None {
        return None;
    }
    let mut ts = to_taffy(style);
    if id == ROOT {
        ts.size = Size { width: percent(1.0), height: percent(1.0) };
    }
    if node.tag() == Some("input") {
        return tree.new_leaf_with_context(ts, Ctx::Input(id)).ok();
    }

    let mut kids = Vec::new();
    let mut runs: Vec<TextRun> = Vec::new();
    let flush = |runs: &mut Vec<TextRun>, kids: &mut Vec<taffy::NodeId>, tree: &mut TaffyTree<Ctx>| {
        if runs.iter().any(|r| !r.text.trim().is_empty()) {
            let leaf = tree
                .new_leaf_with_context(taffy::Style::default(), Ctx::Text { owner: id, runs: std::mem::take(runs), align: style.text_align.clone() })
                .unwrap();
            kids.push(leaf);
        }
        runs.clear();
    };
    let blockify = matches!(style.display, Display::Flex | Display::Grid);
    for &c in &node.children {
        let is_text = matches!(dom.get(c).map(|n| &n.kind), Some(NodeKind::Text(_)));
        if is_inline(dom, styles, c) && (is_text || !blockify) {
            collect_runs(dom, styles, c, style, &mut runs);
        } else {
            flush(&mut runs, &mut kids, tree);
            if let Some(k) = build_node(dom, styles, c, tree) {
                kids.push(k);
            }
        }
    }
    flush(&mut runs, &mut kids, tree);
    let n = tree.new_with_children(ts, &kids).ok()?;
    tree.set_node_context(n, Some(Ctx::Element(id))).ok();
    Some(n)
}

fn is_inline(dom: &Dom, styles: &HashMap<NodeId, Style>, id: NodeId) -> bool {
    match dom.get(id).map(|n| &n.kind) {
        Some(NodeKind::Text(_)) => true,
        Some(NodeKind::Element { tag, .. }) => {
            tag != "input" && styles.get(&id).is_some_and(|s| s.display == Display::Inline)
        }
        None => false,
    }
}

fn collect_runs(dom: &Dom, styles: &HashMap<NodeId, Style>, id: NodeId, parent: &Style, runs: &mut Vec<TextRun>) {
    let Some(node) = dom.get(id) else { return };
    match &node.kind {
        NodeKind::Text(t) => {
            let collapsed = collapse_ws(t);
            if !collapsed.is_empty() {
                runs.push(TextRun { text: collapsed, style: parent.clone() });
            }
        }
        NodeKind::Element { .. } => {
            let Some(style) = styles.get(&id) else { return };
            for &c in &node.children {
                collect_runs(dom, styles, c, style, runs);
            }
        }
    }
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut ws = false;
    for ch in s.chars() {
        if ch.is_whitespace() && ch != '\u{a0}' {
            if !ws {
                out.push(' ');
            }
            ws = true;
        } else {
            out.push(ch);
            ws = false;
        }
    }
    out
}

fn measure(
    known: Size<Option<f32>>,
    available: Size<AvailableSpace>,
    ctx: Option<&mut Ctx>,
    dom: &Dom,
    styles: &HashMap<NodeId, Style>,
    fcx: &mut FontContext,
    lcx: &mut LayoutContext<TextBrush>,
) -> Size<f32> {
    let max_w = known.width.or(match available.width {
        AvailableSpace::Definite(w) => Some(w),
        AvailableSpace::MinContent => Some(0.0),
        AvailableSpace::MaxContent => None,
    });
    match ctx {
        Some(Ctx::Text { runs, align, .. }) => {
            let layout = text_layout(fcx, lcx, runs, max_w, align);
            Size { width: known.width.unwrap_or(layout.width().ceil()), height: known.height.unwrap_or(layout.height().ceil()) }
        }
        Some(Ctx::Input(id)) => {
            let style = &styles[id];
            let value = dom.get(*id).map(|n| n.value.clone()).unwrap_or_default();
            let runs = [TextRun { text: if value.is_empty() { " ".into() } else { value }, style: style.clone() }];
            let layout = text_layout(fcx, lcx, &runs, None, "start");
            Size { width: known.width.unwrap_or(layout.width().ceil()), height: known.height.unwrap_or(layout.height().ceil()) }
        }
        _ => Size { width: known.width.unwrap_or(0.0), height: known.height.unwrap_or(0.0) },
    }
}

pub fn text_layout(
    fcx: &mut FontContext,
    lcx: &mut LayoutContext<TextBrush>,
    runs: &[TextRun],
    max_width: Option<f32>,
    align: &str,
) -> Layout<TextBrush> {
    use parley::style::{FontFamily, FontStyle, FontWeight, StyleProperty};
    let mut full = String::new();
    let mut ranges = Vec::new();
    for r in runs {
        let start = full.len();
        full.push_str(&r.text);
        ranges.push(start..full.len());
    }
    let text = full.trim_end();
    let first = &runs[0].style;
    let mut b = lcx.ranged_builder(fcx, text, 1.0, true);
    b.push_default(StyleProperty::LineHeight(parley::style::LineHeight::FontSizeRelative(first.line_height)));
    for (r, range) in runs.iter().zip(ranges) {
        let range = range.start.min(text.len())..range.end.min(text.len());
        if range.is_empty() {
            continue;
        }
        let s = &r.style;
        b.push(StyleProperty::FontSize(s.font_size), range.clone());
        b.push(StyleProperty::FontWeight(FontWeight::new(s.font_weight)), range.clone());
        b.push(StyleProperty::FontFamily(FontFamily::Source(s.font_family.to_string().into())), range.clone());
        if s.italic {
            b.push(StyleProperty::FontStyle(FontStyle::Italic), range.clone());
        }
        if s.letter_spacing != 0.0 {
            b.push(StyleProperty::LetterSpacing(s.letter_spacing), range.clone());
        }
        let c = s.color;
        b.push(StyleProperty::Brush(TextBrush([c.r, c.g, c.b, c.a])), range.clone());
    }
    let mut layout: Layout<TextBrush> = b.build(text);
    layout.break_all_lines(max_width);
    let alignment = match align {
        "center" => parley::Alignment::Center,
        "right" | "end" => parley::Alignment::End,
        "justify" => parley::Alignment::Justify,
        _ => parley::Alignment::Start,
    };
    layout.align(alignment, parley::AlignmentOptions::default());
    layout
}

fn dim(l: Len) -> Dimension {
    match l {
        Len::Auto => Dimension::auto(),
        Len::Px(p) => Dimension::length(p),
        Len::Pct(p) => Dimension::percent(p),
    }
}

fn lp(l: Len) -> LengthPercentage {
    match l {
        Len::Pct(p) => LengthPercentage::percent(p),
        Len::Px(p) => LengthPercentage::length(p),
        Len::Auto => LengthPercentage::length(0.0),
    }
}

fn lpa(l: Len) -> LengthPercentageAuto {
    match l {
        Len::Auto => LengthPercentageAuto::auto(),
        Len::Px(p) => LengthPercentageAuto::length(p),
        Len::Pct(p) => LengthPercentageAuto::percent(p),
    }
}

fn track(t: Track) -> TrackSizingFunction {
    match t {
        Track::Px(p) => length(p),
        Track::Pct(p) => percent(p),
        Track::Fr(f) => fr(f),
        Track::Auto => auto(),
        Track::MinContent => min_content(),
        Track::MaxContent => max_content(),
    }
}

fn align(v: &Option<String>) -> Option<AlignItems> {
    Some(match v.as_deref()? {
        "center" => AlignItems::CENTER,
        "flex-start" | "start" => AlignItems::START,
        "flex-end" | "end" => AlignItems::END,
        "baseline" => AlignItems::BASELINE,
        "stretch" => AlignItems::STRETCH,
        _ => return None,
    })
}

fn justify(v: &Option<String>) -> Option<JustifyContent> {
    Some(match v.as_deref()? {
        "center" => JustifyContent::CENTER,
        "flex-start" | "start" => JustifyContent::START,
        "flex-end" | "end" => JustifyContent::END,
        "space-between" => JustifyContent::SPACE_BETWEEN,
        "space-around" => JustifyContent::SPACE_AROUND,
        "space-evenly" => JustifyContent::SPACE_EVENLY,
        "stretch" => JustifyContent::STRETCH,
        _ => return None,
    })
}

fn to_taffy(s: &Style) -> taffy::Style {
    let rect = |v: [Len; 4], f: fn(Len) -> LengthPercentage| taffy::Rect { top: f(v[0]), right: f(v[1]), bottom: f(v[2]), left: f(v[3]) };
    taffy::Style {
        display: match s.display {
            Display::None => taffy::Display::None,
            Display::Flex => taffy::Display::Flex,
            Display::Grid => taffy::Display::Grid,
            Display::Block | Display::Inline => taffy::Display::Block,
        },
        box_sizing: taffy::BoxSizing::BorderBox,
        position: if s.absolute { Position::Absolute } else { Position::Relative },
        inset: taffy::Rect { top: lpa(s.inset[0]), right: lpa(s.inset[1]), bottom: lpa(s.inset[2]), left: lpa(s.inset[3]) },
        flex_direction: match s.flex_direction.as_str() {
            "column" => FlexDirection::Column,
            "row-reverse" => FlexDirection::RowReverse,
            "column-reverse" => FlexDirection::ColumnReverse,
            _ => FlexDirection::Row,
        },
        flex_wrap: if s.flex_wrap { FlexWrap::Wrap } else { FlexWrap::NoWrap },
        justify_content: justify(&s.justify_content).unwrap_or(JustifyContent::NORMAL),
        align_items: align(&s.align_items).unwrap_or(AlignItems::NORMAL),
        align_self: align(&s.align_self),
        flex_grow: s.flex_grow,
        flex_shrink: s.flex_shrink,
        flex_basis: dim(s.flex_basis),
        gap: Size { width: lp(s.gap.1), height: lp(s.gap.0) },
        grid_template_columns: s.grid_columns.iter().map(|t| track(*t).into()).collect(),
        grid_template_rows: s.grid_rows.iter().map(|t| track(*t).into()).collect(),
        grid_column: if s.grid_column_span > 1 {
            Line { start: GridPlacement::Auto, end: GridPlacement::Span(s.grid_column_span) }
        } else {
            Line { start: GridPlacement::Auto, end: GridPlacement::Auto }
        },
        size: Size { width: dim(s.width), height: dim(s.height) },
        min_size: Size { width: lpa(s.min_width), height: lpa(s.min_height) },
        max_size: Size { width: lpa(s.max_width), height: lpa(s.max_height) },
        aspect_ratio: s.aspect_ratio,
        padding: rect(s.padding, lp),
        margin: taffy::Rect { top: lpa(s.margin[0]), right: lpa(s.margin[1]), bottom: lpa(s.margin[2]), left: lpa(s.margin[3]) },
        border: taffy::Rect {
            top: LengthPercentage::length(s.border_width),
            right: LengthPercentage::length(s.border_width),
            bottom: LengthPercentage::length(s.border_width),
            left: LengthPercentage::length(s.border_width),
        },
        ..Default::default()
    }
}
