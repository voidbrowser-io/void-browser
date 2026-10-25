use void_css::{cascade, parse_color, parse_len, split_top, uses_hover, Background, Display, Len, Rgba, Selector, Stylesheet, Track, UA_CSS};
use void_dom::{Dom, ROOT};

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

fn rgb_close(c: Rgba, r: u8, g: u8, b: u8) -> bool {
    (c.r * 255.0 - r as f32).abs() < 1.5 && (c.g * 255.0 - g as f32).abs() < 1.5 && (c.b * 255.0 - b as f32).abs() < 1.5
}

/// Build `root > div.card#main > span`, apply `css`, return (dom, card, span).
fn tree(css: &str) -> (Dom, Stylesheet, u32, u32) {
    let mut dom = Dom::new();
    let card = dom.create_element("div");
    dom.set_attr(card, "class", "card");
    dom.set_attr(card, "id", "main");
    let span = dom.create_element("span");
    dom.insert(ROOT, card, None);
    dom.insert(card, span, None);
    let mut sheet = Stylesheet::parse(UA_CSS);
    sheet.append(css);
    (dom, sheet, card, span)
}

// ───────────── Colors & values ─────────────

#[test]
fn hex_colors() {
    let cur = Rgba::BLACK;
    assert!(rgb_close(parse_color("#fff", cur).unwrap(), 255, 255, 255));
    assert!(rgb_close(parse_color("#336699", cur).unwrap(), 0x33, 0x66, 0x99));
    let c = parse_color("#00000080", cur).unwrap();
    assert!(close(c.a, 128.0 / 255.0));
    assert!(parse_color("#12", cur).is_none());
}

#[test]
fn functional_colors() {
    let cur = Rgba::BLACK;
    assert!(rgb_close(parse_color("rgb(10 20 30)", cur).unwrap(), 10, 20, 30));
    assert!(rgb_close(parse_color("rgba(10, 20, 30, 0.5)", cur).unwrap(), 10, 20, 30));
    assert!(close(parse_color("rgb(0 0 0 / 0.25)", cur).unwrap().a, 0.25));
    assert!(rgb_close(parse_color("hsl(0 100% 50%)", cur).unwrap(), 255, 0, 0));
}

#[test]
fn oklch_matches_reference_values() {
    let cur = Rgba::BLACK;
    // oklch(62.8% 0.2577 29.23) is sRGB red.
    assert!(rgb_close(parse_color("oklch(62.8% 0.2577 29.23)", cur).unwrap(), 255, 0, 0));
    // Zero chroma is a gray; L=100% is white.
    assert!(rgb_close(parse_color("oklch(100% 0 0)", cur).unwrap(), 255, 255, 255));
    let a = parse_color("oklch(50% 0.1 200 / 0.3)", cur).unwrap();
    assert!(close(a.a, 0.3));
}

#[test]
fn named_and_current_color() {
    let cur = Rgba::rgb8(1, 2, 3);
    assert_eq!(parse_color("currentColor", cur), Some(cur));
    assert_eq!(parse_color("transparent", cur).unwrap().a, 0.0);
    assert!(parse_color("papayawhip-ish", cur).is_none());
}

#[test]
fn lengths() {
    assert_eq!(parse_len("12px", 16.0), Len::Px(12.0));
    assert_eq!(parse_len("2rem", 10.0), Len::Px(32.0));
    assert_eq!(parse_len("2em", 10.0), Len::Px(20.0));
    assert_eq!(parse_len("50%", 16.0), Len::Pct(0.5));
    assert_eq!(parse_len("auto", 16.0), Len::Auto);
    assert_eq!(parse_len("0", 16.0), Len::Px(0.0));
}

#[test]
fn split_respects_parentheses() {
    assert_eq!(split_top("a, rgb(1, 2, 3), b", ','), vec!["a", "rgb(1, 2, 3)", "b"]);
    assert_eq!(split_top("0 4px  rgb(0 0 0)", ' '), vec!["0", "4px", "rgb(0 0 0)"]);
}

// ───────────── Selectors ─────────────

#[test]
fn specificity() {
    assert_eq!(Selector::parse("#a .b c").unwrap().specificity, (1, 1, 1));
    assert_eq!(Selector::parse("a.b.c:hover").unwrap().specificity, (0, 3, 1));
    assert!(Selector::parse("a::before").is_none(), "pseudo-elements are not supported");
    assert!(Selector::parse(":nth-child(2)").is_none());
}

#[test]
fn descendant_and_child_combinators() {
    let (dom, _, card, span) = tree("");
    assert!(Selector::parse(".card span").unwrap().matches(&dom, span));
    assert!(Selector::parse("void-root span").unwrap().matches(&dom, span));
    assert!(Selector::parse(".card > span").unwrap().matches(&dom, span));
    assert!(!Selector::parse("void-root > span").unwrap().matches(&dom, span));
    assert!(Selector::parse("div#main.card").unwrap().matches(&dom, card));
    assert!(!Selector::parse("div.other").unwrap().matches(&dom, card));
}

#[test]
fn hover_matches_ancestors_of_hovered_node() {
    let (mut dom, _, card, span) = tree("");
    let sel = Selector::parse(".card:hover").unwrap();
    assert!(!sel.matches(&dom, card));
    dom.hover = Some(span);
    assert!(sel.matches(&dom, card));
}

