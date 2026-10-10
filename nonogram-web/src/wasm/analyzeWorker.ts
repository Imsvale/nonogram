// Runs in a Web Worker so an expensive analyze() call (exhaustive search) can't freeze the
// page. One request at a time is fine — the main thread only ever has one Analyze in flight.
import init, { analyze } from "./pkg/nonogram_wasm.js";

export interface AnalyzeRequest {
  id: number;
  rowClues: number[][];
  colClues: number[][];
}

export interface WasmSolveStep {
  kind: "Logic" | "Trial";
  line: { axis: "row" | "col"; index: number } | null;
  cells_changed: { idx: number; state: number }[];
}

export interface AnalyzeResponse {
  id: number;
  // Field names as wasm-bindgen hands them back — snake_case kept as-is here;
  // src/wasm/analyze.ts translates to the app's own naming on the way out.
  result?: {
    solution_count: number;
    needs_guessing: boolean;
    grid: number[] | null;
    steps: WasmSolveStep[] | null;
  };
  error?: string;
}

let ready: Promise<unknown> | null = null;

self.onmessage = async (e: MessageEvent<AnalyzeRequest>) => {
  const { id, rowClues, colClues } = e.data;
  try {
    ready ??= init();
    await ready;
    const result = analyze({ row_clues: rowClues, col_clues: colClues });
    (self as unknown as Worker).postMessage({ id, result } satisfies AnalyzeResponse);
  } catch (err) {
    (self as unknown as Worker).postMessage({ id, error: String(err) } satisfies AnalyzeResponse);
  }
};
