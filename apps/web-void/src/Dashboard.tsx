import { signal, computed, For, onCleanup } from "void";

type Todo = { id: number; text: string; done: ReturnType<typeof signal<boolean>> };

function Clock() {
  const time = signal(new Date());
  const id = setInterval(() => time.set(new Date()), 1000);
  onCleanup(() => clearInterval(id));
  const pad = (n: number) => String(n).padStart(2, "0");
  return (
    <div class="card clock">
      <div class="label">Local time</div>
      <div class="big">
        {() => `${pad(time().getHours())}:${pad(time().getMinutes())}:${pad(time().getSeconds())}`}
      </div>
      <div class="muted">setInterval → signal → one text node</div>
    </div>
  );
}

function Counter() {
  const count = signal(0);
  const double = computed(() => count() * 2);
  return (
    <div class="card">
      <div class="label">Counter</div>
      <div class="big">{count}</div>
      <div class="muted">double = {double}</div>
      <div class="row">
        <button onClick={() => count.update((c) => c - 1)}>−1</button>
        <button class="primary" onClick={() => count.update((c) => c + 1)}>+1</button>
        <button onClick={() => count.set(0)}>Reset</button>
      </div>
    </div>
  );
}

function Todos() {
  let nextId = 1;
  const draft = signal("");
  const todos = signal<Todo[]>([
    { id: nextId++, text: "Ship the Void prototype", done: signal(true) },
    { id: nextId++, text: "Benchmark signals vs React", done: signal(false) },
    { id: nextId++, text: "Push the monorepo to GitHub", done: signal(false) },
  ]);
  const left = computed(() => todos().filter((t) => !t.done()).length);

  const add = () => {
    const text = draft().trim();
    if (!text) return;
    todos.update((list) => [...list, { id: nextId++, text, done: signal(false) }]);
    draft.set("");
  };

  return (
    <div class="card todos">
      <div class="label">Todos · {left} left</div>
      <div class="row">
        <input
          placeholder="What needs to be done?"
          value={draft}
          onInput={(e: { value: string }) => draft.set(e.value)}
          onKeyDown={(e: { key: string }) => e.key === "Enter" && add()}
        />
        <button class="primary" onClick={add}>Add</button>
      </div>
      <div class="list">
        <For each={todos}>
          {(todo: Todo) => (
            <div class={() => (todo.done() ? "todo done" : "todo")} onClick={() => todo.done.update((d) => !d)}>
              <div class="check">{() => (todo.done() ? "✓" : "")}</div>
              <span>{todo.text}</span>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}

function Stats() {
  const items = [
    { k: "Engine", v: "Rust + wgpu" },
    { k: "JS", v: "QuickJS" },
    { k: "TSX", v: "oxc, per-file" },
    { k: "Layout", v: "Taffy" },
  ];
  return (
    <div class="stats">
      {items.map((s) => (
        <div class="stat">
          <div class="muted">{s.k}</div>
          <div class="stat-value">{s.v}</div>
        </div>
      ))}
    </div>
  );
}

export function Dashboard() {
  return (
    <div class="dashboard">
      <div class="hero">
        <h1>What if the web had no legacy?</h1>
        <p class="muted">
          Flexbox, grid, nesting, <code>oklch()</code>, custom properties and signals.
          No floats, no quirks mode, no virtual DOM. Save a file and it is on screen.
        </p>
      </div>
      <Stats />
      <div class="grid">
        <Counter />
        <Clock />
        <Todos />
      </div>
    </div>
  );
}