#[test]
fn first_and_last_child_skip_text_nodes() {
    let mut dom = Dom::new();
    let t = dom.create_text(" ");
    let a = dom.create_element("li");
    let b = dom.create_element("li");
    for id in [t, a, b] {
        dom.insert(ROOT, id, None);
    }
    assert!(Selector::parse("li:first-child").unwrap().matches(&dom, a));
    assert!(Selector::parse("li:last-child").unwrap().matches(&dom, b));
    assert!(!Selector::parse("li:first-child").unwrap().matches(&dom, b));
}

// ───────────── Parsing & cascade ─────────────

#[test]
fn nesting_expands_to_flat_rules() {
    let sheet = Stylesheet::parse(".a { color: red; &:hover { color: blue; } .b { color: green; } }");
    assert_eq!(sheet.rules.len(), 3);
    assert!(uses_hover(&sheet));
}

#[test]
fn comments_and_media_queries_are_skipped() {
    let sheet = Stylesheet::parse("/* hi */ .a { color: red } @media (min-width: 1px) { .b { color: blue } }");
    assert_eq!(sheet.rules.len(), 1);
}

#[test]
fn specificity_beats_order_and_inline_beats_all() {
    let (mut dom, sheet, card, _) = tree("#main { color: #ff0000 } .card { color: #00ff00 }");
    let s = &cascade(&dom, &sheet).styles[&card];
    assert!(rgb_close(s.color, 255, 0, 0));
    dom.set_attr(card, "style", "color: #0000ff");
    let s = &cascade(&dom, &sheet).styles[&card];
    assert!(rgb_close(s.color, 0, 0, 255));
}

#[test]
fn later_rule_wins_on_equal_specificity() {
    let (dom, sheet, card, _) = tree(".card { padding: 1px } .card { padding: 2px }");
    assert_eq!(cascade(&dom, &sheet).styles[&card].padding[0], Len::Px(2.0));
}

#[test]
fn inheritance_and_custom_properties() {
    let (dom, sheet, card, span) = tree(
        ".card { --accent: #123456; color: var(--accent); font-size: 20px }
         span { background: var(--missing, #ffffff); padding: 1em }",
    );
    let styles = cascade(&dom, &sheet).styles;
    assert!(rgb_close(styles[&card].color, 0x12, 0x34, 0x56));
    assert!(rgb_close(styles[&span].color, 0x12, 0x34, 0x56), "color is inherited");
    assert_eq!(styles[&span].font_size, 20.0, "font-size is inherited");
    assert_eq!(styles[&span].padding[0], Len::Px(20.0), "em uses the element's font size");
    assert_eq!(styles[&span].background, Some(Background::Color(Rgba::rgb8(255, 255, 255))));
}

#[test]
fn ua_defaults() {
    let (dom, sheet, card, span) = tree("");
    let styles = cascade(&dom, &sheet).styles;
    assert_eq!(styles[&card].display, Display::Block);
    assert_eq!(styles[&span].display, Display::Inline);
    assert_eq!(styles[&card].margin[0], Len::Px(0.0), "no default margins in Void");
}

#[test]
fn shorthands() {
    let (dom, sheet, card, _) = tree(
        ".card { padding: 1px 2px 3px; margin: 4px 5px; flex: 1; gap: 6px 7px;
                 border: 2px solid #ff0000; border-radius: continuous; white-space: nowrap }",
    );
    let s = &cascade(&dom, &sheet).styles[&card];
    assert_eq!(s.padding, [Len::Px(1.0), Len::Px(2.0), Len::Px(3.0), Len::Px(2.0)]);
    assert_eq!(s.margin, [Len::Px(4.0), Len::Px(5.0), Len::Px(4.0), Len::Px(5.0)]);
    assert_eq!((s.flex_grow, s.flex_shrink, s.flex_basis), (1.0, 1.0, Len::Pct(0.0)));
    assert_eq!(s.gap, (Len::Px(6.0), Len::Px(7.0)));
    assert_eq!(s.border_width, 2.0);
    assert!(rgb_close(s.border_color, 255, 0, 0));
    assert!(s.squircle);
    assert!(s.nowrap);
}

#[test]
fn grid_tracks_and_gradients() {
    let (dom, sheet, card, _) = tree(
        ".card { display: grid; grid-template-columns: repeat(2, 1fr) 100px minmax(0, 2fr);
                 background: linear-gradient(to right, #000000, #ffffff);
                 box-shadow: 0 4px 12px rgb(0 0 0 / 0.5) }",
    );
    let s = &cascade(&dom, &sheet).styles[&card];
    assert_eq!(s.display, Display::Grid);
    assert_eq!(s.grid_columns, vec![Track::Fr(1.0), Track::Fr(1.0), Track::Px(100.0), Track::Fr(2.0)]);
    let Some(Background::Linear { angle_deg, stops }) = &s.background else { panic!("gradient") };
    assert_eq!(*angle_deg, 90.0);
    assert_eq!(stops.len(), 2);
    let sh = s.shadow.unwrap();
    assert_eq!((sh.x, sh.y, sh.blur), (0.0, 4.0, 12.0));
    assert!(close(sh.color.a, 0.5));
}

#[test]
fn display_none_hides() {
    let (dom, sheet, card, _) = tree(".card { display: none }");
    assert_eq!(cascade(&dom, &sheet).styles[&card].display, Display::None);
}
