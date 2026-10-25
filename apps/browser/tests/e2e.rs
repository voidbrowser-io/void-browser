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
