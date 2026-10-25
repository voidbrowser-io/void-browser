import { signal, Show } from "void";
import { Dashboard } from "./Dashboard.tsx";
import { Bench } from "./Bench.tsx";

type Tab = "dashboard" | "bench";

export function App() {
  const tab = signal<Tab>("dashboard");
  const dark = signal(true);

  const TabButton = (props: { id: Tab; label: string }) => (
    <button class={() => (tab() === props.id ? "tab active" : "tab")} onClick={() => tab.set(props.id)}>
      {props.label}
    </button>
  );

  return (
    <div class={() => (dark() ? "app dark" : "app")}>
      <header class="topbar">
        <div class="brand">
          <div class="logo" />
          <span>Void</span>
          <span class="badge">pure .tsx · no bundler</span>
        </div>
        <nav class="tabs">
          <TabButton id="dashboard" label="Dashboard" />
          <TabButton id="bench" label="Benchmark" />
        </nav>
        <button class="ghost" onClick={() => dark.update((d) => !d)}>
          {() => (dark() ? "☀ Light" : "☾ Dark")}
        </button>
      </header>
      <main class="content">
        <Show when={() => tab() === "dashboard"} fallback={<Bench />}>
          <Dashboard />
        </Show>
      </main>
    </div>
  );
}
