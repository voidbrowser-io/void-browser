use parley::{FontContext, LayoutContext};
use void_css::{cascade, Stylesheet, UA_CSS};
use void_dom::{Dom, NodeId, ROOT};
use void_layout::{build, Ctx, Frame};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Box2 {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// Lay out `dom` with `css` in a 800×600 viewport.
fn layout(dom: &Dom, css: &str) -> Frame {
    let mut sheet = Stylesheet::parse(UA_CSS);
    sheet.append(css);
    let styles = cascade(dom, &sheet);
    build(dom, styles, (800.0, 600.0), &mut FontContext::new(), &mut LayoutContext::new())
}

/// Absolute box of the element `id`.
fn rect(frame: &Frame, id: NodeId) -> Option<Box2> {
    fn walk(f: &Frame, n: taffy::NodeId, ox: f32, oy: f32, id: NodeId) -> Option<Box2> {
        let l = f.tree.layout(n).ok()?;
        let (x, y) = (ox + l.location.x, oy + l.location.y);
        if matches!(f.tree.get_node_context(n), Some(Ctx::Element(e) | Ctx::Input(e)) if *e == id) {
            return Some(Box2 { x, y, w: l.size.width, h: l.size.height });
        }
        f.tree.children(n).ok()?.into_iter().find_map(|c| walk(f, c, x, y, id))
    }
    walk(frame, frame.root, 0.0, 0.0, id)
}

fn text_leaves(frame: &Frame) -> Vec<(String, f32, f32)> {
    let mut out = Vec::new();
    fn walk(f: &Frame, n: taffy::NodeId, out: &mut Vec<(String, f32, f32)>) {
        let l = f.tree.layout(n).unwrap();
        if let Some(Ctx::Text { runs, .. }) = f.tree.get_node_context(n) {
            out.push((runs.iter().map(|r| r.text.as_str()).collect(), l.size.width, l.size.height));
        }
        for c in f.tree.children(n).unwrap() {
            walk(f, c, out);
        }
    }
    walk(frame, frame.root, &mut out);
    out
}

fn el(dom: &mut Dom, parent: NodeId, tag: &str, class: &str) -> NodeId {
    let id = dom.create_element(tag);
    if !class.is_empty() {
        dom.set_attr(id, "class", class);
    }
    dom.insert(parent, id, None);
    id
}

fn text(dom: &mut Dom, parent: NodeId, s: &str) -> NodeId {
    let id = dom.create_text(s);
    dom.insert(parent, id, None);
    id
}

#[test]
fn root_fills_viewport() {
    let dom = Dom::new();
    let f = layout(&dom, "");
    assert_eq!(rect(&f, ROOT), Some(Box2 { x: 0.0, y: 0.0, w: 800.0, h: 600.0 }));
}

#[test]
fn flex_row_with_gap_and_grow() {
    let mut dom = Dom::new();
    let row = el(&mut dom, ROOT, "div", "row");
    let a = el(&mut dom, row, "div", "a");
    let b = el(&mut dom, row, "div", "b");
    let f = layout(
        &dom,
        ".row { display: flex; gap: 10px; padding: 20px; width: 500px }
         .a { width: 100px; height: 50px }
         .b { flex-grow: 1; height: 50px }",
    );
    assert_eq!(rect(&f, a), Some(Box2 { x: 20.0, y: 20.0, w: 100.0, h: 50.0 }));
    // 500 - 2*20 padding - 100 - 10 gap = 350
    assert_eq!(rect(&f, b), Some(Box2 { x: 130.0, y: 20.0, w: 350.0, h: 50.0 }));
}

#[test]
fn border_box_is_the_only_box_model() {
    let mut dom = Dom::new();
    let a = el(&mut dom, ROOT, "div", "a");
    let f = layout(&dom, ".a { width: 100px; padding: 10px; border: 5px solid red }");
    assert_eq!(rect(&f, a).unwrap().w, 100.0);
}

#[test]
fn flex_centering() {
    let mut dom = Dom::new();
    let wrap = el(&mut dom, ROOT, "div", "wrap");
    let dot = el(&mut dom, wrap, "div", "dot");
    let f = layout(
        &dom,
        ".wrap { display: flex; justify-content: center; align-items: center; width: 200px; height: 100px }
         .dot { width: 20px; height: 20px }",
    );
    assert_eq!(rect(&f, dot), Some(Box2 { x: 90.0, y: 40.0, w: 20.0, h: 20.0 }));
}

#[test]
fn grid_columns_with_fr_and_px() {
    let mut dom = Dom::new();
    let grid = el(&mut dom, ROOT, "div", "grid");
    let cells: Vec<NodeId> = (0..4).map(|_| el(&mut dom, grid, "div", "cell")).collect();
    let f = layout(
        &dom,
        ".grid { display: grid; grid-template-columns: 100px repeat(2, 1fr); gap: 10px; width: 400px }
         .cell { height: 30px }",
    );
    // 400 - 100 - 2*10 = 280 → 140 per fr
    assert_eq!(rect(&f, cells[0]).unwrap().w, 100.0);
    assert_eq!(rect(&f, cells[1]), Some(Box2 { x: 110.0, y: 0.0, w: 140.0, h: 30.0 }));
    assert_eq!(rect(&f, cells[3]).unwrap().y, 40.0, "fourth cell wraps to row two");
}

#[test]
fn block_children_stack_vertically() {
    let mut dom = Dom::new();
    let a = el(&mut dom, ROOT, "div", "x");
    let b = el(&mut dom, ROOT, "div", "x");
    let f = layout(&dom, ".x { height: 25px }");
    assert_eq!(rect(&f, a).unwrap().y, 0.0);
    assert_eq!(rect(&f, b).unwrap().y, 25.0);
    assert_eq!(rect(&f, b).unwrap().w, 800.0);
}

#[test]
fn display_none_removes_box() {
    let mut dom = Dom::new();
    let a = el(&mut dom, ROOT, "div", "gone");
    let f = layout(&dom, ".gone { display: none }");
    assert_eq!(rect(&f, a), None);
}

#[test]
fn inline_runs_merge_into_one_paragraph() {
    let mut dom = Dom::new();
    let p = el(&mut dom, ROOT, "p", "");
    text(&mut dom, p, "Hello ");
    let b = el(&mut dom, p, "b", "");
    text(&mut dom, b, "bold");
    text(&mut dom, p, "   world");
    let f = layout(&dom, "");
    let leaves = text_leaves(&f);
    assert_eq!(leaves.len(), 1);
    assert_eq!(leaves[0].0, "Hello bold world", "whitespace collapses");
    assert!(leaves[0].1 > 50.0 && leaves[0].2 > 10.0, "text has a real measured size: {:?}", leaves[0]);
}

#[test]
fn flex_children_are_blockified() {
    let mut dom = Dom::new();
    let row = el(&mut dom, ROOT, "div", "row");
    let a = el(&mut dom, row, "span", "");
    text(&mut dom, a, "one");
    let b = el(&mut dom, row, "span", "");
    text(&mut dom, b, "two");
    let f = layout(&dom, ".row { display: flex; gap: 8px }");
    let (ra, rb) = (rect(&f, a).unwrap(), rect(&f, b).unwrap());
    assert_eq!(text_leaves(&f).len(), 2, "each span is its own flex item");
    assert!(rb.x >= ra.x + ra.w + 8.0 - 0.5);
}

#[test]
fn text_wraps_unless_nowrap() {
    let long = "lorem ipsum dolor sit amet ".repeat(8);
    let mut dom = Dom::new();
    let a = el(&mut dom, ROOT, "div", "box");
    text(&mut dom, a, &long);
    let wrapped = layout(&dom, ".box { width: 150px }");
    let nowrap = layout(&dom, ".box { width: 150px; white-space: nowrap }");
    let (hw, hn) = (text_leaves(&wrapped)[0].2, text_leaves(&nowrap)[0].2);
    assert!(hw > hn * 3.0, "wrapped text is many lines tall ({hw} vs {hn})");
}

#[test]
fn input_gets_a_line_box_plus_padding() {
    let mut dom = Dom::new();
    let input = el(&mut dom, ROOT, "input", "");
    let f = layout(&dom, "input { padding: 10px; border: 0; font-size: 20px; line-height: 1 }");
    let r = rect(&f, input).unwrap();
    assert!(r.h >= 40.0 - 0.5 && r.h < 50.0, "20px line + 2×10px padding, got {}", r.h);
}
