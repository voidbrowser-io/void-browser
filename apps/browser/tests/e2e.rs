//! End-to-end tests: drive the real browser (window + GPU) with `--auto`
//! scripts. Set VOID_SKIP_E2E=1 on machines without a GPU/display.

use std::path::PathBuf;
use std::process::Command;

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

/// Run the browser with `args` and an `--auto` script; panic with the log on failure.
fn run(args: &[&str], script: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_void"))
        .current_dir(workspace())
        .args(args)
        .arg("--auto")
        .arg(format!("{script}; quit"))
        .output()
        .expect("launch void");
    let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "browser run failed:\n{log}");
    assert!(!log.contains("FAIL"), "{log}");
    log
}

fn skip() -> bool {
    std::env::var_os("VOID_SKIP_E2E").is_some()
}

#[test]
fn new_tab_page_and_tab_shortcuts() {
    if skip() {
        return;
    }
    run(
        &[],
        "expect The web, without the legacy.; expect-tabs 1; \
         key ctrl+t; key ctrl+t; expect-tabs 3; key ctrl+w; expect-tabs 2; \
         click-text ×; expect-tabs 1",
    );
}

#[test]
fn demo_app_counter_todos_and_benchmark() {
    if skip() {
        return;
    }
    run(
        &["apps/web-void"],
        "expect-title Void Demo; expect What if the web had no legacy?; \
         click-text +1; click-text +1; click-text +1; expect double = 6; \
         click-text −1; expect double = 4; \
         click-text What needs to be done?; type Write e2e tests; key enter; \
         expect Write e2e tests; expect Todos · 3 left; \
         click-text Benchmark signals vs React; expect Todos · 2 left; \
         click-text ☀ Light; expect ☾ Dark; \
         click-text Benchmark; click-text Update 10%; expect 1 round, 10%:",
    );
}

#[test]
fn links_history_and_address_bar() {
    if skip() {
        return;
    }
    let demo = workspace().join("apps/web-void/index.html");
    run(
        &[],
        &format!(
            "click-text Demo app · local; expect-title Void Demo; \
             key alt+left; expect The web, without the legacy.; \
             key alt+right; expect-title Void Demo; \
             key ctrl+l; type {}; key enter; expect-title Void Demo",
            demo.display()
        ),
    );
}

#[test]
fn legacy_pages_are_refused() {
    if skip() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("void-e2e-legacy-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("index.html"), "<table><tr><td><font>1999</font></td></tr></table>").unwrap();
    run(&[dir.to_str().unwrap()], "expect not a Void page; expect The legacy web is not supported");
    run(&[], "key ctrl+l; type how to center a div; key enter; expect no search engine");
}

#[test]
fn screenshots_are_rendered_on_the_gpu() {
    if skip() {
        return;
    }
    let shot = std::env::temp_dir().join(format!("void-e2e-{}.png", std::process::id()));
    let log = run(&["apps/web-void"], &format!("shot {}", shot.display()));
    assert!(log.contains("[void] shot"), "{log}");
    let bytes = std::fs::read(&shot).unwrap();
    assert_eq!(&bytes[1..4], b"PNG");
    assert!(bytes.len() > 20_000, "a real frame, not a blank image ({} bytes)", bytes.len());
}

#[test]
fn built_in_demo_works_without_files() {
    if skip() {
        return;
    }
    run(
        &[],
        "click-text Demo app · built in; expect-title Void Demo; \
         click-text +1; expect double = 2; \
         key ctrl+l; type void://nope; key enter; expect no such page",
    );
}

/// A page taller than the window: an input, a scrollable list that can shrink
/// and grow, a horizontally scrolling strip, and many rows.
fn long_page() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("void-e2e-scroll-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.tsx"),
        r#"import { render, signal } from 'void';
import './page.css';
const msg = signal('none');
const n = signal(30);
render(() => (
  <div class="page">
    <h1>Top of page</h1>
    <div class="row-flex">
      <input placeholder="Type here" />
      <button onClick={() => n.set(2)}>Shrink</button>
      <button onClick={() => n.set(30)}>Grow</button>
    </div>
    <div class="inner">
      {() => Array.from({ length: n() }, (_, i) => <p>Inner {i}</p>)}
      <button onClick={() => msg.set('inner bottom')}>Inner bottom</button>
    </div>
    <div class="wide">
      <div class="track">
        <span>Wide start</span>
        <button onClick={() => msg.set('far right')}>Far right</button>
      </div>
    </div>
    {Array.from({ length: 60 }, (_, i) => <p class="row">Row {i}</p>)}
    <button onClick={() => msg.set('page bottom')}>Page bottom</button>
    <p>clicked: {msg}</p>
  </div>
));
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("page.css"),
        ".page { padding: 20px } h1 { height: 40px } .row { height: 30px }
         .row-flex { display: flex; gap: 8px; margin-bottom: 8px }
         .inner { height: 150px; width: 300px; overflow-y: auto; border: 1px solid gray }
         .wide { width: 300px; overflow-x: auto; margin-top: 8px }
         .track { display: flex; justify-content: space-between; width: 1200px; white-space: nowrap }",
    )
    .unwrap();
    dir.join("main.tsx")
}

