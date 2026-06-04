# nonogram-graph-search

Pure best-first graph search solver. Each node in the search graph is a fully-propagated partial grid. The priority queue (min-heap via `Reverse`) always expands the node whose most-constrained unresolved line has the fewest valid completions — the Minimum Remaining Values (MRV) heuristic.

## Algorithm

1. **Propagate** the initial grid by intersection-forcing: for each unresolved line, enumerate all valid completions and set any cell that is the same value across all of them. Cascade until stable.
2. If the grid is complete, return it. If propagation detected a conflict, return `NoSolution`.
3. Otherwise, push the propagated grid onto the heap.
4. **Pop** the lowest-priority node (fewest completions for its tightest line).
5. Find the most-constrained unresolved line; **enumerate** its completions.
6. For each completion: apply it, propagate, push survivors back onto the heap.
7. Repeat until a complete valid grid is found or the heap empties.

Most well-formed puzzles are solved entirely by propagation (0 heap nodes). The heap is exercised only when propagation stalls.

## Key Internal Types (all private)

### `Cell`
Private 3-valued enum `{ Unknown, Filled, Empty }`. Never exposed outside this crate. Converted to/from `CellState` at the public boundary only.

### `SearchState`
Holds `rows`, `cols`, `row_clues: Vec<Vec<usize>>`, `col_clues: Vec<Vec<usize>>`. Clues are stored as `usize` internally for arithmetic; converted from `u32` at construction in `SearchState::from_puzzle`.

### `Node`
`{ min_count: usize, grid: Vec<Cell> }`. Ordered by `min_count` (lower = pop first).

## Constraint Functions

| Function | Description |
|---|---|
| `count_completions` | Memoised DP; counts valid completions without enumerating them |
| `enumerate_completions` | Recursive placer; returns all valid fully-determined line states |
| `forced_cells` | Intersection: cells identical across all completions |
| `line_matches` | Validates a fully-assigned line against its clues |

## Public API

```rust
pub struct GraphSearchSolver;
impl Solver for GraphSearchSolver { ... }
```

`SearchState::solve_counted` also returns the heap push count — useful for benchmarking and debugging, but not exposed through the trait.

## Design Constraints

- `Cell` must remain private. The boundary conversion (`to_state`) is the only place `Cell` becomes `CellState`.
- Clue arithmetic stays in `usize`. Do not change internal clue storage to `u32` — the cast at `from_puzzle` is the intended isolation point.
- The MRV priority and propagation logic are the core performance levers. Profile before changing the heap ordering or propagation loop.

## Git Commits

**Before committing, verify scope.**

- Change is entirely within this crate and the workspace compiles without touching anything else → commit from here.
- Change was triggered by a `nonogram-core` update, touches files in other crates, or modifies the workspace `Cargo.toml` → defer to **Claude-Main** (workspace root).

This rule applies even if the user explicitly asks this Claude to commit. Git has no technical barrier — any Claude can commit from any working directory in the repo — but a partial cross-boundary commit leaves the workspace broken. When in doubt, check scope first.
