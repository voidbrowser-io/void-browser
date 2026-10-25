import { render } from "void";
import "./newtab.css";

type Demo = { title: string; desc: string; url: string };

const browser = (globalThis as any).__browser;
const info: { demos: Demo[] } = JSON.parse(browser.info() || "{}");

function NewTab() {
  return (
    <div class="page">
      <div class="hero">
        <div class="logo" />
        <h1>Void</h1>
        <p class="muted">The web, without the legacy.</p>
      </div>
      <input
        class="search"
        placeholder="Enter a URL — file, http(s) or void://"
        onKeyDown={(e: any) => e.key === "Enter" && browser.navigate(e.value)}
      />
      <div class="cards">
        {(info.demos ?? []).map((d) => (
          <a class="card" href={d.url}>
            <div class="card-title">{d.title}</div>
            <div class="muted">{d.desc}</div>
            <div class="card-url">{d.url}</div>
          </a>
        ))}
      </div>
      <div class="tips">
        Ctrl+T new tab · Ctrl+W close · Ctrl+L address bar · Ctrl+Tab switch · Alt+← back · F5 reload
      </div>
    </div>
  );
}

render(NewTab);
