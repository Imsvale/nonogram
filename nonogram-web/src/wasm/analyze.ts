// Main-thread side of the Analyze button: owns the worker (created lazily, reused across
// calls) and turns its message-passing into a Promise-returning call.
import type { AnalyzeRequest, AnalyzeResponse } from "./analyzeWorker";
import type { Grid } from "../core/types";

/** One step of a solve trace, for the solution viewer. `idx` is a flat row-major index into
 *  a grid the same size as the puzzle, matching `Grid`'s own indexing. */
export interface SolveStep {
  /** "Logic" (pure propagation) or "Trial" (a branch/guess) — see nonogram-wasm's `step_kind`. */
  kind: "Logic" | "Trial";
  line: { axis: "row" | "col"; index: number } | null;
  cellsChanged: { idx: number; state: number }[];
}

export interface AnalyzeResult {
  /** 0 = unsolvable, 1 = unique, ≥2 = ambiguous. */
  solutionCount: number;
  /** false iff propagation alone (no branching/trialing) resolved the puzzle either way. */
  needsGuessing: boolean;
  /** The first solution found, if any (null iff solutionCount === 0). */
  grid: Grid | null;
  /** That solution's step trace (null under the same condition as `grid`). */
  steps: SolveStep[] | null;
}

let worker: Worker | null = null;
let nextId = 1;
const pending = new Map<number, { resolve: (r: AnalyzeResult) => void; reject: (e: unknown) => void }>();

function getWorker(): Worker {
  if (worker) return worker;
  worker = new Worker(new URL("./analyzeWorker.ts", import.meta.url), { type: "module" });
  worker.onmessage = (e: MessageEvent<AnalyzeResponse>) => {
    const { id, result, error } = e.data;
    const p = pending.get(id);
    if (!p) return;
    pending.delete(id);
    if (error) { p.reject(new Error(error)); return; }
    if (!result) { p.reject(new Error("worker returned neither a result nor an error")); return; }
    p.resolve({
      solutionCount: result.solution_count,
      needsGuessing: result.needs_guessing,
      grid: result.grid ? Uint8Array.from(result.grid) : null,
      steps: result.steps
        ? result.steps.map((s) => ({
            kind: s.kind,
            line: s.line,
            cellsChanged: s.cells_changed,
          }))
        : null,
    });
  };
  return worker;
}

export function analyzePuzzle(rowClues: number[][], colClues: number[][]): Promise<AnalyzeResult> {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    getWorker().postMessage({ id, rowClues, colClues } satisfies AnalyzeRequest);
  });
}
