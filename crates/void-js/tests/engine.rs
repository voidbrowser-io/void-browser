use std::path::PathBuf;
use std::time::Duration;
use url::Url;
use void_css::UA_CSS;
use void_dom::{Dom, NodeId, NodeKind, ROOT};
use void_js::{parse_input, transpile, Engine, HostCmd};

// ───────────── Helpers ─────────────

/// A fresh directory with the given files.
fn site(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("void-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, src) in files {
        let p = dir.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, src).unwrap();
    }
    dir
}

fn file_url(dir: &PathBuf, file: &str) -> Url {
    Url::from_file_path(dir.join(file)).unwrap()
}

fn no_builtins(_: &str) -> Option<&'static str> {
    None
}

fn test_builtins(key: &str) -> Option<&'static str> {
    match key {
        "test/main.tsx" => Some("import { render } from 'void'; render(() => <p>{typeof __browser}</p>);"),
        _ => None,
    }
}

fn boot(url: &Url) -> Engine {
    match Engine::boot(url, UA_CSS, no_builtins, false, String::new()) {
        Ok(e) => e,
        Err((_, msg)) => panic!("boot failed: {msg}"),
    }
}

/// Boot a single-module app.
fn app(name: &str, tsx: &str) -> Engine {
    let dir = site(name, &[("main.tsx", tsx)]);
    boot(&file_url(&dir, "main.tsx"))
}

fn text_of(dom: &Dom, id: NodeId) -> String {
    match &dom.get(id).unwrap().kind {
        NodeKind::Text(t) => t.clone(),
        NodeKind::Element { .. } => dom.get(id).unwrap().children.iter().map(|&c| text_of(dom, c)).collect(),
    }
}

fn page_text(e: &Engine) -> String {
    text_of(&e.dom.borrow(), ROOT)
}

/// First attached element with `tag` (and `class`, if given).
fn find(e: &Engine, tag: &str, class: &str) -> NodeId {
    fn walk(dom: &Dom, id: NodeId, tag: &str, class: &str) -> Option<NodeId> {
        let node = dom.get(id)?;
        if let NodeKind::Element { tag: t, classes, .. } = &node.kind {
            if t == tag && (class.is_empty() || classes.iter().any(|c| c == class)) {
                return Some(id);
            }
        }
        node.children.iter().find_map(|&c| walk(dom, c, tag, class))
    }
    walk(&e.dom.borrow(), ROOT, tag, class).unwrap_or_else(|| panic!("no <{tag} class={class}>"))
}

fn click(e: &Engine, id: NodeId) -> bool {
    let path = e.dom.borrow().path(id);
    e.dispatch(path, "click", "", "").unwrap()
}

// ───────────── Transpiling & addresses ─────────────

#[test]
fn transpiles_tsx_to_plain_js() {
    let js = transpile("App.tsx", "const n: number = 1; export const A = () => <div class=\"x\">{n}</div>;").unwrap();
    assert!(js.contains("void"), "JSX imports the void runtime: {js}");
    assert!(js.contains("jsx"), "{js}");
    assert!(!js.contains(": number"), "types are stripped: {js}");
}

#[test]
fn syntax_errors_name_the_file() {
    let err = transpile("Broken.tsx", "const = ;").unwrap_err();
    assert!(err.starts_with("Broken.tsx:"), "{err}");
}

#[test]
fn address_bar_input() {
    assert_eq!(parse_input("http://a.b/c").unwrap().as_str(), "http://a.b/c");
    assert_eq!(parse_input("void://newtab").unwrap().scheme(), "void");
    assert_eq!(parse_input("localhost:8080").unwrap().as_str(), "http://localhost:8080/");
    assert_eq!(parse_input("example.com").unwrap().as_str(), "https://example.com/");
    assert!(parse_input("how to center a div").unwrap_err().contains("no search engine"));
    assert!(parse_input("  ").is_err());
    let dir = site("addr", &[("index.html", "<title>x</title>")]);
    let u = parse_input(dir.to_str().unwrap()).unwrap();
    assert_eq!(u.scheme(), "file");
}

// ───────────── Loading ─────────────

#[test]
fn boots_a_tsx_module() {
    let e = app("module", "import { render } from 'void'; const name: string = 'Void'; render(() => <h1>Hello {name}</h1>);");
    assert_eq!(page_text(&e), "Hello Void");
    assert_eq!(e.shared.borrow().stats.modules, 1);
}

#[test]
fn boots_an_html_page_through_its_module_script() {
    let dir = site(
        "html",
        &[
            ("index.html", "<!doctype html><title> My App </title><script type=\"module\" src=\"./src/main.tsx\"></script>"),
            ("src/main.tsx", "import { render } from 'void'; import { Hi } from './parts/Hi.tsx'; render(Hi);"),
            ("src/parts/Hi.tsx", "export function Hi() { return <p>from a nested module</p>; }"),
        ],
    );
    let e = boot(&file_url(&dir, "index.html"));
    assert_eq!(e.html_title.as_deref(), Some("My App"));
    assert_eq!(page_text(&e), "from a nested module");
    assert_eq!(e.shared.borrow().stats.modules, 2);
    // A directory URL means its index.html.
    let e = boot(&Url::from_directory_path(&dir).unwrap());
    assert_eq!(page_text(&e), "from a nested module");
}

