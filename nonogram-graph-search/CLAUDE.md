# nonogram-graph-search

Best-first graph search solver — the branching/heuristic layer of a
propagate-then-search solver. Each node in the search graph is a
fully-propagated partial grid. The priority queue (min-heap via `Reverse`)
always expands the node whose most-constrained unresolved line has the
fewest valid completions — the Minimum Remaining Values (MRV) heuristic.

**This crate no longer implements propagation itself.** It depends on
`nonogram-propagation` (category 3: full per-line deductive closure, no
branching) for the propagation cascade and for the completion-counting and
completion-enumeration primitives the MRV heuristic and branch-candidate
generation need. See `nonogram-propagation`'s own `CLAUDE.md` for why that
logic lives there and not here — short version: it's general nonogram line
deduction that only ever had one caller before the split, not
graph-search-specific logic that happened to also be reusable.

## Algorithm

1. **Propagate** the initial grid via `Propagator::propagate` (from
   `nonogram-propagation`) — cascades intersection-forcing across every
   line until stable, or returns false on contradiction.
2. If the grid is complete, return it. If propagation detected a conflict,
   return `Unsolvable`.
3. Otherwise, push the propagated grid onto the heap.
4. **Pop** the lowest-priority node (fewest completions for its tightest line).
5. Find the most-constrained unresolved line (`most_constrained`, using
   `nonogram_propagation::count_completions` per line); **enumerate** its
   completions (`nonogram_propagation::enumerate_completions`) — safe to
   materialize precisely because this is the *one* line chosen for having
   the fewest.
6. For each completion: apply it, propagate again, push survivors back onto
   the heap.
7. Repeat until a complete valid grid is found or the heap empties.

Most well-formed puzzles are solved entirely by propagation (0 heap nodes)
— this crate's own logic never runs at all for those. The heap is
exercised only when propagation stalls.

## Key Internal Types (all private)

### `SearchState`
Wraps a `nonogram_propagation::Propagator` (which owns `rows`, `cols`,
`row_clues: Vec<Vec<usize>>`, `col_clues: Vec<Vec<usize>>` — clues stored as
`usize` for arithmetic, converted from `u32` at construction). Adds the
branching-specific operations the Propagator doesn't own: `most_constrained`
(MRV line selection), `apply_line` (write a chosen branch into the grid),
and the `solve_counted`/`solve_all_counted` orchestration loops.

### `Node`
`{ min_count: usize, grid: Vec<CellState> }`. Ordered by `min_count` (lower
= pop first). Grid is `nonogram_core::CellState` directly — this crate has
no private cell type of its own anymore; that distinction lives in
`nonogram-propagation` now (see its `CLAUDE.md` for why).

## Public API

```rust
pub struct GraphSearchSolver;
impl Solver for GraphSearchSolver { ... }
impl ExhaustiveSolver for GraphSearchSolver { ... }
```

Plus `GraphSearchSolver::solve_with_progress` (opt-in progress observability
— see doc comments on `ProgressConfig`/`ProgressUpdate`). `SearchState::solve_counted`
also returns the heap push count internally — useful for benchmarking and
debugging, but not exposed through the trait.

## Design Constraints

- Clue arithmetic stays in `usize`. Do not change internal clue storage to
  `u32` — the cast at `Propagator::from_puzzle` (in `nonogram-propagation`)
  is the intended isolation point.
- Line-level deduction (counting, enumerating, per-cell forcing) belongs in
  `nonogram-propagation`, not here. If a change means touching per-line
  logic rather than heap/MRV/branching logic, it likely belongs in that
  crate instead — check before adding it here.
- The MRV priority and the orchestration loop are the core performance
  levers left in this crate. Profile before changing the heap ordering or
  `solve_counted`'s loop structure. (Propagation's own performance is
  `nonogram-propagation`'s concern — see the extraction history in
  `discussions/SolverTaxonomy.md` and `docs/graph-search-propagation-performance.md`
  for why that split happened and what was learned fixing it.)

## Ownership

This crate is owned by **Claude-graph**. For cross-crate changes or design
questions involving `nonogram-core` or `nonogram-propagation`, coordinate
via a `discussions/` doc — see `discussions/SolverTaxonomy.md` for the
taxonomy and reasoning behind this crate's current shape.

## Git Commits

**Before committing, verify scope.**

- Change is entirely within this crate and the workspace compiles without touching anything else → commit from here.
- Change was triggered by a `nonogram-core` or `nonogram-propagation` update, touches files in other crates, or modifies the workspace `Cargo.toml` → defer to **Claude-Main** (workspace root).

This rule applies even if the user explicitly asks this Claude to commit. Git has no technical barrier — any Claude can commit from any working directory in the repo — but a partial cross-boundary commit leaves the workspace broken. When in doubt, check scope first.
