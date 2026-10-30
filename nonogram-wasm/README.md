# nonogram-wasm

A thin [`wasm-bindgen`](https://rustwasm.github.io/wasm-bindgen/) adapter around
[`nonogram-graph-search`](../nonogram-graph-search), a best-first nonogram (picross)
solver. It contains no solving logic of its own — it just translates plain clue arrays
into a `Puzzle`, runs the solver, and translates the result back into plain JSON.

Two exports: `solve` (find one solution) and `analyze` (uniqueness, whether guessing is
required, and the winning solution's step-by-step trace).

## Building

```sh
cargo install wasm-pack   # once
wasm-pack build --target web --out-dir pkg
```

`--target web` produces an ES module (`pkg/nonogram_wasm.js`) that `fetch`es its own
`.wasm` binary — it works with Vite, webpack, or any modern bundler with no extra
plugin, and in a plain `<script type="module">` too. Other `wasm-pack` targets
(`--target bundler`, `--target nodejs`) work the same way if you need a different
loading story.

## Usage

```js
import init, { analyze } from "./pkg/nonogram_wasm.js";

await init(); // loads and instantiates the .wasm once

const result = analyze({
  row_clues: [[2], []],
  col_clues: [[1], [1]],
});
```

Both functions are synchronous and CPU-bound — `analyze` in particular runs an
exhaustive search and can take a while on a large or highly ambiguous puzzle. Call them
from a Web Worker rather than the main thread if you don't want to block the page;
[`nonogram-web`'s own usage](../nonogram-web/src/wasm/) (`analyze.ts` +
`analyzeWorker.ts`) is a complete, working reference for that pattern.

For a plain static page with no build step (e.g. a GitHub Pages site), see
[STATIC_SITE.md](STATIC_SITE.md): a step-by-step walkthrough, including a timeout for
runaway searches.

## Cell encoding

Every grid in this API — input solutions, output grids, step deltas — is a flat,
row-major array of puzzle width × height, one byte per cell:

| Value | Meaning |
|---|---|
| `0` | Unknown |
| `1` | Filled |
| `2` | Empty (crossed out) |

Cell `(row, col)` is at flat index `row * width + col`.

## `solve(clues)`

Finds one solution via best-first search (propagation first, branching only if needed).

**Input** — `{ row_clues: number[][], col_clues: number[][] }`. Width and height are
taken from `col_clues.length` / `row_clues.length`, not passed separately, so they
can't disagree with the clues themselves.

**Output:**

```ts
{
  state: "Complete" | "Partial" | "Unsolvable" | "Aborted" | "Invalid";
  invalid_reason: string | null;  // only set when state === "Invalid"
  grid: number[];                 // length width*height; all 0 if unsolved
}
```

`"Invalid"` means the puzzle is structurally malformed (e.g. row/column clue totals
don't match) — no solve was attempted. The other four states mirror
`nonogram_core::SolutionState`.

**Example:**

```js
solve({ row_clues: [[2], []], col_clues: [[1], [1]] });
// => { state: "Complete", invalid_reason: null, grid: [1, 1, 2, 2] }
```

## `analyze(clues)`

Runs an *exhaustive* search — every solution, not just the first — so it can answer
three things in one call: is the solution unique, does finding it require
branching/guessing (as opposed to pure logical deduction), and what does a solution
look like, step by step.

**Input:** same as `solve`.

**Output:**

```ts
{
  solution_count: number;   // 0 = unsolvable, 1 = unique, >=2 = ambiguous
  needs_guessing: boolean;  // false iff propagation alone resolved it either way
  grid: number[] | null;    // first solution found; null iff solution_count === 0
  steps: Step[] | null;     // that solution's derivation; null under the same condition as grid
}

type Step = {
  kind: "Logic" | "Trial";                        // see caveat below
  line: { axis: "row" | "col"; index: number } | null;
  cells_changed: { idx: number; state: number }[]; // flat index + new cell value
};
```

`needs_guessing` is derived from the search's own node count (whether the branching
heap was ever touched) — a structural fact, not a guess. `grid` is populated even when
`solution_count >= 2`: an ambiguous puzzle still has *a* solution worth showing, just
not a unique one.

**Caveat on `Step.kind`:** this is a *display* classification, not a stable contract —
it's derived by checking for a literal `"branch"` marker in the solver's internal,
free-text step description, which exists for human-readable logging, not as a public
API. It's reliable today (graph-search's own tests cover it), but don't build anything
load-bearing on top of it beyond "label this step for a human."

**Example** (a 2×2 puzzle with a single filled cell per row/column — two valid
solutions, neither findable by propagation alone):

```js
analyze({ row_clues: [[1], [1]], col_clues: [[1], [1]] });
// => {
//   solution_count: 2,
//   needs_guessing: true,
//   grid: [1, 2, 2, 1],
//   steps: [
//     { kind: "Trial", line: { axis: "row", index: 0 }, cells_changed: [{ idx: 0, state: 1 }, { idx: 1, state: 2 }] },
//     { kind: "Logic", line: { axis: "col", index: 0 }, cells_changed: [{ idx: 2, state: 2 }] },
//     { kind: "Logic", line: { axis: "col", index: 1 }, cells_changed: [{ idx: 3, state: 1 }] },
//   ]
// }
```

## Errors

Malformed input (e.g. clues that fail to deserialize into the expected shape) rejects
the call with a JS exception carrying a plain string message — there's no structured
error type. A puzzle that's merely *unsolvable* or *invalid* is not an error; it's a
normal result (see `state`/`solution_count` above).
