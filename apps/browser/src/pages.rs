//! Built-in `void://` pages, compiled into the binary.

macro_rules! pages {
    ($($key:literal => $file:literal),* $(,)?) => {
        pub fn builtin(key: &str) -> Option<&'static str> {
            match key {
                $($key => Some(include_str!(concat!("../pages/", $file))),)*
                _ => None,
            }
        }
    };
}

pages! {
    "chrome/index.html" => "chrome/index.html",
    "chrome/main.tsx" => "chrome/main.tsx",
    "chrome/chrome.css" => "chrome/chrome.css",
    "newtab/index.html" => "newtab/index.html",
    "newtab/main.tsx" => "newtab/main.tsx",
    "newtab/newtab.css" => "newtab/newtab.css",
}
