//! Void CSS: modern-only subset. Nesting, custom properties, `:has`-free
//! selectors, flex/grid, `oklch()`, gradients and shadows. No floats, no
//! tables, no quirks, no vendor prefixes, `box-sizing: border-box` always.

use void_dom::{Dom, NodeId, NodeKind, ROOT};
use std::collections::HashMap;
use std::rc::Rc;

// ───────────────────────────── Stylesheet ─────────────────────────────

#[derive(Debug, Clone)]
pub struct Rule {
    pub selector: Selector,
    pub decls: Rc<Vec<(String, String)>>,
    pub order: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

pub const UA_CSS: &str = r#"
void-root { display: block; width: 100%; height: 100%; }
div, section, main, header, footer, nav, article, aside, ul, ol, li, p, form,
h1, h2, h3, h4, h5, h6, pre { display: block; }
span, b, strong, i, em, code, a, small, label { display: inline; }
b, strong { font-weight: 700; }
i, em { font-style: italic; }
code, pre { font-family: Cascadia Code, Consolas, monospace; }
h1 { font-size: 2em; font-weight: 700; }
h2 { font-size: 1.5em; font-weight: 700; }
h3 { font-size: 1.17em; font-weight: 600; }
button {
  display: flex; align-items: center; justify-content: center;
  padding: 6px 14px; border-radius: 6px; background: #e4e4e7; color: #111;
}
input {
  display: block; padding: 6px 10px; border: 1px solid #a1a1aa;
  border-radius: 6px; background: #fff; color: #111; min-width: 160px;
}
"#;

impl Stylesheet {
    pub fn parse(src: &str) -> Stylesheet {
        let mut sheet = Stylesheet::default();
        sheet.append(src);
        sheet
    }

    pub fn append(&mut self, src: &str) {
        let src = strip_comments(src);
        let mut flat = Vec::new();
        parse_block(&src, &[], &mut flat);
        for (selectors, decls) in flat {
            let decls = Rc::new(decls);
            for sel in selectors {
                if let Some(selector) = Selector::parse(&sel) {
                    let order = self.rules.len();
                    self.rules.push(Rule { selector, decls: decls.clone(), order });
                }
            }
        }
    }
}

fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        rest = match rest[i + 2..].find("*/") {
            Some(j) => &rest[i + 2 + j + 2..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// Recursive block parser with native CSS nesting (`&`).
fn parse_block(text: &str, parents: &[String], out: &mut Vec<(Vec<String>, Vec<(String, String)>)>) {
    let slot = out.len();
    out.push((parents.to_vec(), Vec::new()));
    let bytes = text.as_bytes();
    let (mut i, mut start) = (0, 0);
    let mut decls = Vec::new();
    let mut paren = 0i32;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => paren += 1,
            b')' => paren -= 1,
            b';' if paren == 0 => {
                push_decl(&text[start..i], &mut decls);
                start = i + 1;
            }
            b'{' if paren == 0 => {
                let prelude = text[start..i].trim().to_string();
                let close = matching_brace(bytes, i);
                let inner = &text[i + 1..close];
                if prelude.starts_with("@media") || prelude.starts_with("@supports") {
                    // Void ships one window: media queries are not evaluated yet.
                } else if prelude.starts_with("@layer") {
                    parse_block(inner, parents, out);
                } else if !prelude.starts_with('@') {
                    let resolved = resolve_nested(&prelude, parents);
                    parse_block(inner, &resolved, out);
                }
                i = close;
                start = close + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < text.len() {
        push_decl(&text[start..], &mut decls);
    }
    out[slot].1 = decls;
}

fn matching_brace(bytes: &[u8], open: usize) -> usize {
    let mut depth = 0;
    for (j, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return j;
                }
            }
            _ => {}
        }
    }
    bytes.len()
}

fn push_decl(raw: &str, decls: &mut Vec<(String, String)>) {
    if let Some((k, v)) = raw.split_once(':') {
        let k = k.trim();
        let v = v.trim().trim_end_matches("!important").trim();
        if !k.is_empty() && !v.is_empty() {
            let key = if k.starts_with("--") { k.to_string() } else { k.to_ascii_lowercase() };
            decls.push((key, v.to_string()));
        }
    }
}

fn resolve_nested(prelude: &str, parents: &[String]) -> Vec<String> {
    let own: Vec<&str> = prelude.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if parents.is_empty() {
        return own.iter().map(|s| s.to_string()).collect();
    }
    let mut out = Vec::new();
    for p in parents {
        for s in &own {
            if s.contains('&') {
                out.push(s.replace('&', p));
            } else {
                out.push(format!("{p} {s}"));
            }
        }
    }
    out
}

// ───────────────────────────── Selectors ─────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct Compound {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    hover: bool,
    active: bool,
    focus: bool,
    root: bool,
    first: bool,
    last: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Comb {
    Descendant,
    Child,
}

#[derive(Debug, Clone)]
pub struct Selector {
    /// Rightmost compound first.
    parts: Vec<(Compound, Comb)>,
    pub specificity: (u32, u32, u32),
    pub uses_hover: bool,
}

impl Selector {
    pub fn parse(src: &str) -> Option<Selector> {
        let src = src.replace('>', " > ");
        let mut parts = Vec::new();
        let mut comb = Comb::Descendant;
        for tok in src.split_whitespace() {
            if tok == ">" {
                comb = Comb::Child;
                continue;
            }
            parts.push((parse_compound(tok)?, comb));
            comb = Comb::Descendant;
        }
        if parts.is_empty() {
            return None;
        }
        // Each compound stores the combinator that links it to the compound on its right.
        let n = parts.len();
        let mut linked = Vec::with_capacity(n);
        for i in 0..n {
            let comb = if i + 1 < n { parts[i + 1].1 } else { Comb::Descendant };
            linked.push((parts[i].0.clone(), comb));
        }
        linked.reverse();
        let mut spec = (0, 0, 0);
        let mut uses_hover = false;
        for (c, _) in &linked {
            spec.0 += c.id.is_some() as u32;
            spec.1 += c.classes.len() as u32 + c.hover as u32 + c.active as u32 + c.focus as u32 + c.root as u32;
            spec.1 += c.first as u32 + c.last as u32;
            spec.2 += c.tag.is_some() as u32;
            uses_hover |= c.hover || c.active;
        }
        Some(Selector { parts: linked, specificity: spec, uses_hover })
    }

