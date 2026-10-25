// The browser UI — tabs, toolbar, address bar — is itself a Void page.
import { signal, render, focus } from "void";
import "./chrome.css";

type Tab = { title: string; url: string; error: boolean };
type State = { tabs: Tab[]; active: number; canBack: boolean; canForward: boolean; zoom: number };

const browser = (globalThis as any).__browser;
const state = signal<State>({ tabs: [], active: 0, canBack: false, canForward: false, zoom: 100 });
let urlInput: unknown = null;

// The Rust host pushes state here and asks for address-bar focus (Ctrl+L).
(globalThis as any).__browser_state = (json: string) => state.set(JSON.parse(json));
(globalThis as any).__browser_focus_url = () => focus(urlInput);

const current = () => state().tabs[state().active];

function TabStrip() {
  return (
    <div class="strip">
      {() =>
        state().tabs.map((t, i) => (
          <div class={i === state().active ? "tab active" : "tab"} onClick={() => browser.activate(i)}>
            <div class={t.error ? "dot err" : "dot"} />
            <span class="title">{t.title}</span>
            <div
              class="close"
              onClick={(e: any) => {
                e.stopPropagation();
                browser.closeTab(i);
              }}
            >
              ×
            </div>
          </div>
        ))
      }
      <div class="add" onClick={() => browser.newTab()}>+</div>
    </div>
  );
}

function Toolbar() {
  return (
    <div class="toolbar">
      <button class={() => (state().canBack ? "nav" : "nav off")} onClick={() => browser.back()}>←</button>
      <button class={() => (state().canForward ? "nav" : "nav off")} onClick={() => browser.forward()}>→</button>
      <button class="nav" onClick={() => browser.reload()}>⟳</button>
      <input
        class="url"
        ref={(n: unknown) => (urlInput = n)}
        placeholder="Enter a URL — file, http(s) or void://"
        value={() => current()?.url ?? ""}
        onKeyDown={(e: any) => e.key === "Enter" && browser.navigate(e.value)}
      />
      {() => (state().zoom !== 100 ? <div class="zoom">{state().zoom}%</div> : null)}
      <div class="badge">Void</div>
    </div>
  );
}

render(() => (
  <div class="chrome">
    <TabStrip />
    <Toolbar />
  </div>
));
