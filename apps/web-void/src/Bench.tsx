import { signal, batch } from "void";

const ROWS = 40;
const COLS = 25;

/** 1 000 cells, each its own signal. Updating a cell touches one text node. */
export function Bench() {
  const cells = Array.from({ length: ROWS * COLS }, (_, i) => signal(i % 100));
  const result = signal("Press a button to run");

  const run = (label: string, rounds: number, share: number) => {
    const t0 = performance.now();
    for (let r = 0; r < rounds; r++) {
      batch(() => {
        for (let i = 0; i < cells.length; i++) {
          if (Math.random() < share) cells[i].set(Math.floor(Math.random() * 100));
        }
      });
    }
    const ms = performance.now() - t0;
    result.set(`${label}: ${ms.toFixed(1)} ms (${(ms / rounds).toFixed(2)} ms per round)`);
  };

  return (
    <div class="bench">
      <div class="card">
        <div class="label">Signals benchmark · {ROWS * COLS} cells</div>
        <div class="row">
          <button class="primary" onClick={() => run("1 round, 10%", 1, 0.1)}>Update 10%</button>
          <button onClick={() => run("100 rounds, 10%", 100, 0.1)}>100 × 10%</button>
          <button onClick={() => run("100 rounds, 100%", 100, 1)}>100 × all</button>
        </div>
        <div class="result">{result}</div>
        <div class="muted">Measures JS + DOM patch time. Layout and GPU paint are shown in the window title.</div>
      </div>
      <div class="cells">
        {cells.map((c) => (
          <div class={() => (c() > 50 ? "cell hot" : "cell")}>{c}</div>
        ))}
      </div>
    </div>
  );
}