    pub fn matches(&self, dom: &Dom, node: NodeId) -> bool {
        match_from(&self.parts, dom, node)
    }
}

fn parse_compound(tok: &str) -> Option<Compound> {
    let mut c = Compound::default();
    let mut chars = tok.char_indices().peekable();
    let read_ident = |start: usize, s: &str| -> (String, usize) {
        let end = s[start..]
            .find(|ch: char| !(ch.is_alphanumeric() || ch == '-' || ch == '_'))
            .map(|e| start + e)
            .unwrap_or(s.len());
        (s[start..end].to_string(), end)
    };
    let mut pos = 0;
    while pos < tok.len() {
        let ch = tok[pos..].chars().next()?;
        match ch {
            '*' => pos += 1,
            '.' => {
                let (id, end) = read_ident(pos + 1, tok);
                c.classes.push(id);
                pos = end;
            }
            '#' => {
                let (id, end) = read_ident(pos + 1, tok);
                c.id = Some(id);
                pos = end;
            }
            ':' => {
                let (name, end) = read_ident(pos + 1, tok);
                match name.as_str() {
                    "hover" => c.hover = true,
                    "active" => c.active = true,
                    "focus" | "focus-visible" | "focus-within" => c.focus = true,
                    "root" => c.root = true,
                    "first-child" => c.first = true,
                    "last-child" => c.last = true,
                    _ => return None,
                }
                pos = end;
            }
            _ => {
                let (tag, end) = read_ident(pos, tok);
                if tag.is_empty() {
                    return None;
                }
                c.tag = Some(tag.to_ascii_lowercase());
                pos = end;
            }
        }
    }
    let _ = chars.peek();
    Some(c)
}

fn match_compound(c: &Compound, dom: &Dom, id: NodeId) -> bool {
    let Some(node) = dom.get(id) else { return false };
    let NodeKind::Element { tag, attrs, classes } = &node.kind else { return false };
    if let Some(t) = &c.tag {
        if t != tag {
            return false;
        }
    }
    if let Some(want) = &c.id {
        if attrs.get("id") != Some(want) {
            return false;
        }
    }
    if !c.classes.iter().all(|k| classes.contains(k)) {
        return false;
    }
    if c.root && id != ROOT {
        return false;
    }
    if c.hover && !dom.hover.is_some_and(|h| dom.is_ancestor(id, h)) {
        return false;
    }
    if c.active && !dom.active.is_some_and(|h| dom.is_ancestor(id, h)) {
        return false;
    }
    if c.focus && dom.focus != Some(id) {
        return false;
    }
    if c.first || c.last {
        let siblings: Vec<NodeId> = node
            .parent
            .and_then(|p| dom.get(p))
            .map(|p| p.children.iter().copied().filter(|&s| dom.get(s).and_then(|n| n.tag()).is_some()).collect())
            .unwrap_or_default();
        if c.first && siblings.first() != Some(&id) {
            return false;
        }
        if c.last && siblings.last() != Some(&id) {
            return false;
        }
    }
    true
}

fn match_from(parts: &[(Compound, Comb)], dom: &Dom, id: NodeId) -> bool {
    let Some(((c, comb), rest)) = parts.split_first() else { return true };
    if !match_compound(c, dom, id) {
        return false;
    }
    if rest.is_empty() {
        return true;
    }
    let mut cur = dom.get(id).and_then(|n| n.parent);
    match comb {
        Comb::Child => cur.is_some_and(|p| match_from(rest, dom, p)),
        Comb::Descendant => {
            while let Some(p) = cur {
                if match_from(rest, dom, p) {
                    return true;
                }
                cur = dom.get(p).and_then(|n| n.parent);
            }
            false
        }
    }
}

// ───────────────────────────── Values ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const TRANSPARENT: Rgba = Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
    pub fn rgb8(r: u8, g: u8, b: u8) -> Rgba {
        Rgba { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Len {
    Auto,
    Px(f32),
    Pct(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Track {
    Px(f32),
    Pct(f32),
    Fr(f32),
    Auto,
    MinContent,
    MaxContent,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Display {
    None,
    Block,
    Flex,
    Grid,
    Inline,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Background {
    Color(Rgba),
    Linear { angle_deg: f32, stops: Vec<Rgba> },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub color: Rgba,
}

#[derive(Debug, Clone)]
pub struct Style {
    pub display: Display,
    pub absolute: bool,
    pub inset: [Len; 4],
    pub flex_direction: String,
    pub flex_wrap: bool,
    pub justify_content: Option<String>,
    pub align_items: Option<String>,
    pub align_self: Option<String>,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Len,
    pub gap: (Len, Len),
    pub grid_columns: Vec<Track>,
    pub grid_rows: Vec<Track>,
    pub grid_column_span: u16,
    pub width: Len,
    pub height: Len,
    pub min_width: Len,
    pub min_height: Len,
    pub max_width: Len,
    pub max_height: Len,
    pub aspect_ratio: Option<f32>,
    pub padding: [Len; 4],
    pub margin: [Len; 4],
    pub border_width: f32,
    pub border_color: Rgba,
    pub radius: f32,
    pub squircle: bool,
    pub background: Option<Background>,
    pub shadow: Option<Shadow>,
    pub opacity: f32,
    pub overflow_hidden: bool,
    // Inherited
    pub color: Rgba,
    pub font_size: f32,
    pub font_weight: f32,
    pub font_family: Rc<str>,
    pub italic: bool,
    pub line_height: f32,
    pub letter_spacing: f32,
    pub text_align: String,
    pub custom: Rc<HashMap<String, String>>,
}

impl Style {
    pub fn root() -> Style {
        Style {
            display: Display::Block,
            absolute: false,
            inset: [Len::Auto; 4],
            flex_direction: "row".into(),
            flex_wrap: false,
            justify_content: None,
            align_items: None,
            align_self: None,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Len::Auto,
            gap: (Len::Px(0.0), Len::Px(0.0)),
            grid_columns: Vec::new(),
            grid_rows: Vec::new(),
            grid_column_span: 1,
            width: Len::Auto,
            height: Len::Auto,
            min_width: Len::Auto,
            min_height: Len::Auto,
            max_width: Len::Auto,
            max_height: Len::Auto,
            aspect_ratio: None,
            padding: [Len::Px(0.0); 4],
            margin: [Len::Px(0.0); 4],
            border_width: 0.0,
            border_color: Rgba::BLACK,
            radius: 0.0,
            squircle: false,
            background: None,
            shadow: None,
            opacity: 1.0,
            overflow_hidden: false,
            color: Rgba::rgb8(17, 17, 17),
            font_size: 16.0,
            font_weight: 400.0,
            font_family: "Segoe UI Variable Text, Segoe UI, system-ui, sans-serif".into(),
            italic: false,
            line_height: 1.4,
            letter_spacing: 0.0,
            text_align: "start".into(),
            custom: Rc::new(HashMap::new()),
        }
    }

    fn inherit(parent: &Style) -> Style {
        let mut s = Style::root();
        s.display = Display::Inline;
        s.color = parent.color;
        s.font_size = parent.font_size;
        s.font_weight = parent.font_weight;
        s.font_family = parent.font_family.clone();
        s.italic = parent.italic;
        s.line_height = parent.line_height;
        s.letter_spacing = parent.letter_spacing;
        s.text_align = parent.text_align.clone();
        s.custom = parent.custom.clone();
        s
    }
}

// ───────────────────────────── Cascade ─────────────────────────────

pub struct Cascade {
    pub styles: HashMap<NodeId, Style>,
}

pub fn cascade(dom: &Dom, sheet: &Stylesheet) -> Cascade {
    let mut styles = HashMap::new();
    let root = Style::root();
    compute_node(dom, sheet, ROOT, &root, &mut styles);
    Cascade { styles }
}

fn compute_node(dom: &Dom, sheet: &Stylesheet, id: NodeId, parent: &Style, out: &mut HashMap<NodeId, Style>) {
    let Some(node) = dom.get(id) else { return };
    if node.tag().is_none() {
        return;
    }
    let mut matched: Vec<&Rule> = sheet.rules.iter().filter(|r| r.selector.matches(dom, id)).collect();
    matched.sort_by_key(|r| (r.selector.specificity, r.order));
    let mut decls: Vec<(String, String)> = Vec::new();
    for r in matched {
        decls.extend(r.decls.iter().cloned());
    }
    if !node.inline_style.is_empty() {
        for part in node.inline_style.split(';') {
            push_decl(part, &mut decls);
        }
    }
    let style = compute_style(parent, &decls);
    let children = node.children.clone();
    for c in children {
        compute_node(dom, sheet, c, &style, out);
    }
    out.insert(id, style);
}

fn compute_style(parent: &Style, decls: &[(String, String)]) -> Style {
    let mut s = Style::inherit(parent);
    // Pass 1: custom properties (inherited, resolved lazily).
    let customs: Vec<_> = decls.iter().filter(|(k, _)| k.starts_with("--")).collect();
    if !customs.is_empty() {
        let mut map = (*s.custom).clone();
        for (k, v) in customs {
            let v = resolve_vars(v, &map);
            map.insert(k.clone(), v);
        }
        s.custom = Rc::new(map);
    }
    // Pass 2: font-size first so `em` resolves.
    for (k, v) in decls {
        if k == "font-size" {
            let v = resolve_vars(v, &s.custom);
            if let Some(px) = length_px(&v, parent.font_size, parent.font_size) {
                s.font_size = px;
            }
        }
    }
    for (k, v) in decls {
        if k.starts_with("--") || k == "font-size" {
            continue;
        }
        let v = resolve_vars(v, &s.custom);
        apply(&mut s, k, v.trim());
    }
    s
}

fn resolve_vars(v: &str, vars: &HashMap<String, String>) -> String {
    let mut out = v.to_string();
    for _ in 0..8 {
        let Some(i) = out.find("var(") else { break };
        let close = matching_paren(&out, i + 3);
        let inner = &out[i + 4..close];
        let (name, fallback) = match split_top(inner, ',').as_slice() {
            [n] => (n.trim().to_string(), None),
            [n, rest @ ..] => (n.trim().to_string(), Some(rest.join(",").trim().to_string())),
            [] => break,
        };
        let value = vars.get(&name).cloned().or(fallback).unwrap_or_default();
        out = format!("{}{}{}", &out[..i], value, &out[close + 1..]);
    }
    out
}

fn matching_paren(s: &str, open: usize) -> usize {
    let mut depth = 0;
    for (j, b) in s.bytes().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return j;
                }
            }
            _ => {}
        }
    }
    s.len().saturating_sub(1)
}

/// Split on `sep` outside parentheses.
pub fn split_top(s: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && (ch == sep || (sep == ' ' && ch.is_whitespace())) {
            if !cur.trim().is_empty() {
                out.push(cur.trim().to_string());
            }
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

fn apply(s: &mut Style, k: &str, v: &str) {
    let fs = s.font_size;
    let len = |v: &str| parse_len(v, fs);
    match k {
        "display" => {
            s.display = match v {
                "none" => Display::None,
                "flex" | "inline-flex" => Display::Flex,
                "grid" | "inline-grid" => Display::Grid,
                "inline" => Display::Inline,
                _ => Display::Block,
            }
        }
        "position" => s.absolute = v == "absolute" || v == "fixed",
        "inset" => s.inset = sides(v, fs),
        "top" => s.inset[0] = len(v),
        "right" => s.inset[1] = len(v),
        "bottom" => s.inset[2] = len(v),
        "left" => s.inset[3] = len(v),
        "flex-direction" => s.flex_direction = v.to_string(),
        "flex-wrap" => s.flex_wrap = v == "wrap",
        "flex" => {
            let parts = split_top(v, ' ');
            match parts.as_slice() {
                [one] if one == "none" => (s.flex_grow, s.flex_shrink) = (0.0, 0.0),
                [one] if one == "auto" => (s.flex_grow, s.flex_shrink) = (1.0, 1.0),
                [one] => {
                    if let Ok(g) = one.parse() {
                        (s.flex_grow, s.flex_shrink, s.flex_basis) = (g, 1.0, Len::Pct(0.0));
                    } else {
                        s.flex_basis = len(one);
                    }
                }
                [g, sh, rest @ ..] => {
                    s.flex_grow = g.parse().unwrap_or(0.0);
                    s.flex_shrink = sh.parse().unwrap_or(1.0);
                    if let Some(b) = rest.first() {
                        s.flex_basis = len(b);
                    }
                }
                _ => {}
            }
        }
        "flex-grow" => s.flex_grow = v.parse().unwrap_or(0.0),
        "flex-shrink" => s.flex_shrink = v.parse().unwrap_or(1.0),
        "flex-basis" => s.flex_basis = len(v),
        "justify-content" => s.justify_content = Some(v.to_string()),
        "align-items" => s.align_items = Some(v.to_string()),
        "align-self" => s.align_self = Some(v.to_string()),
        "place-items" => s.align_items = Some(v.split_whitespace().next().unwrap_or(v).to_string()),
        "gap" => {
            let p = split_top(v, ' ');
            let r = len(&p[0]);
            s.gap = (r, p.get(1).map(|c| len(c)).unwrap_or(r));
        }
        "row-gap" => s.gap.0 = len(v),
        "column-gap" => s.gap.1 = len(v),
        "grid-template-columns" => s.grid_columns = tracks(v, fs),
        "grid-template-rows" => s.grid_rows = tracks(v, fs),
        "grid-column" => {
            if let Some(n) = v.strip_prefix("span").and_then(|n| n.trim().parse().ok()) {
                s.grid_column_span = n;
            }
        }
        "width" | "inline-size" => s.width = len(v),
        "height" | "block-size" => s.height = len(v),
        "min-width" => s.min_width = len(v),
        "min-height" => s.min_height = len(v),
        "max-width" => s.max_width = len(v),
        "max-height" => s.max_height = len(v),
        "aspect-ratio" => {
            s.aspect_ratio = match v.split_once('/') {
                Some((a, b)) => a.trim().parse::<f32>().ok().zip(b.trim().parse::<f32>().ok()).map(|(a, b)| a / b),
                None => v.parse().ok(),
            }
        }
        "padding" => s.padding = sides(v, fs),
        "padding-top" => s.padding[0] = len(v),
        "padding-right" => s.padding[1] = len(v),
        "padding-bottom" => s.padding[2] = len(v),
        "padding-left" => s.padding[3] = len(v),
        "padding-inline" => {
            let p = split_top(v, ' ');
            s.padding[3] = len(&p[0]);
            s.padding[1] = len(p.get(1).unwrap_or(&p[0]));
        }
        "padding-block" => {
            let p = split_top(v, ' ');
            s.padding[0] = len(&p[0]);
            s.padding[2] = len(p.get(1).unwrap_or(&p[0]));
        }
        "margin" => s.margin = sides(v, fs),
        "margin-top" => s.margin[0] = len(v),
        "margin-right" => s.margin[1] = len(v),
        "margin-bottom" => s.margin[2] = len(v),
        "margin-left" => s.margin[3] = len(v),
        "margin-inline" => {
            let p = split_top(v, ' ');
            s.margin[3] = len(&p[0]);
            s.margin[1] = len(p.get(1).unwrap_or(&p[0]));
        }
        "margin-block" => {
            let p = split_top(v, ' ');
            s.margin[0] = len(&p[0]);
            s.margin[2] = len(p.get(1).unwrap_or(&p[0]));
        }
        "border" => {
            if v == "none" || v == "0" {
                s.border_width = 0.0;
            }
            for part in split_top(v, ' ') {
                if let Some(px) = length_px(&part, fs, 0.0) {
                    s.border_width = px;
                } else if let Some(c) = parse_color(&part, s.color) {
                    s.border_color = c;
                }
            }
        }
        "border-width" => s.border_width = length_px(v, fs, 0.0).unwrap_or(0.0),
        "border-color" => s.border_color = parse_color(v, s.color).unwrap_or(s.border_color),
        "border-radius" => {
            if v == "continuous" {
                s.squircle = true;
            } else if let Some(px) = length_px(split_top(v, ' ').first().map(String::as_str).unwrap_or(v), fs, 0.0) {
                s.radius = px;
            }
        }
        "corner-shape" => s.squircle = v == "squircle" || v == "superellipse(2)",
        "background" | "background-color" | "background-image" => s.background = parse_background(v, s.color),
        "box-shadow" => s.shadow = parse_shadow(v, fs, s.color),
        "opacity" => s.opacity = v.parse().unwrap_or(1.0),
        "overflow" => s.overflow_hidden = v != "visible",
        "color" => s.color = parse_color(v, s.color).unwrap_or(s.color),
        "font-weight" => {
            s.font_weight = match v {
                "normal" => 400.0,
                "bold" => 700.0,
                _ => v.parse().unwrap_or(400.0),
            }
        }
        "font-family" => s.font_family = v.into(),
        "font-style" => s.italic = v == "italic",
        "line-height" => {
            s.line_height = if let Ok(n) = v.parse::<f32>() {
                n
            } else {
                length_px(v, fs, fs).map(|px| px / fs).unwrap_or(1.4)
            }
        }
        "letter-spacing" => s.letter_spacing = length_px(v, fs, 0.0).unwrap_or(0.0),
        "text-align" => s.text_align = v.to_string(),
        _ => {}
    }
}

fn sides(v: &str, fs: f32) -> [Len; 4] {
    let p: Vec<Len> = split_top(v, ' ').iter().map(|x| parse_len(x, fs)).collect();
    match p.as_slice() {
        [a] => [*a; 4],
        [a, b] => [*a, *b, *a, *b],
        [a, b, c] => [*a, *b, *c, *b],
        [a, b, c, d, ..] => [*a, *b, *c, *d],
        [] => [Len::Px(0.0); 4],
    }
}

pub fn parse_len(v: &str, fs: f32) -> Len {
    let v = v.trim();
    if v == "auto" {
        return Len::Auto;
    }
    if let Some(p) = v.strip_suffix('%') {
        return p.trim().parse::<f32>().map(|p| Len::Pct(p / 100.0)).unwrap_or(Len::Auto);
    }
    length_px(v, fs, fs).map(Len::Px).unwrap_or(Len::Auto)
}

pub fn length_px(v: &str, fs: f32, _parent_fs: f32) -> Option<f32> {
    let v = v.trim();
    if v == "0" {
        return Some(0.0);
    }
    for (suffix, mul) in [("px", 1.0), ("rem", 16.0), ("em", fs)] {
        if let Some(n) = v.strip_suffix(suffix) {
            return n.trim().parse::<f32>().ok().map(|n| n * mul);
        }
    }
    v.parse::<f32>().ok()
}

fn tracks(v: &str, fs: f32) -> Vec<Track> {
    let mut out = Vec::new();
    for part in split_top(v, ' ') {
        if let Some(inner) = part.strip_prefix("repeat(").and_then(|r| r.strip_suffix(')')) {
            let args = split_top(inner, ',');
            if let (Some(n), Some(t)) = (args.first().and_then(|n| n.parse::<usize>().ok()), args.get(1)) {
                let ts = tracks(t, fs);
                for _ in 0..n {
                    out.extend(ts.iter().copied());
                }
            }
            continue;
        }
        if let Some(inner) = part.strip_prefix("minmax(").and_then(|r| r.strip_suffix(')')) {
            // Simplified: use the max track.
            if let Some(max) = split_top(inner, ',').get(1) {
                out.extend(tracks(max, fs));
            }
            continue;
        }
        out.push(match part.as_str() {
            "auto" => Track::Auto,
            "min-content" => Track::MinContent,
            "max-content" => Track::MaxContent,
            p if p.ends_with("fr") => Track::Fr(p.trim_end_matches("fr").parse().unwrap_or(1.0)),
            p if p.ends_with('%') => Track::Pct(p.trim_end_matches('%').parse::<f32>().unwrap_or(0.0) / 100.0),
            p => Track::Px(length_px(p, fs, fs).unwrap_or(0.0)),
        });
    }
    out
}

fn parse_background(v: &str, current: Rgba) -> Option<Background> {
    if v == "none" || v == "transparent" {
        return None;
    }
    if let Some(inner) = v.strip_prefix("linear-gradient(").and_then(|r| r.strip_suffix(')')) {
        let args = split_top(inner, ',');
        let mut angle = 180.0;
        let mut stops = Vec::new();
        for (i, a) in args.iter().enumerate() {
            if i == 0 {
                if let Some(d) = a.strip_suffix("deg") {
                    angle = d.trim().parse().unwrap_or(180.0);
                    continue;
                }
                if let Some(dir) = a.strip_prefix("to ") {
                    angle = match dir.trim() {
                        "top" => 0.0,
                        "right" => 90.0,
                        "bottom" => 180.0,
                        "left" => 270.0,
                        "bottom right" | "right bottom" => 135.0,
                        "top right" | "right top" => 45.0,
                        "bottom left" | "left bottom" => 225.0,
                        _ => 315.0,
                    };
                    continue;
                }
            }
            // Ignore explicit stop positions — stops are evenly spaced.
            let color_part = split_top(a, ' ').into_iter().next().unwrap_or_default();
            if let Some(c) = parse_color(&color_part, current) {
                stops.push(c);
            }
        }
        if stops.len() >= 2 {
            return Some(Background::Linear { angle_deg: angle, stops });
        }
        return stops.first().map(|c| Background::Color(*c));
    }
    parse_color(v, current).map(Background::Color)
}

fn parse_shadow(v: &str, fs: f32, current: Rgba) -> Option<Shadow> {
    if v == "none" {
        return None;
    }
    let first = split_top(v, ',').into_iter().next()?;
    let mut nums = Vec::new();
    let mut color = Rgba { a: 0.25, ..Rgba::BLACK };
    for part in split_top(&first, ' ') {
        if let Some(px) = length_px(&part, fs, fs) {
            nums.push(px);
        } else if let Some(c) = parse_color(&part, current) {
            color = c;
        }
    }
    Some(Shadow { x: *nums.first()?, y: *nums.get(1)?, blur: nums.get(2).copied().unwrap_or(0.0), color })
}

pub fn parse_color(v: &str, current: Rgba) -> Option<Rgba> {
    let v = v.trim().to_ascii_lowercase();
    if let Some(hex) = v.strip_prefix('#') {
        let h = |s: &str| u8::from_str_radix(s, 16).ok();
        let d = |c: &str| h(&c.repeat(2));
        return match hex.len() {
            3 => Some(Rgba::rgb8(d(&hex[0..1])?, d(&hex[1..2])?, d(&hex[2..3])?)),
            4 => Some(Rgba { a: d(&hex[3..4])? as f32 / 255.0, ..Rgba::rgb8(d(&hex[0..1])?, d(&hex[1..2])?, d(&hex[2..3])?) }),
            6 => Some(Rgba::rgb8(h(&hex[0..2])?, h(&hex[2..4])?, h(&hex[4..6])?)),
            8 => Some(Rgba { a: h(&hex[6..8])? as f32 / 255.0, ..Rgba::rgb8(h(&hex[0..2])?, h(&hex[2..4])?, h(&hex[4..6])?) }),
            _ => None,
        };
    }
    if let Some((name, args)) = v.split_once('(') {
        let args = args.trim_end_matches(')').replace(',', " ").replace('/', " / ");
        let parts: Vec<&str> = args.split_whitespace().collect();
        let (chan, alpha) = match parts.iter().position(|p| *p == "/") {
            Some(i) => (&parts[..i], parts.get(i + 1).copied()),
            None if parts.len() == 4 => (&parts[..3], Some(parts[3])),
            None => (&parts[..], None),
        };
        let num = |s: &str, scale: f32| -> f32 {
            match s.strip_suffix('%') {
                Some(p) => p.parse::<f32>().unwrap_or(0.0) / 100.0 * scale,
                None => s.trim_end_matches("deg").parse::<f32>().unwrap_or(0.0),
            }
        };
        let a = alpha.map(|a| num(a, 1.0)).unwrap_or(1.0);
        if chan.len() < 3 {
            return None;
        }
        return match name.trim() {
            "rgb" | "rgba" => Some(Rgba {
                r: num(chan[0], 255.0) / 255.0,
                g: num(chan[1], 255.0) / 255.0,
                b: num(chan[2], 255.0) / 255.0,
                a,
            }),
            "oklch" => Some(oklch(num(chan[0], 1.0), num(chan[1], 0.4), num(chan[2], 1.0), a)),
            "hsl" | "hsla" => Some(hsl(num(chan[0], 1.0), num(chan[1], 1.0), num(chan[2], 1.0), a)),
            _ => None,
        };
    }
    Some(match v.as_str() {
        "currentcolor" => current,
        "transparent" => Rgba::TRANSPARENT,
        "white" => Rgba::rgb8(255, 255, 255),
        "black" => Rgba::BLACK,
        "red" => Rgba::rgb8(239, 68, 68),
        "green" => Rgba::rgb8(34, 197, 94),
        "blue" => Rgba::rgb8(59, 130, 246),
        "gray" | "grey" => Rgba::rgb8(128, 128, 128),
        _ => return None,
    })
}

fn oklch(l: f32, c: f32, h: f32, alpha: f32) -> Rgba {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_.powi(3), m_.powi(3), s_.powi(3));
    let lin = [
        4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3,
        -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3,
        -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3,
    ];
    let enc = |x: f32| {
        let x = x.clamp(0.0, 1.0);
        if x <= 0.003_130_8 { 12.92 * x } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 }
    };
    Rgba { r: enc(lin[0]), g: enc(lin[1]), b: enc(lin[2]), a: alpha }
}

fn hsl(h: f32, s: f32, l: f32, a: f32) -> Rgba {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    Rgba { r: r + m, g: g + m, b: b + m, a }
}

/// True if any rule depends on `:hover`/`:active`, so pointer moves need a restyle.
pub fn uses_hover(sheet: &Stylesheet) -> bool {
    sheet.rules.iter().any(|r| r.selector.uses_hover)
}
