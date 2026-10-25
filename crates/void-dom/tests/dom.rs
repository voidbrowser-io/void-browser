use void_dom::{Dom, NodeKind, ROOT};

fn children(dom: &Dom, id: u32) -> Vec<u32> {
    dom.get(id).unwrap().children.clone()
}

#[test]
fn starts_with_root_element() {
    let dom = Dom::new();
    assert_eq!(dom.get(ROOT).unwrap().tag(), Some("void-root"));
    assert!(dom.dirty);
}

#[test]
fn tags_are_lowercased() {
    let mut dom = Dom::new();
    let id = dom.create_element("DIV");
    assert_eq!(dom.get(id).unwrap().tag(), Some("div"));
}

#[test]
fn insert_appends_and_inserts_before() {
    let mut dom = Dom::new();
    let a = dom.create_element("a");
    let b = dom.create_element("b");
    let c = dom.create_element("c");
    dom.insert(ROOT, a, None);
    dom.insert(ROOT, c, None);
    dom.insert(ROOT, b, Some(c));
    assert_eq!(children(&dom, ROOT), vec![a, b, c]);
    assert_eq!(dom.get(b).unwrap().parent, Some(ROOT));
}

#[test]
fn insert_moves_node_from_old_parent() {
    let mut dom = Dom::new();
    let p1 = dom.create_element("div");
    let p2 = dom.create_element("div");
    let x = dom.create_text("x");
    dom.insert(ROOT, p1, None);
    dom.insert(ROOT, p2, None);
    dom.insert(p1, x, None);
    dom.insert(p2, x, None);
    assert!(children(&dom, p1).is_empty());
    assert_eq!(children(&dom, p2), vec![x]);
}

#[test]
fn insert_ignores_self_and_unknown_ids() {
    let mut dom = Dom::new();
    let a = dom.create_element("a");
    dom.insert(a, a, None);
    dom.insert(ROOT, 999, None);
    dom.insert(999, a, None);
    assert!(children(&dom, a).is_empty());
    assert!(children(&dom, ROOT).is_empty());
}

#[test]
fn remove_detaches_and_marks_dirty() {
    let mut dom = Dom::new();
    let a = dom.create_element("a");
    dom.insert(ROOT, a, None);
    dom.dirty = false;
    dom.remove(a);
    assert!(children(&dom, ROOT).is_empty());
    assert_eq!(dom.get(a).unwrap().parent, None);
    assert!(dom.dirty);
}

#[test]
fn removing_focused_subtree_clears_focus() {
    let mut dom = Dom::new();
    let form = dom.create_element("form");
    let input = dom.create_element("input");
    dom.insert(ROOT, form, None);
    dom.insert(form, input, None);
    dom.focus_input(input);
    assert_eq!(dom.focus, Some(input));
    dom.remove(form);
    assert_eq!(dom.focus, None);
}

#[test]
fn clear_between_removes_only_inner_siblings() {
    let mut dom = Dom::new();
    let ids: Vec<u32> = (0..5).map(|i| dom.create_text(&i.to_string())).collect();
    for &id in &ids {
        dom.insert(ROOT, id, None);
    }
    dom.clear_between(ids[0], ids[4]);
    assert_eq!(children(&dom, ROOT), vec![ids[0], ids[4]]);
}

#[test]
fn set_text_only_dirties_on_change() {
    let mut dom = Dom::new();
    let t = dom.create_text("hi");
    dom.dirty = false;
    dom.set_text(t, "hi");
    assert!(!dom.dirty);
    dom.set_text(t, "bye");
    assert!(dom.dirty);
    assert!(matches!(&dom.get(t).unwrap().kind, NodeKind::Text(s) if s == "bye"));
}

#[test]
fn set_attr_routes_class_style_and_value() {
    let mut dom = Dom::new();
    let el = dom.create_element("input");
    dom.set_attr(el, "class", "  a  b ");
    dom.set_attr(el, "style", "color: red");
    dom.set_attr(el, "value", "typed");
    dom.set_attr(el, "placeholder", "hint");
    let node = dom.get(el).unwrap();
    let NodeKind::Element { classes, attrs, .. } = &node.kind else { panic!() };
    assert_eq!(classes, &vec!["a".to_string(), "b".to_string()]);
    assert_eq!(attrs.get("placeholder").map(String::as_str), Some("hint"));
    assert_eq!(node.inline_style, "color: red");
    assert_eq!(node.value, "typed");
}

#[test]
fn path_goes_from_target_to_root() {
    let mut dom = Dom::new();
    let a = dom.create_element("div");
    let b = dom.create_element("span");
    dom.insert(ROOT, a, None);
    dom.insert(a, b, None);
    assert_eq!(dom.path(b), vec![b, a, ROOT]);
    assert!(dom.is_ancestor(ROOT, b));
    assert!(!dom.is_ancestor(b, a));
}

#[test]
fn focus_input_selects_and_ignores_non_inputs() {
    let mut dom = Dom::new();
    let div = dom.create_element("div");
    let a = dom.create_element("input");
    let b = dom.create_element("input");
    dom.focus_input(div);
    assert_eq!(dom.focus, None);
    dom.focus_input(a);
    assert!(dom.get(a).unwrap().selected);
    dom.focus_input(b);
    assert_eq!(dom.focus, Some(b));
    assert!(!dom.get(a).unwrap().selected, "old input loses its selection");
}
