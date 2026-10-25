# Void

**What would the web look like if we stopped promising never to break it?**

Void is an experimental browser engine for Windows, written in Rust. It runs
only modern web code: TSX files directly with no bundler, fine-grained
signals instead of a virtual DOM, Flexbox/Grid layout, and GPU rendering. It
deliberately drops 20+ years of compatibility. Old websites will not open, and
that is the point.

> Status: early prototype. Expect rough edges.

## What's inside

```
crates/
  void-dom      DOM as a flat arena — no legacy APIs
  void-css      Modern-only CSS: nesting, custom properties, oklch(), grid
  void-layout   Flexbox / Grid / block via Taffy, text via Parley
  void-paint    GPU vector rendering via Vello (wgpu, DX12/Vulkan)
  void-js       QuickJS + oxc: TSX is transpiled per file on import
apps/
  browser       Void Browser (Windows .exe)
  web-void      Demo app: plain .tsx files, run directly by Void
  web-react     The same app with React + Vite, as a baseline in Chrome
packages/
  void-types    TypeScript types for `import … from "void"`
```

## Run it

Requirements: Rust (MSVC toolchain), Visual Studio C++ Build Tools, Node 20+, pnpm.

```bash
# Void: open the demo app, edit any file in apps/web-void/src and it hot-reloads
cargo run -p void-browser --release -- apps/web-void/src/main.tsx

# Baseline: the same app in React + Vite
pnpm install
pnpm react
```

## Browsing

Void has tabs, history and an address bar. The browser UI itself is a Void
page written in TSX (`apps/browser/pages/chrome`), rendered by the same engine.

Every tab runs its own JS engine and DOM. A tab can open:

| Address | What happens |
|---|---|
| `void://newtab` | Built-in pages, compiled into the binary (privileged, like `chrome://`) |
| `file:///…` or a local path | Loaded from disk, **hot-reloads** on save |
| `http(s)://…/main.tsx` | Modules fetched over the network, transpiled on import |
| `http(s)://…/` (HTML) | Void finds `<script type="module" src="…">` and runs it — the Vite convention |
| Anything else | Refused: legacy HTML is not supported, on purpose |

`<a href>` links, `navigate(url)` and `setTitle(title)` from `"void"` work inside pages.

Shortcuts: `Ctrl+T` new tab · `Ctrl+W` close · `Ctrl+L` address bar ·
`Ctrl+Tab` / `Ctrl+1…9` switch · `Alt+←/→` back/forward · `F5` reload ·
`Ctrl+A/C/V` in inputs · `Ctrl+=` / `Ctrl+-` / `Ctrl+0` zoom in, out, reset.

Scrolling and zoom:

| Input | What it does |
|---|---|
| Wheel | Scrolls the innermost scroll container under the pointer; at its limit, the next one out scrolls |
| `Shift`+wheel | Scrolls sideways |
| `Ctrl`+wheel | Zooms |
| `↑` `↓` `←` `→` | 40px |
| `PageUp` / `PageDown`, `Space` / `Shift+Space` | 87.5% of the container height |
| `Home` / `End` | Start / end |

Keys scroll only when no input has focus. They scroll the container under
the pointer, or the page when the pointer is over the browser UI.
Scrollbars are thin overlay thumbs: drawn on top, no layout space.

Zoom is per tab, in steps from 50% to 300%. The toolbar shows the zoom
level when it is not 100%.

```bash
# Serve the demo over HTTP and open it in Void
python -m http.server 8080 -d apps/web-void
cargo run -p void-browser --release -- http://localhost:8080/
```

## The Void profile

