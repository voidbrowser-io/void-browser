//! Built-in `void://` pages, compiled into the binary.

macro_rules! pages {
    ($($key:literal => $file:literal),* $(,)?) => {
        pub fn builtin(key: &str) -> Option<&'static str> {
            match key {
                $($key => Some(include_str!($file)),)*
                _ => None,
            }
        }
    };
}

pages! {
    "chrome/index.html" => "../pages/chrome/index.html",
    "chrome/main.tsx" => "../pages/chrome/main.tsx",
    "chrome/chrome.css" => "../pages/chrome/chrome.css",
    "newtab/index.html" => "../pages/newtab/index.html",
    "newtab/main.tsx" => "../pages/newtab/main.tsx",
    "newtab/newtab.css" => "../pages/newtab/newtab.css",
    // The demo app ships inside the browser, so an installed copy works offline.
    "demo/index.html" => "../../web-void/index.html",
    "demo/src/main.tsx" => "../../web-void/src/main.tsx",
    "demo/src/App.tsx" => "../../web-void/src/App.tsx",
    "demo/src/Dashboard.tsx" => "../../web-void/src/Dashboard.tsx",
    "demo/src/Bench.tsx" => "../../web-void/src/Bench.tsx",
    "demo/src/app.css" => "../../web-void/src/app.css",
}

/// Only the browser's own UI pages may control tabs (`__browser`).
pub fn privileged(url: &url::Url) -> bool {
    url.scheme() == "void" && matches!(url.host_str(), Some("chrome" | "newtab"))
}
