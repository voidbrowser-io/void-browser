# Void Browser

A modern-only web browser engine for Windows, in Rust. It runs TSX directly
(no bundler), uses fine-grained signals instead of a virtual DOM, and renders
on the GPU. It deliberately drops legacy web compatibility — see "Void Profile".

## Layout

```
crates/void-dom      DOM arena (u32 ids). No deps.
crates/void-css      Parser (nesting), selectors, cascade, values. Depends on dom.
crates/void-layout   Styles → Taffy (flex/grid/block); Parley text leaves. Depends on dom, css.
crates/void-paint    Layout tree → Vello scene; hit boxes. Depends on dom, css, layout.
crates/void-js       QuickJS + oxc; module loader (file/http/void://); host commands. Depends on dom, css.
  runtime/void.js    Signals, JSX runtime, For/Show, event dispatch — the only JS in the engine.
apps/browser         winit window, tabs (View = one page), --auto scripting, installer.
  pages/             Built-in void:// pages (browser UI = void://chrome, void://newtab) in TSX.
apps/web-void        Demo app as plain .tsx (also compiled in as void://demo).
apps/web-react       Same app in React + Vite — the baseline for comparisons.
packages/void-types  TypeScript types for `import … from "void"`.
```

Dependency direction is strict: `dom ← css ← layout ← paint`, `js → dom, css`,
`browser → everything`. Never add an upward edge (e.g. css depending on layout).

## Commands

Rust is in `~/.cargo/bin` (may not be on PATH in fresh shells).
pnpm must be run as `npx -y pnpm@10.18.0` (the nvm corepack is broken).

```bash
cargo test --workspace                       # all Rust tests incl. e2e (opens windows)
cargo test -p void-css                       # one crate
VOID_SKIP_E2E=1 cargo test --workspace       # no GPU/display
npx -y pnpm@10.18.0 -r test                  # web app tests
npx -y pnpm@10.18.0 -r typecheck
cargo run -p void-browser -- apps/web-void   # run the browser on the demo
cargo packager -p void-browser --release     # dist/void_<ver>_x64-setup.exe
```

If a `void.exe` window is open, `cargo build` cannot overwrite it — use
`CARGO_TARGET_DIR=target/e2e` instead of closing the user's window.

## Checking UI changes

Never click on the real desktop. Drive the browser with `--auto`:

```bash
target/debug/void.exe apps/web-void --auto "click-text +1; expect double = 2; shot out.png; quit"
```

Steps: `click x y`, `click-text <text>`, `move x y`, `move-on <text>`, `type`, `key ctrl+t`,
`nav <url>`, `scroll x y dy [ctrl|shift]` (positive dy = down), `scroll-on <text> | <dy> [ctrl|shift]`,
`wait <ms>`, `expect <text>`, `expect-title`, `expect-tabs <n>`, `expect-zoom <percent>`,
`expect-hidden <text>` (on the page, not visible), `where <text>` (prints window position),
`shot <png>`, `quit`. `key` also takes `up`, `down`, `pageup`, `pagedown`, `home`, `end`,
`space`, `plus`, `minus`; `key ctrl+<letter>` sends the Windows control character as the key.
A failed `expect` exits with code 1. Look at the PNG before claiming a visual change works.

## Conventions

- Every bug fix comes with a test that failed before the fix.
- Every user-visible feature gets an e2e scenario in `apps/browser/tests/e2e.rs`.
- Library APIs (vello, parley, taffy, oxc, rquickjs) move fast. Read the source in
  `~/.cargo/registry/src/*/<crate>-<ver>/` instead of guessing signatures.
- Keep JS in the engine to `runtime/void.js` and `prelude.js`; everything else is Rust.
- Never edit source files through PowerShell `Get-Content`/`Set-Content`: Windows
  PowerShell 5.1 reads UTF-8 as CP1251 and writes BOMs. Use the editor tools, or
  Python with explicit UTF-8.
- Commit messages: imperative subject, body explains why. End with the Co-Authored-By line.
- Push only when asked. Remote is `git@github-void:voidbrowser-io/void-browser.git`
  (SSH host alias `github-void`; plain `github.com` is the user's work key — leave it alone).

## Void Profile (what the engine accepts)

Kept: flexbox, grid, block, inline text, CSS nesting, custom properties,
`oklch()`/`rgb()`/`hsl()`, gradients, shadows, `:hover/:active/:focus/:first-child/:last-child`,
`overflow`/`overflow-x`/`overflow-y` (`visible|clip|hidden|auto|scroll`, CSS Overflow 3
axis rules; `clip` is not a scroll container; `void-root` is `overflow: auto`),
overlay scrollbars, wheel/keyboard scrolling (repaint only, no relayout), per-tab zoom,
ES modules, TSX/TS, signals.

Dropped on purpose: floats, table layout, quirks mode, vendor prefixes,
`document.write`, classic scripts, `var`-hoisting tricks, content-box, UA margins,
legacy HTML pages (refused with an error page), scrollbar styling
(`::-webkit-scrollbar`, `scrollbar-width`, `scrollbar-color`), scrollbars that take
layout space, `body` → viewport overflow propagation.

Before adding a web feature, check it against this list and the
`void-architecture` skill. If it only exists for backwards compatibility, it does not go in.

## Security model

Pages are isolated per tab (own QuickJS runtime and DOM). Only `void://chrome`
and `void://newtab` get `__browser` (see `pages::privileged`). Any change to the
module loader, URL handling, `HostCmd`, or privileges needs a security review.