#[test]
fn refuses_legacy_html() {
    let dir = site("legacy", &[("index.html", "<table><tr><td><font>1999</font></td></tr></table>")]);
    let Err((_, msg)) = Engine::boot(&file_url(&dir, "index.html"), UA_CSS, no_builtins, false, String::new()) else {
        panic!("legacy page must not boot");
    };
    assert!(msg.contains("not a Void page"), "{msg}");
}

#[test]
fn css_imports_feed_the_cascade() {
    let dir = site(
        "css",
        &[("main.tsx", "import { render } from 'void'; import './app.css'; render(() => <div class='x' />);"), ("app.css", ".x { color: red; &:hover { color: blue } }")],
    );
    let e = boot(&file_url(&dir, "main.tsx"));
    let css = &e.shared.borrow().css;
    assert!(css.rules.len() > 2);
    assert!(void_css::uses_hover(css));
}

#[test]
fn runtime_errors_are_reported() {
    let dir = site("throws", &[("main.tsx", "throw new Error('boom');")]);
    let Err((_, msg)) = Engine::boot(&file_url(&dir, "main.tsx"), UA_CSS, no_builtins, false, String::new()) else {
        panic!("must fail");
    };
    assert!(msg.contains("boom"), "{msg}");
}

#[test]
fn missing_modules_are_reported() {
    let dir = site("missing", &[("main.tsx", "import './nope.tsx';")]);
    let Err((_, msg)) = Engine::boot(&file_url(&dir, "main.tsx"), UA_CSS, no_builtins, false, String::new()) else {
        panic!("must fail");
    };
    assert!(msg.contains("nope.tsx"), "{msg}");
}

// ───────────── Signals & DOM binding ─────────────

const COUNTER: &str = r#"
import { render, signal, computed } from 'void';
function Counter() {
  const count = signal(0);
  const double = computed(() => count() * 2);
  return (
    <div>
      <button class="inc" onClick={() => count.update((c) => c + 1)}>+</button>
      <span class="out">{count}/{double}</span>
    </div>
  );
}
render(Counter);
"#;

#[test]
fn signals_patch_text_without_rerendering() {
    let e = app("counter", COUNTER);
    assert!(page_text(&e).contains("0/0"));
    let nodes_before = e.dom.borrow().nodes.len();
    let btn = find(&e, "button", "inc");
    click(&e, btn);
    click(&e, btn);
    assert!(page_text(&e).contains("2/4"), "{}", page_text(&e));
    assert_eq!(e.dom.borrow().nodes.len(), nodes_before, "no nodes were created: text was patched in place");
}

#[test]
fn reactive_attributes() {
    let e = app(
        "attrs",
        r#"import { render, signal } from 'void';
        const on = signal(false);
        render(() => <div class={() => (on() ? 'box on' : 'box')} style={{ paddingTop: '3px' }} onClick={() => on.set(true)} />);"#,
    );
    let div = find(&e, "div", "box");
    assert_eq!(e.dom.borrow().get(div).unwrap().inline_style, "padding-top:3px");
    click(&e, div);
    let dom = e.dom.borrow();
    let NodeKind::Element { classes, .. } = &dom.get(div).unwrap().kind else { panic!() };
    assert!(classes.contains(&"on".to_string()));
}

#[test]
fn keyed_for_reuses_nodes() {
    let e = app(
        "for",
        r#"import { render, signal, For } from 'void';
        const a = { n: 'a' }, b = { n: 'b' }, c = { n: 'c' };
        const items = signal([a, b]);
        render(() => (
          <ul>
            <For each={items}>{(it) => <li>{it.n}</li>}</For>
            <button class="go" onClick={() => items.set([b, a, c])}>go</button>
          </ul>
        ));"#,
    );
    let li_ids = |e: &Engine| -> Vec<NodeId> {
        let ul = find(e, "ul", "");
        let dom = e.dom.borrow();
        dom.get(ul).unwrap().children.iter().copied().filter(|&c| dom.get(c).unwrap().tag() == Some("li")).collect()
    };
    let before = li_ids(&e);
    assert_eq!(page_text(&e), "abgo");
    click(&e, find(&e, "button", "go"));
    let after = li_ids(&e);
    assert_eq!(page_text(&e), "bacgo");
    assert_eq!(after[0], before[1], "b's <li> was moved, not recreated");
    assert_eq!(after[1], before[0]);
    assert_eq!(after.len(), 3);
}

