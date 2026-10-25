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
`Ctrl+A/C/V` in inputs.

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

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.