#[test]
fn wheel_scrolls_nested_boxes_then_chains_to_the_page() {
    if skip() {
        return;
    }
    let page = long_page();
    run(
        &[page.to_str().unwrap()],
        "expect-hidden Page bottom; expect-hidden Inner bottom; \
         scroll-on Inner 0 | 3000; click-text Inner bottom; expect clicked: inner bottom; \
         click-text Top of page; \
         scroll-on Inner bottom | 300; expect-hidden Top of page; \
         scroll-on Row 10 | 5000; click-text Page bottom; expect clicked: page bottom; \
         scroll-on Page bottom | -100000; click-text Top of page",
    );
}

#[test]
fn shift_wheel_scrolls_sideways_and_ctrl_wheel_zooms() {
    if skip() {
        return;
    }
    let page = long_page();
    run(
        &[page.to_str().unwrap()],
        "expect-hidden Far right; scroll-on Wide start | 2000 shift; click-text Far right; expect clicked: far right; \
         click-text Top of page; \
         scroll-on Top of page | -100 ctrl; expect-zoom 110; scroll-on Top of page | 100 ctrl; expect-zoom 100",
    );
}

#[test]
fn keyboard_scrolls_what_is_under_the_pointer() {
    if skip() {
        return;
    }
    let page = long_page();
    run(
        &[page.to_str().unwrap()],
        // Pointer over the browser UI: keys scroll the page.
        "key end; click-text Page bottom; expect-hidden Top of page; \
         key home; click-text Top of page; \
         key pagedown; expect-hidden Top of page; key pageup; click-text Top of page; \
         key space; expect-hidden Top of page; key home; \
         move-on Inner 0; key end; click-text Inner bottom; click-text Top of page; \
         click-text Type here; key space; key pagedown; click-text Top of page",
    );
}

#[test]
fn scroll_offset_resets_when_content_stops_overflowing() {
    if skip() {
        return;
    }
    let page = long_page();
    run(
        &[page.to_str().unwrap()],
        "scroll-on Inner 0 | 3000; expect-hidden Inner 0; \
         click-text Shrink; click-text Grow; click-text Inner 0",
    );
}

#[test]
fn zoom_is_per_tab_and_hit_testing_follows_it() {
    if skip() {
        return;
    }
    // "+1" is at (130, 558.5) at 100%: page y 474.5 below the 84px browser UI.
    // At 110% it must be at (143, 84 + 474.5 × 1.1 ≈ 606). If zoom were ignored
    // for hit testing, this click would land 47px below the button.
    run(
        &["apps/web-void"],
        "key ctrl+plus; expect-zoom 110; click 143 606; expect double = 2; \
         key ctrl+plus; key ctrl+plus; expect-zoom 150; click-text +1; expect double = 4; \
         key ctrl+minus; expect-zoom 125; \
         key ctrl+t; expect-zoom 100; key ctrl+w; expect-zoom 125; \
         key ctrl+0; expect-zoom 100",
    );
}

#[test]
fn ctrl_shortcuts_work_with_windows_control_characters() {
    if skip() {
        return;
    }
    // `key ctrl+t` sends "\u{14}" as the logical key, like Windows does.
    run(&[], "key ctrl+t; expect-tabs 2; key ctrl+w; expect-tabs 1; key ctrl+l; type void://demo; key enter; expect-title Void Demo");
}
