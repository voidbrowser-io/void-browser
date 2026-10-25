import { useState } from "react";
import { flushSync } from "react-dom";

const ROWS = 40;
const COLS = 25;

/** 1 000 cells in one state array. Every update re-renders and diffs the grid. */
export function Bench() {
  const [cells, setCells] = useState(() => Array.from({ length: ROWS * COLS }, (_, i) => i % 100));
  const [result, setResult] = useState("Press a button to run");

  const run = (label: string, rounds: number, share: number) => {
    const t0 = performance.now();
    for (let r = 0; r < rounds; r++) {
      // flushSync forces each round to commit, like Void's synchronous batches.
      flushSync(() =>
        setCells((prev) => prev.map((v) => (Math.random() < share ? Math.floor(Math.random() * 100) : v))),
      );
    }
    const ms = performance.now() - t0;
    setResult(`${label}: ${ms.toFixed(1)} ms (${(ms / rounds).toFixed(2)} ms per round)`);
  };

  return (
    <div className="bench">
      <div className="card">
        <div className="label">React benchmark · {ROWS * COLS} cells</div>
        <div className="row">
          <button className="primary" onClick={() => run("1 round, 10%", 1, 0.1)}>Update 10%</button>
          <button onClick={() => run("100 rounds, 10%", 100, 0.1)}>100 × 10%</button>
          <button onClick={() => run("100 rounds, 100%", 100, 1)}>100 × all</button>
        </div>
        <div className="result">{result}</div>
        <div className="muted">Measures React render + reconcile + DOM commit (no layout/paint).</div>
      </div>
      <div className="cells">
        {cells.map((c, i) => (
          <div key={i} className={c > 50 ? "cell hot" : "cell"}>{c}</div>
        ))}
      </div>
    </div>
  );
}