#[test]
fn show_toggles_branches() {
    let e = app(
        "show",
        r#"import { render, signal, Show } from 'void';
        const open = signal(false);
        render(() => (
          <div>
            <button class="t" onClick={() => open.update((o) => !o)}>t</button>
            <Show when={open} fallback={<i>closed</i>}><b>open</b></Show>
          </div>
        ));"#,
    );
    assert_eq!(page_text(&e), "tclosed");
    click(&e, find(&e, "button", "t"));
    assert_eq!(page_text(&e), "topen");
    click(&e, find(&e, "button", "t"));
    assert_eq!(page_text(&e), "tclosed");
}

#[test]
fn events_bubble_and_can_be_stopped_or_prevented() {
    let e = app(
        "events",
        r#"import { render, signal } from 'void';
        const log = signal('');
        render(() => (
          <div class="outer" onClick={() => log.update((l) => l + 'O')}>
            <button class="bubble" onClick={() => log.update((l) => l + 'B')}>b</button>
            <button class="stop" onClick={(e) => { e.stopPropagation(); log.update((l) => l + 'S'); }}>s</button>
            <button class="prevent" onClick={(e) => e.preventDefault()}>p</button>
            <p class="log">{log}</p>
          </div>
        ));"#,
    );
    assert!(click(&e, find(&e, "button", "bubble")));
    assert!(click(&e, find(&e, "button", "stop")));
    assert_eq!(text_of(&e.dom.borrow(), find(&e, "p", "log")), "BOS");
    assert!(!click(&e, find(&e, "button", "prevent")), "preventDefault is reported to the host");
}

#[test]
fn input_events_carry_the_value() {
    let e = app(
        "input",
        r#"import { render, signal } from 'void';
        const v = signal('');
        render(() => <div><input onInput={(e) => v.set(e.value)} /><p>{v}</p></div>);"#,
    );
    let input = find(&e, "input", "");
    let path = e.dom.borrow().path(input);
    e.dispatch(path, "input", "typed text", "").unwrap();
    assert!(page_text(&e).contains("typed text"));
}

#[test]
fn timers_fire_through_the_host() {
    let e = app(
        "timers",
        r#"import { render, signal } from 'void';
        const s = signal('waiting');
        setTimeout(() => s.set('fired'), 0);
        const id = setInterval(() => {}, 1000);
        clearInterval(id);
        render(() => <p>{s}</p>);"#,
    );
    assert_eq!(page_text(&e), "waiting");
    std::thread::sleep(Duration::from_millis(5));
    let (next, err) = e.run_timers();
    assert!(err.is_none());
    assert_eq!(page_text(&e), "fired");
    assert!(next.is_none(), "cleared interval left no timers");
}

#[test]
fn microtasks_run_after_boot() {
    let e = app(
        "promise",
        r#"import { render, signal } from 'void';
        const s = signal('sync');
        Promise.resolve().then(() => s.set('async'));
        render(() => <p>{s}</p>);"#,
    );
    assert_eq!(page_text(&e), "async");
}

// ───────────── Host integration ─────────────

#[test]
fn navigate_and_title_go_to_the_host() {
    let e = app(
        "host",
        r#"import { render, navigate, setTitle } from 'void';
        setTitle('Hello tab');
        render(() => <a class="go" onClick={() => navigate('../other/page.html')}>go</a>);"#,
    );
    click(&e, find(&e, "a", "go"));
    let cmds = e.take_cmds();
    assert!(matches!(&cmds[0], HostCmd::Title(t) if t == "Hello tab"));
    let HostCmd::Navigate(to) = &cmds[1] else { panic!("{cmds:?}") };
    assert!(to.starts_with("file:///") && to.ends_with("/other/page.html"), "relative URL resolved: {to}");
}

#[test]
fn only_void_pages_are_privileged() {
    let e = app("unprivileged", "import { render } from 'void'; render(() => <p>{typeof __browser}</p>);");
    assert_eq!(page_text(&e), "undefined");

    let url = Url::parse("void://test/main.tsx").unwrap();
    let Ok(e) = Engine::boot(&url, UA_CSS, test_builtins, true, String::new()) else { panic!() };
    assert_eq!(page_text(&e), "object");
}

#[test]
fn privileged_pages_drive_the_browser() {
    let dir = site(
        "chrome",
        &[("main.tsx", "__browser.newTab('void://newtab'); __browser.activate(2); __browser.back(); globalThis.__browser_state = (s) => { globalThis.got = JSON.parse(s).n; };")],
    );
    let Ok(e) = Engine::boot(&file_url(&dir, "main.tsx"), UA_CSS, no_builtins, true, "{}".into()) else { panic!() };
    let cmds = e.take_cmds();
    assert!(matches!(&cmds[0], HostCmd::NewTab(Some(u)) if u == "void://newtab"));
    assert!(matches!(cmds[1], HostCmd::Activate(2)));
    assert!(matches!(cmds[2], HostCmd::Back));
    e.call_global("__browser_state", "{\"n\":7}").unwrap();
    e.call_global("does_not_exist", "").unwrap();
}
