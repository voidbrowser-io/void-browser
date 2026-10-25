---
name: void-architecture
description: Architecture rules and design principles for the Void Browser engine (Rust crates void-dom, void-css, void-layout, void-paint, void-js and apps/browser). Use this skill before designing or reviewing any change to Void — adding a CSS property, a DOM/JS API, a browser feature, a new crate or module, refactoring, or judging whether code follows SOLID, GRASP or design patterns. Also use it when someone asks "where should this code go", "is this good design", or wants an architecture review of Void code, even if they don't name the skill.
---

# Void architecture

Void is a modern-only browser engine. Its value comes from two things that are
easy to erode one convenient change at a time: **a small, layered engine** and
**the Void Profile** (no legacy). This skill is the checklist for protecting both.

Read `CLAUDE.md` at the repo root first for the layout and commands.

## 1. Layering

```
void-dom  ←  void-css  ←  void-layout  ←  void-paint
    ↑            ↑
    └── void-js ─┘            apps/browser → all crates
```

- Edges only point left/up as drawn. A lower crate never learns about a higher one:
  the DOM knows nothing about styles, CSS nothing about layout, layout nothing about
  the GPU, and no crate knows about windows, tabs or `winit`.
- `void-js` talks to the host only through `HostCmd` and `Shared`. It does not know
  about tabs, the window or the browser UI. New host features = a new `HostCmd`
  variant handled in `apps/browser`.
- `apps/browser` is the only place allowed to own platform concerns: winit, wgpu
  surfaces, files on disk outside page loading, clipboard, installer.
- If a change seems to need an upward dependency, the data is in the wrong place.
  Pass it down as a parameter or move the type to the lower crate.

## 2. Responsibilities (who is the Information Expert)

| Question | Owner |
|---|---|
| Tree shape, focus, hover, input values, event paths | `void-dom::Dom` |
| What a CSS value means, which rule wins | `void-css` (`compute_style`, `Selector`) |
| Where a box goes, how big text is | `void-layout` |
| What pixels to draw, what is clickable | `void-paint` (`Painter`, `HitBox`) |
| Running code, loading modules, page → host requests | `void-js::Engine` |
| Tabs, history, routing input to a page, shortcuts | `apps/browser` (`App`, `View`) |
| Reactivity, JSX, `For`/`Show` | `runtime/void.js` |

Put new logic with the type that already has the data it needs. A function that
takes a `&Dom` and five fields of `Style` probably belongs on `Style`.

## 3. Design principles, translated for Rust

These are the classic principles, phrased for this codebase. Apply the intent, not
the ceremony — Void is a small engine and indirection has a cost.

**GRASP**
- *Information Expert*: see the table above.
- *Low Coupling*: crates expose plain data (`Style`, `Frame`, `HostCmd`) rather than
  callbacks into each other. Prefer passing values over sharing `Rc<RefCell<…>>`;
  `Rc<RefCell>` is reserved for state that JS closures must mutate (`Dom`, `Shared`).
- *High Cohesion*: one crate, one stage of the pipeline. A module that grows a second
  job (e.g. paint starting to compute layout) should be split.
- *Protected Variations*: fast-moving dependencies (taffy, parley, vello, oxc,
  rquickjs) are wrapped at one spot each — `to_taffy`, `text_layout`, `Painter`,
  `transpile`, `Engine`. Never leak their types across crate APIs unless the
  crate's whole purpose is that dependency.
- *Controller*: `App` routes input; `View` owns one page. Don't put tab logic in `View`
  or page logic in `App`.

**SOLID**
- *Single responsibility*: by pipeline stage (see layering), not "one struct per noun".
- *Open/closed*: extend with new enum variants and `match` arms (`Display`, `Step`,
  `HostCmd`, `Track`). The compiler's exhaustiveness check is the safety net —
  avoid `_ =>` arms on our own enums so new variants force a decision.
- *Liskov / Interface segregation*: if a trait is introduced, keep it small and
  implemented by more than one type. Do not add a trait with one implementation
  "for testability" — test the concrete type.
- *Dependency inversion*: lower crates take inputs as parameters (`BuiltinPages` is a
  `fn` pointer, not a dependency on the browser app). Follow that pattern.

**Design patterns (GoF) — what fits Rust here**
- Fits naturally: Builder (`TaffyTree`, `RangedBuilder`), Command (`HostCmd`, `Step`),
  Observer (signals), Composite (DOM/layout trees), Interpreter (`--auto` scripts),
  Facade (`Engine`, `View`).
- Usually wrong here: Visitor (use `match` on enums), Abstract Factory, Singleton
  (no globals; pass context), deep Template Method hierarchies. If a pattern needs
  `dyn Trait` plus `Box` plus a registry, look for an enum first.

## 4. The Void Profile gate

Before adding any web-facing feature (CSS property, selector, DOM/JS API, HTML
behaviour), answer:

1. Does it exist only for backwards compatibility? → It does not go in.
2. Is there a modern equivalent already supported? → Use that instead.
3. Does it need a second code path for "old" behaviour (quirks, prefixes, aliases)?
   → Only the modern path goes in. Unknown values are ignored, never guessed.
4. Can it be specified in one sentence that a page author would understand?

Accepted features are listed in `CLAUDE.md` and the README "Void profile" table;
update both when the profile changes.

## 5. Security boundaries

- Pages are untrusted. A page may only: mutate its own DOM, set its title,
  navigate its own tab, use timers. Anything else is a privilege.
- Privileged = `pages::privileged(url)` (today: `void://chrome`, `void://newtab`).
  Adding a privileged capability or a new privileged page needs explicit sign-off.
- The loader must resolve relative imports against the importing module's URL and
  must not let `http(s)` pages read `file://` or `void://` sources.

## 6. Tests that a change must bring

| Change | Required test |
|---|---|
| Bug fix | a test that fails without the fix |
| CSS value/selector | `crates/void-css/tests/css.rs` |
| Layout behaviour | `crates/void-layout/tests/layout.rs` (assert boxes) |
| JS/runtime/loader | `crates/void-js/tests/engine.rs` (boot a temp site) |
| Browser feature | `apps/browser/tests/e2e.rs` `--auto` scenario |
| Visual change | an `--auto ... shot` you actually looked at |

## 7. Review checklist

When reviewing Void code, report findings under these headings, most severe first:

1. **Layering** — new upward edges, platform code outside `apps/browser`.
2. **Profile** — legacy features or compatibility paths sneaking in.
3. **Security** — privilege, loader and URL handling changes.
4. **Ownership** — logic living away from its Information Expert; leaked dependency types.
5. **Tests** — missing or non-failing-before-fix tests (section 6).
6. **Simplicity** — indirection without a second user, dead code, `_ =>` on own enums.

Each finding: file:line, what is wrong, why it matters here, the smallest fix.
