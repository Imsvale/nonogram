# nonogram-human-by-ai

Technique-driven constraint propagation solver — "everything the user would have deduced by hand, implemented by Claude." No backtracking, no search. If the current set of techniques cannot fully determine the grid, the solver returns `SolutionState::Partial` with the partial result.

This content used to live at `nonogram-propagation`. Moved here per `discussions/SolverTaxonomy.md`: "propagation" is the generic CSP term and doesn't imply "restricted to named, human-legible techniques" — that restriction is this crate's actual identity, so the generic name was freed for the new crate extracting the *unrestricted* propagation machinery out of `nonogram-graph-search`. Per that document's identity model, Claude-Propagation's identity stayed bound to the `nonogram-propagation` path rather than following this content here — so this crate's owner is a fresh, unassigned identity, not a renamed Claude-Propagation (see the document for the full reasoning).

The solver struct was renamed `PropagationSolver` → `HumanByAiSolver` after the fact, once it became clear both crates having a struct with the identical name was purely an artifact of the split (not a deliberate choice), matching the existing `GraphSearchSolver`/`HumanSolver`/`PropagationSolver` (category 3) convention.

## Approach

Each solving technique is a *pass function* with signature:
```rust
fn technique(cells: &mut [CellState], meta: &mut LineMeta) -> Result<bool, ()>
```
`Ok(true)` = at least one `Unknown` cell was changed. `Ok(false)` = no change. `Err(())` = logical contradiction.

Passes are applied in a queue loop: when a line changes, all perpendicular lines are re-enqueued. The queue drains when no pass can make further progress.

## Pass Functions (in dependency order)

| Function | What it does |
|---|---|
| `trim` | Shrinks the working window past confirmed `Empty` cells at either edge |
| `preprocess` | Handles empty-clue lines and zero-slack lines (only one valid placement) |
| `overlap` | Fills cells guaranteed `Filled` by any valid placement of the clues |
| `edge_forcing` | When a clue touches the window edge, fills the rest of that run |
| `completed_run` | Claims a fully-bounded run that matches an edge clue; removes that clue |
| `extend` | Anchors partial runs to the single clue that can cover them |
| `delim` | Narrows possible span of a single-clue line using existing filled cells |
| `split` | Divides at interior `Empty` barriers; uses forward/backward DP to assign clues to sub-windows |

`run_core_passes` = all except `split`. `run_passes_on_line` = core + split.

## Key Types (private to this crate)

### `LineMeta`
Working window `[start, end)` into the line's cell array, plus the current clue list. `completed_run` and `split` may shrink the clue list and advance the window bounds. `complete` flag short-circuits all passes once set.

### `SolverState<'a>`
Borrows `&'a Puzzle` plus owns `cells: Vec<CellState>`, `row_meta`, `col_meta`. Access helpers: `row_slice`, `row_slice_mut`, `col_vec`, `write_col`.

## Public API

```rust
pub struct HumanByAiSolver;
impl Solver for HumanByAiSolver { ... }
```

## Termination Invariant

A pass may only return `Ok(true)` when at least one `Unknown` cell was set. Since the cell count is finite and cells never revert to `Unknown`, the queue must eventually drain.

## Column Copy–Modify–Writeback

Columns are non-contiguous in the flat row-major grid. All column passes operate on a `Vec<CellState>` copy (`col_vec`), then scatter back with `write_col`. Change detection for columns uses an explicit before/after comparison of cell values, not the pass return value (which includes window-bound changes that do not affect cells).

## Authorship Note

This solver was implemented with AI assistance. The pass logic is correct and well-tested, but the structure may not fully reflect the author's intended human-logic approach. See `nonogram-human` for the fully author-driven implementation.

## Adding a New Pass

1. Write a function with the signature above.
2. Add it to `run_core_passes` (or `run_passes_on_line` if it calls `run_core_passes` internally, like `split`).
3. Obey the termination invariant.
4. Add a puzzle to `puzzles/` that requires the technique.

## Ownership

This crate's owner is **unassigned** as of this move — see the identity-model correction in `discussions/SolverTaxonomy.md` (Claude-Propagation stayed bound to the `nonogram-propagation` path and did not follow this content here). For cross-crate changes or design questions involving `nonogram-core`, coordinate with **Claude-Main**.

## Git Commits

**Before committing, verify scope.**

- Change is entirely within this crate and the workspace compiles without touching anything else → commit from here.
- Change was triggered by a `nonogram-core` update, touches files in other crates, or modifies the workspace `Cargo.toml` → defer to **Claude-Main** (workspace root).

This rule applies even if the user explicitly asks this Claude to commit. Git has no technical barrier — any Claude can commit from any working directory in the repo — but a partial cross-boundary commit leaves the workspace broken. When in doubt, check scope first.
