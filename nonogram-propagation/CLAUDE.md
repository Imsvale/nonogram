# nonogram-propagation

Category 3 (per `discussions/SolverTaxonomy.md`): fully computerized
propagation — hard logic, no branching, no search, and *no obligation to be
human-interpretable*. A cell is forced here iff it's forced across every
valid completion of its line, full stop, regardless of whether any named,
human-recognizable pattern would catch it.

This is a different design goal from `nonogram-human-by-ai` (category 2),
which deliberately *caps itself* at a curated set of named passes so its
`Partial` result stays legible ("these techniques ran out"). This crate has
no such cap. Its `Partial` means "propagation alone — the strongest
per-line deductive argument possible — cannot fully solve this puzzle,"
which is a different, strictly more powerful claim than category 2's.

**History note for whoever reads this crate's git blame:** this directory
previously held category 2's content (the technique-driven pass-function
solver, struct `PropagationSolver`, kept unrenamed — not this move's call, per
the identity-model correction below). That content moved to
`nonogram-human-by-ai`; this crate was populated fresh with what's
described below, extracted out of `nonogram-graph-search`, where it
originated as an internal implementation detail before being split into its
own crate. Per this workspace's convention, a Claude identity is bound to
the directory, not the code in it — so despite the near-total content swap,
this is still Claude-Propagation's crate.

## Why this exists as its own crate

`nonogram-graph-search` (category 4) needs propagation between every branch
guess, and needs line-level completion counting/enumeration for its MRV
heuristic and branch-candidate generation. None of that is
graph-search-specific — it's general nonogram line deduction that happened
to have exactly one caller before this split. See
`discussions/SolverTaxonomy.md` for the full reasoning, and
`docs/graph-search-propagation-performance.md` for the performance
investigation that happened while this logic still lived inside
`nonogram-graph-search` (the bug and fix described there are unaffected by
the move — same algorithm, same complexity, just relocated).

## Key Types

### `Propagator`
Holds a puzzle's dimensions and clues: `rows`, `cols`,
`row_clues: Vec<Vec<usize>>`, `col_clues: Vec<Vec<usize>>` (clues stored as
`usize` for arithmetic, converted from `u32` once at construction via
`Propagator::from_puzzle`). All fields `pub` — `nonogram-graph-search`
reads them directly for its own MRV loop, rather than this crate exposing a
parallel accessor API for the same data.

Methods: `propagate` (cascade to fixpoint), `min_completion_count` (MRV
signal), `is_complete`, `check` (validates a fully-assigned grid against
clues).

Operates directly on `nonogram_core::CellState` — no private internal cell
type. That's a deliberate difference from how `nonogram-graph-search` used
to work (where a private `Cell` was converted to `CellState` only at a
single `solve()` boundary): this crate has multiple public entry points at
the line level, not just one top-level solve, so a redundant parallel type
requiring conversion everywhere would cost more than it buys. `CellState`
already *is* the three-valued representation this logic needs.

## Line-Level Functions (pure, no `Propagator` needed)

| Function | Description |
|---|---|
| `count_completions` | Memoized DP; counts valid completions without ever enumerating them |
| `enumerate_completions` | Recursive placer; materializes every valid completion — expensive on sparse lines, only safe to call on a line already known to have a small count |
| `forced_cells_dp` (private) | The DP `Propagator::propagate` uses internally: forces cells via two DP passes over the same state space `count_completions` uses, without enumerating anything. See its doc comment for the algorithm and a subtle bug (reversed-line-mirror shortcut) it does *not* use because that shortcut is wrong for per-cell forcing. |
| `line_matches` (private) | Validates one fully-assigned line against its clues |

## Public API

```rust
pub struct Propagator { pub rows: usize, pub cols: usize, pub row_clues: Vec<Vec<usize>>, pub col_clues: Vec<Vec<usize>> }
pub fn count_completions(cells: &[CellState], clues: &[usize]) -> usize;
pub fn enumerate_completions(cells: &[CellState], clues: &[usize]) -> Vec<Vec<CellState>>;

pub struct PropagationSolver;
impl Solver for PropagationSolver { ... }
```

`PropagationSolver` is category 3 exposed as a standalone `Solver` —
propagation alone, returns `Partial` if it can't finish the grid. Whether
this should be wired into `nonogram-cli`/`nonogram-gui` as a user-selectable
solver is an open question (see `discussions/SolverTaxonomy.md`) — not
decided as of this writing. The impl exists because it costs almost nothing
on top of `Propagator` and keeps the option open without committing to it.

## Design Constraints

- Keep this crate's `Partial` meaning "hard logic exhausted," full stop —
  do not add a fallback to any named-technique restriction here. That
  restriction is `nonogram-human-by-ai`'s entire reason for existing
  separately; blurring it here defeats the point of both crates. (See the
  discussion doc for the fuller argument, made by this crate's own
  identity, against ever routing category-2-style technique attribution
  through this crate's DP.)
- `nonogram-graph-search` should depend on *this* crate for propagation,
  never on `nonogram-human-by-ai` — category 2's technique cap is a feature
  for explainability, not a computational floor, and would only make a
  machine search branch on cells that were in fact forced, for zero
  benefit.
- Clue arithmetic stays in `usize`; the cast from `u32` happens once, at
  `Propagator::from_puzzle`.

## Ownership

This crate is owned by **Claude-Propagation**. For cross-crate changes —
most notably anything touching `nonogram-graph-search`'s use of this
crate's API, or `nonogram-core` — coordinate via a `discussions/` doc rather
than assuming either side unilaterally.

## Git Commits

**Before committing, verify scope.**

- Change is entirely within this crate and the workspace compiles without touching anything else → commit from here.
- Change was triggered by a `nonogram-core` update, touches files in other crates (including `nonogram-graph-search`'s use of this crate), or modifies the workspace `Cargo.toml` → defer to **Claude-Main** (workspace root).

This rule applies even if the user explicitly asks this Claude to commit. A partial cross-boundary commit leaves the workspace broken. When in doubt, check scope first.
