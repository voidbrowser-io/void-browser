import { useEffect, useState } from "react";

type Todo = { id: number; text: string; done: boolean };

function Clock() {
  const [time, setTime] = useState(new Date());
  useEffect(() => {
    const id = setInterval(() => setTime(new Date()), 1000);
    return () => clearInterval(id);
  }, []);
  const pad = (n: number) => String(n).padStart(2, "0");
  return (
    <div className="card clock">
      <div className="label">Local time</div>
      <div className="big">{`${pad(time.getHours())}:${pad(time.getMinutes())}:${pad(time.getSeconds())}`}</div>
      <div className="muted">setInterval → setState → re-render + diff</div>
    </div>
  );
}

function Counter() {
  const [count, setCount] = useState(0);
  return (
    <div className="card">
      <div className="label">Counter</div>
      <div className="big">{count}</div>
      <div className="muted">double = {count * 2}</div>
      <div className="row">
        <button onClick={() => setCount((c) => c - 1)}>−1</button>
        <button className="primary" onClick={() => setCount((c) => c + 1)}>+1</button>
        <button onClick={() => setCount(0)}>Reset</button>
      </div>
    </div>
  );
}

let nextId = 1;

function Todos() {
  const [draft, setDraft] = useState("");
  const [todos, setTodos] = useState<Todo[]>(() => [
    { id: nextId++, text: "Ship the Void prototype", done: true },
    { id: nextId++, text: "Benchmark signals vs React", done: false },
    { id: nextId++, text: "Push the monorepo to GitHub", done: false },
  ]);
  const left = todos.filter((t) => !t.done).length;

  const add = () => {
    const text = draft.trim();
    if (!text) return;
    setTodos((list) => [...list, { id: nextId++, text, done: false }]);
    setDraft("");
  };
  const toggle = (id: number) => setTodos((list) => list.map((t) => (t.id === id ? { ...t, done: !t.done } : t)));

  return (
    <div className="card todos">
      <div className="label">Todos · {left} left</div>
      <div className="row">
        <input
          placeholder="What needs to be done?"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
        />
        <button className="primary" onClick={add}>Add</button>
      </div>
      <div className="list">
        {todos.map((todo) => (
          <div key={todo.id} className={todo.done ? "todo done" : "todo"} onClick={() => toggle(todo.id)}>
            <div className="check">{todo.done ? "✓" : ""}</div>
            <span>{todo.text}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function Stats() {
  const items = [
    { k: "Engine", v: "Chromium / Blink" },
    { k: "JS", v: "V8" },
    { k: "TSX", v: "Vite + esbuild" },
    { k: "Layout", v: "Blink LayoutNG" },
  ];
  return (
    <div className="stats">
      {items.map((s) => (
        <div className="stat" key={s.k}>
          <div className="muted">{s.k}</div>
          <div className="stat-value">{s.v}</div>
        </div>
      ))}
    </div>
  );
}

export function Dashboard() {
  return (
    <div className="dashboard">
      <div className="hero">
        <h1>What if the web had no legacy?</h1>
        <p className="muted">
          The same app as <code>web-void</code>, built the usual way: React, a virtual DOM, a bundler and
          node_modules. Use it as the baseline.
        </p>
      </div>
      <Stats />
      <div className="grid">
        <Counter />
        <Clock />
        <Todos />
      </div>
    </div>
  );
}
