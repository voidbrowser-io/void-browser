import { useState } from "react";
import { Dashboard } from "./Dashboard.tsx";
import { Bench } from "./Bench.tsx";

type Tab = "dashboard" | "bench";

export function App() {
  const [tab, setTab] = useState<Tab>("dashboard");
  const [dark, setDark] = useState(true);

  const TabButton = (props: { id: Tab; label: string }) => (
    <button className={tab === props.id ? "tab active" : "tab"} onClick={() => setTab(props.id)}>
      {props.label}
    </button>
  );

  return (
    <div className={dark ? "app dark" : "app"}>
      <header className="topbar">
        <div className="brand">
          <div className="logo" />
          <span>Void</span>
          <span className="badge">React + Vite baseline</span>
        </div>
        <nav className="tabs">
          <TabButton id="dashboard" label="Dashboard" />
          <TabButton id="bench" label="Benchmark" />
        </nav>
        <button className="ghost" onClick={() => setDark((d) => !d)}>
          {dark ? "☀ Light" : "☾ Dark"}
        </button>
      </header>
      <main className="content">{tab === "dashboard" ? <Dashboard /> : <Bench />}</main>
    </div>
  );
}
