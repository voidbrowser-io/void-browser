use parley::{FontContext, LayoutContext};
use vello::kurbo::{Affine, Point, Vec2};
use vello::Scene;
use void_css::{cascade, Stylesheet, UA_CSS};
use void_dom::{Dom, NodeId, ROOT};
use void_paint::{HitBox, Painter, Scroller};

struct Painted {
    hits: Vec<HitBox>,
    scrollers: Vec<Scroller>,
    /// Layers left open at the end of the scene (must be 0).
    open_layers: u32,
}

fn paint(dom: &Dom, css: &str) -> Painted {
    let mut sheet = Stylesheet::parse(UA_CSS);
    sheet.append(css);
    let (mut fcx, mut lcx) = (FontContext::new(), LayoutContext::new());
    let frame = void_layout::build(dom, cascade(dom, &sheet), (800.0, 600.0), &mut fcx, &mut lcx);
    let mut scene = Scene::new();
    let mut p = Painter {
        scene: &mut scene,
        fcx: &mut fcx,
        lcx: &mut lcx,
        dom,
        transform: Affine::IDENTITY,
        hits: Vec::new(),
        scrollers: Vec::new(),
        caret_on: false,
    };
    p.paint(&frame);
    let (hits, scrollers) = (p.hits, p.scrollers);
    Painted { hits, scrollers, open_layers: scene.encoding().n_open_clips }
}

fn el(dom: &mut Dom, parent: NodeId, class: &str) -> NodeId {
    let id = dom.create_element("div");
    dom.set_attr(id, "class", class);
    dom.insert(parent, id, None);
    id
}

fn hit(p: &Painted, id: NodeId) -> &HitBox {
    p.hits.iter().find(|h| h.id == id).unwrap()
}

const CSS: &str = ".box { width: 200px; height: 100px; overflow: auto } .tall { height: 400px }";

/// root > .box (100px tall scroller) > .tall (400px)
fn scroll_box() -> (Dom, NodeId, NodeId) {
    let mut dom = Dom::new();
    let b = el(&mut dom, ROOT, "box");
    let tall = el(&mut dom, b, "tall");
    (dom, b, tall)
}

#[test]
fn scroll_container_reports_its_range() {
    let (dom, b, _) = scroll_box();
    let p = paint(&dom, CSS);
    let sc = p.scrollers.iter().find(|s| s.id == b).expect("box is a scroller");
    assert_eq!(sc.max, Vec2::new(0.0, 300.0));
    assert_eq!(sc.offset, Vec2::ZERO);
    assert!(p.scrollers.iter().all(|s| s.id != ROOT), "the page itself fits, so it does not scroll");
}

#[test]
fn scroll_offset_moves_children_and_clips_hits() {
    let (mut dom, b, tall) = scroll_box();
    dom.set_scroll(b, (0.0, 50.0));
    let p = paint(&dom, CSS);
    let (bh, th) = (hit(&p, b), hit(&p, tall));
    assert_eq!(th.rect.y0, bh.rect.y0 - 50.0, "content moved up by the offset");
    assert_eq!(th.clip, bh.rect, "children are clipped to the scroller");
    assert!(th.contains(Point::new(10.0, 10.0)));
    assert!(!th.contains(Point::new(10.0, 150.0)), "content scrolled out of view is not clickable");
}

#[test]
fn offsets_are_clamped_to_the_range() {
    let (mut dom, b, tall) = scroll_box();
    dom.set_scroll(b, (40.0, 10_000.0));
    let p = paint(&dom, CSS);
    let sc = p.scrollers.iter().find(|s| s.id == b).unwrap();
    assert_eq!(sc.offset, Vec2::new(0.0, 300.0), "x cannot scroll at all; y stops at the end");
    assert_eq!(hit(&p, tall).rect.y0, -300.0);
}

#[test]
fn overflow_hidden_clips_but_does_not_scroll() {
    let (dom, b, tall) = scroll_box();
    let p = paint(&dom, ".box { width: 200px; height: 100px; overflow: hidden } .tall { height: 400px }");
    assert!(p.scrollers.iter().all(|s| s.id != b));
    assert_eq!(hit(&p, tall).clip, hit(&p, b).rect);
}

#[test]
fn page_scrolls_when_content_is_taller_than_the_viewport() {
    let mut dom = Dom::new();
    el(&mut dom, ROOT, "huge");
    let p = paint(&dom, ".huge { height: 2000px }");
    let root = p.scrollers.iter().find(|s| s.id == ROOT).expect("root is a scroller");
    assert_eq!(root.max.y, 1400.0);
}

#[test]
fn nested_scrollers_are_listed_outermost_first() {
    let mut dom = Dom::new();
    let outer = el(&mut dom, ROOT, "outer");
    let b = el(&mut dom, outer, "box");
    el(&mut dom, b, "tall");
    el(&mut dom, outer, "spacer");
    let p = paint(&dom, &format!("{CSS} .outer {{ height: 300px; overflow: auto }} .spacer {{ height: 900px }}"));
    let ids: Vec<NodeId> = p.scrollers.iter().map(|s| s.id).collect();
    assert_eq!(ids, vec![outer, b]);
}

#[test]
fn clip_is_the_padding_box_inside_borders() {
    let (dom, b, tall) = scroll_box();
    let p = paint(&dom, &format!("{CSS} .box {{ border: 5px solid red }}"));
    assert_eq!(hit(&p, tall).clip, hit(&p, b).rect.inset(-5.0));
}

#[test]
fn opacity_and_overflow_layers_balance() {
    let (mut dom, b, _) = scroll_box();
    dom.set_scroll(b, (0.0, 20.0));
    for extra in [".box { opacity: 0.5 }", ".box { opacity: 0.5; overflow: hidden }", ".tall { opacity: 0.3 }"] {
        let p = paint(&dom, &format!("{CSS} {extra}"));
        assert_eq!(p.open_layers, 0, "{extra}");
    }
}

#[test]
fn nested_scroll_offsets_add_up_and_clips_intersect() {
    let mut dom = Dom::new();
    let outer = el(&mut dom, ROOT, "outer");
    el(&mut dom, outer, "spacer");
    let b = el(&mut dom, outer, "box");
    let tall = el(&mut dom, b, "tall");
    el(&mut dom, outer, "spacer");
    // Outer content: 120 + 100 + 120 = 340px in a 200px viewport → max offset 140.
    dom.set_scroll(outer, (0.0, 140.0));
    dom.set_scroll(b, (0.0, 30.0));
    let css = format!("{CSS} .outer {{ height: 200px; overflow: auto }} .spacer {{ height: 120px }}");
    let p = paint(&dom, &css);
    let (oh, bh, th) = (hit(&p, outer), hit(&p, b), hit(&p, tall));
    assert_eq!(bh.rect.y0, oh.rect.y0 - 20.0, "box moved by the outer offset, 20px above the outer viewport");
    assert_eq!(th.rect.y0, bh.rect.y0 - 30.0, "content moved by both offsets");
    assert_eq!(th.clip, bh.rect.intersect(oh.rect), "clip is box ∩ outer viewport");
    // Inside the box and its content, but in the 20px scrolled above the outer viewport.
    let above = Point::new(10.0, bh.rect.y0 + 10.0);
    assert!(bh.rect.contains(above) && th.rect.contains(above));
    assert!(!th.contains(above), "scrolled out of the outer viewport: not clickable");
    assert!(th.contains(Point::new(10.0, oh.rect.y0 + 10.0)));
}
