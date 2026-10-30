# Adding the solver to a static site

How to put `nonogram-wasm` on a plain HTML page with no bundler and no build step, the
way a GitHub Pages site usually is. The worked example is
[Nonomaker](https://baronvonkaldo.github.io/Nonomaker/), a single-file nonogram editor.
It gains a live **solver check** that tells the author, while they paint, whether the
picture makes a puzzle with exactly one solution, and whether line logic alone can solve
it.

Everything below was run against a copy of Nonomaker's `index.html`, served over HTTP
and driven in headless Chromium. The results are in [step 5](#5-test-locally).

The finished layout of the site:

```
index.html                 (the existing page, edited)
solver/
  nonogram_wasm.js         (from the wasm-pack build)
  nonogram_wasm_bg.wasm    (from the wasm-pack build)
  analyze-worker.js        (new, step 2)
  LICENSE                  (copy of this repo's LICENSE)
```

## 1. Get the two solver files

Build them from this crate (needs Rust, the `wasm32-unknown-unknown` target and
[`wasm-pack`](https://rustwasm.github.io/wasm-pack/)):

```sh
cd nonogram-wasm
wasm-pack build --target web --out-dir pkg
```

If you don't have Rust, ask the maintainers of this repo for the two built files.

From `pkg/`, copy **only** `nonogram_wasm.js` and `nonogram_wasm_bg.wasm` into
`solver/`, side by side. The `.js` file finds the `.wasm` file relative to its own
location, so the two must stay in the same folder.

- **Don't copy the `pkg/` folder as a whole.** wasm-pack writes a `.gitignore`
  containing `*` into it, so git silently ignores everything inside. The site would then
  deploy without the solver, and every check would show "Solver error".
- **The two files are a matched pair.** To update, rebuild and replace both together.
  Never mix a new `.js` with an old `.wasm` or the other way round.
- **The solver is MIT-licensed.** Keep a copy of [`LICENSE`](../LICENSE) in `solver/`.

## 2. The worker

`analyze()` is synchronous and does an exhaustive search. On an ambiguous picture it can
run for a very long time (see [limitations](#limitations)), and nothing can interrupt it
from inside. So it runs in a Web Worker, and the page stops a stale or runaway check by
terminating the worker.

`solver/analyze-worker.js`:

```js
// Runs the solver off the main thread: analyze() is synchronous and can take a long time on
// an ambiguous picture, so the page terminates this worker instead of waiting for it.
import init, { analyze } from "./nonogram_wasm.js";

const ready = init();

self.onmessage = async (e) => {
  const { id, rowClues, colClues } = e.data;
  try {
    await ready;
    const result = analyze({ row_clues: rowClues, col_clues: colClues });
    self.postMessage({ id, result });
  } catch (err) {
    self.postMessage({ id, error: String(err) });
  }
};
```

This is a *module* worker (it uses `import`). The page's own script doesn't have to be a
module: Nonomaker's script is a classic `<script>` with everything inside one function
wrapper, and it needs no restructuring.

## 3. Wire it into the page

**What the solver needs.** Two arrays of clue lists: one list per row (top to bottom)
and one per column (left to right). Each list holds the run lengths in reading order,
and an empty line is `[]`. Nonomaker's `computeRowClues()` and `computeColClues()`
already return exactly that, computed from the painted grid.

**Where to call it.** Call `scheduleAnalysis()` wherever the grid changes. In
Nonomaker, that's two places:

- at the end of `toggleCell(r, c, val)`, which runs for every cell painted, including
  each cell of a drag;
- at the end of `rebuildAll()`, which covers resizing, clearing, inverting, the text
  import and the screenshot import, plus the first render at startup.

Calling it on every painted cell is fine. It waits 300 ms for painting to pause, and
each new call cancels the previous one.

**The panel.** Add this after the last panel:

```html
<div class="panel">
  <h2>7 — Solver check</h2>
  <p class="meta" id="solverStatus" style="margin:0 0 6px;font-size:13px;color:var(--ink);">—</p>
  <p class="hint" id="solverDetail" style="margin:0;"></p>
</div>
```

**The code.** Add this inside the script's function wrapper, just before the closing
`makeGrid(W,H); rebuildAll();`. It must come before that first `rebuildAll()`, because
the `const` declarations have to run before `scheduleAnalysis()` is first called.

```js
// ---------------- Solver check (nonogram-wasm) ----------------
// analyze() is an exhaustive search: it can't be interrupted from inside, so a stale or
// runaway check is stopped by terminating its worker.
const ANALYZE_DELAY_MS = 300;     // wait for painting to pause before checking
const ANALYZE_TIMEOUT_MS = 10000;
const solverStatus = document.getElementById('solverStatus');
const solverDetail = document.getElementById('solverDetail');
let solverWorker = null, solverBusy = false;
let solverDelay = 0, solverTimeout = 0, solverReqId = 0, solverStarted = 0;

function showSolver(status, detail){
  solverStatus.textContent = status;
  solverDetail.textContent = detail || '';
}

function cancelAnalysis(){
  clearTimeout(solverDelay);
  clearTimeout(solverTimeout);
  if(solverBusy && solverWorker){ solverWorker.terminate(); solverWorker = null; }
  solverBusy = false;
}

function getSolverWorker(){
  if(!solverWorker){
    solverWorker = new Worker('solver/analyze-worker.js', { type: 'module' });
    solverWorker.onmessage = onSolverMessage;
    solverWorker.onerror = (e)=>{
      clearTimeout(solverTimeout);
      solverBusy = false;
      solverWorker.terminate();
      solverWorker = null;
      showSolver('Solver unavailable', e.message);
    };
  }
  return solverWorker;
}

function scheduleAnalysis(){
  cancelAnalysis();
  showSolver('Checking…');
  solverDelay = setTimeout(runAnalysis, ANALYZE_DELAY_MS);
}

function runAnalysis(){
  const id = ++solverReqId;
  solverBusy = true;
  solverStarted = performance.now();
  solverTimeout = setTimeout(()=>{
    cancelAnalysis();
    showSolver('Gave up after ' + ANALYZE_TIMEOUT_MS/1000 + ' s',
      'Very ambiguous pictures can have too many solutions to count.');
  }, ANALYZE_TIMEOUT_MS);
  getSolverWorker().postMessage({ id, rowClues: computeRowClues(), colClues: computeColClues() });
}

function onSolverMessage(e){
  const { id, result, error } = e.data;
  if(id !== solverReqId) return;
  clearTimeout(solverTimeout);
  solverBusy = false;
  const ms = Math.round(performance.now() - solverStarted);
  if(error){ showSolver('Solver error', error); return; }
  const n = result.solution_count;
  if(n === 1){
    showSolver('Unique solution',
      (result.needs_guessing ? 'Needs guessing: line logic alone gets stuck.'
                             : 'Solvable with line logic alone.') + ` (${ms} ms)`);
  } else if(n >= 2){
    showSolver(`Not unique — ${n} solutions`, `(${ms} ms)`);
  } else {
    showSolver('No solution', `(${ms} ms)`);
  }
}
```

How it behaves:

- **An idle worker is reused**, so the wasm module loads once, not on every check.
- **A worker that is still busy gets terminated** when the grid changes again or the
  timeout expires. The next check then starts a fresh worker, which reloads the wasm
  (tens of milliseconds).
- **Each request carries an `id`**, so a late answer to an outdated grid is ignored.

## 4. Reading the result

| `solution_count` | `needs_guessing` | Meaning for the author |
|---|---|---|
| `1` | `false` | A good puzzle: unique, and line logic alone solves it. |
| `1` | `true` | Unique, but a solver has to guess somewhere. Many puzzle sites reject these. |
| `≥ 2` | — | Not a valid puzzle: the clues fit more than one picture. |
| `0` | — | Can't happen when the clues come from a painted grid (the painting is always a solution). |
| *(timed out)* | — | Unknown. In practice: so ambiguous there are too many solutions to count. |

The full result also includes one solution `grid` and its step-by-step `steps`; see
[README.md](README.md#analyzeclues) for the complete shape.

## 5. Test locally

Serve the folder over HTTP. Opening `index.html` straight from disk (`file://`) won't
work, because browsers block module workers and the `.wasm` fetch there.

```sh
python -m http.server 8000
# open http://localhost:8000/
```

The server should send `.wasm` as `application/wasm`. Python's server and GitHub Pages
both do. With any other type, the module still loads, more slowly, and logs a console
warning.

Results from the test run (Chromium, a copy of Nonomaker with the changes above):

| Picture | Status shown | Time |
|---|---|---|
| Empty 15×15 board (startup) | Unique solution · Solvable with line logic alone | 17 ms (includes loading the wasm) |
| 3×5 arrow | Unique solution · Solvable with line logic alone | 1 ms |
| 2×2 diagonal (`#.` / `.#`) | Not unique — 2 solutions | 1 ms |
| 8×8 picture that needs guessing | Unique solution · Needs guessing: line logic alone gets stuck. | 2 ms |
| 30×30 board with a few scattered cells | Gave up after 10 s | 10 s |
| The arrow again, right after that timeout | Unique solution · Solvable with line logic alone | 77 ms (fresh worker) |
| `nonogram_wasm_bg.wasm` missing | Solver error · …HTTP status code is not ok | — |

No console errors or warnings appeared in any of the runs that had both files present.

The 8×8 picture, for reproducing that row:

```
#.##...#
..#.....
##...###
#..##...
####.##.
....#..#
...####.
##.#..#.
```

## 6. Deploy

Commit `index.html` and the four files in `solver/`, and push. GitHub Pages serves them
as they are. The worker path `solver/analyze-worker.js` is relative, so it works under a
project site's sub-path such as `/Nonomaker/`. Over the wire the `.wasm` is about 55 KB,
since GitHub Pages gzips it.

## Limitations

These come from the current API, not from the page code:

- **No cap on the solution count.** `analyze()` keeps every solution it finds, each with
  its own grid and step trace. On sparse or ambiguous pictures it can run, and use
  memory, until the page's timeout stops it. For an editor, "unique or not" only needs
  the search to stop at the second solution, which would make those cases fast. The
  timeout covers this for now.
- **Only the first solution comes back.** That means the page can't yet show *where* a
  puzzle is ambiguous. Returning two solutions would let it highlight the cells where
  they differ.
- **No difficulty rating.** The workspace has a difficulty rater (`nonogram-difficulty`),
  but it isn't exposed through the wasm interface yet.