| Kept | Dropped |
|---|---|
| Flexbox, Grid, block, inline text | Floats, tables-as-layout, quirks mode |
| CSS nesting, custom properties, `oklch()`, gradients, shadows | Vendor prefixes, `!important` semantics |
| `border-box` everywhere, zero default margins | `content-box`, UA margin soup |
| ES modules, TSX/TS natively | Classic scripts, `document.write`, `with` |
| Signals (`signal`, `computed`, `effect`, `For`, `Show`) | Virtual DOM diffing |
| `border-radius: continuous` / `corner-shape: squircle` | — |
| `overflow`, `overflow-x/y`: `visible`, `clip`, `hidden`, `auto`, `scroll`; the page root scrolls | Scrollbar styling (`::-webkit-scrollbar`, `scrollbar-width`, `scrollbar-color`) |
| Overlay scrollbars | Scrollbars that take layout space, `body` → viewport overflow propagation |

## Writing a Void app

```tsx
import { signal, render } from "void";
import "./app.css";

function Counter() {
  const count = signal(0);
  return <button onClick={() => count.update((c) => c + 1)}>Clicked {count} times</button>;
}

render(Counter);
```

Components run once. Pass signals (or functions) into JSX and only the text
nodes and attributes that read them update.

## Automation

```bash
void apps/web-void --auto "click 440 510; key ctrl+t; type localhost:8080; key enter; wait 300; shot out.png; quit"
```

Scripted input and offscreen GPU screenshots, for tests and CI.

## Tests

```bash
pnpm test        # everything: cargo test --workspace + web app tests
cargo test -p void-css          # one crate
cargo test -p void-browser --test e2e   # real window + GPU, driven by --auto
```

| Suite | What it covers |
|---|---|
| `void-dom` | tree operations, focus, event paths |
| `void-css` | parsing, nesting, selectors, specificity, cascade, `var()`, `oklch()`, shorthands, `overflow` |
| `void-layout` | flex, grid, border-box, inline text, blockification, nowrap, inputs, scroll containers in flex |
| `void-paint` | clipping, scroll offsets and ranges, nested scrollers, layer balance |
| `void-js` | TSX, URL loading, HTML entry, signals, `For`/`Show`, events, timers, privileges |
| `void-browser` (e2e) | tabs, history, address bar, links, demo app, legacy refusal, wheel and keyboard scrolling, zoom, Ctrl shortcuts, GPU screenshots |
| `web-react` | the React baseline (vitest + Testing Library) |

E2E tests open a real window. On machines without a GPU, set `VOID_SKIP_E2E=1`.

`--auto` scripts support these steps:

| Step | What it does |
|---|---|
| `click x y`, `click-text <text>`, `move x y`, `move-on <text>` | Pointer input; `<text>` finds the visible element showing it |
| `type <text>`, `key <spec>` | Keyboard; `key` takes `ctrl+t`, `enter`, `up`, `down`, `pageup`, `pagedown`, `home`, `end`, `space`, `plus`, `minus`, … |
| `scroll x y dy [ctrl\|shift]` | Wheel at (x, y); positive `dy` scrolls down |
| `scroll-on <text> \| <dy> [ctrl\|shift]` | Wheel over the element showing `<text>` |
| `nav <url>`, `wait <ms>`, `shot <file.png>`, `quit` | Navigation, timing, screenshots |
| `expect <text>`, `expect-title`, `expect-tabs <n>`, `expect-zoom <percent>` | Checks |
| `expect-hidden <text>` | The text is on the page but not visible (scrolled away or clipped) |
| `where <text>` | Prints the window position of `<text>`, for writing tests |

`key ctrl+<letter>` sends the control character as the key, as Windows does.
A failed `expect` exits with code 1.

## Windows installer

```bash
cargo install cargo-packager --locked   # once
cargo packager -p void-browser --release
```

This produces `dist/Void_<version>_x64-setup.exe`: an NSIS installer
(per-user, no admin rights) with a Start-menu shortcut and an uninstaller.
The demo app is compiled into the binary as `void://demo`, so an installed
copy works without the repo. CI builds the installer on every `v*` tag and
attaches it to the GitHub release.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.
