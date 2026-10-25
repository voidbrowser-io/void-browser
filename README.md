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
void apps/web-void/src/main.tsx --auto "click 120 480; type hello; key Enter; shot out.png; quit"
```

Scripted input and offscreen GPU screenshots, for tests and CI.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.
