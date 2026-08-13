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

Methods: `propagate` (cascade to fixpoint), `propagate_with_telemetry`
(same cascade, plus a per-round forceable-lines survey — see
`discussions/DifficultyRating.md`), `min_completion_count` (MRV signal),
`is_complete`, `check` (validates a fully-assigned grid against clues).

### `propagate_with_telemetry`

Added for `nonogram-difficulty`'s narrowness dimension (dimension 3 in
`discussions/DifficultyRating.md`): for each *productive* cascade round
(one that actually forces a new cell), records how many distinct lines were
independently forceable against the grid state at that round's *start* —
`(bool, Vec<ForceabilityAtRound>)`, success flag plus one
`ForceabilityAtRound { forceable, unresolved }` per productive round. The
trailing no-change round that confirms the fixpoint is never recorded, so
`Vec::is_empty()` means propagation made zero forced deductions at all (not
"one round of nothing"). A puzzle solved entirely by one uninterrupted
cascade round still yields exactly one entry.

`unresolved` (lines still not fully determined at that round's snapshot) is
there specifically so a consumer can normalize `forceable` against what was
actually still in play at that round, not the puzzle's fixed `rows + cols`
— added after `nonogram-difficulty`'s Claude-graph-authored calibration
work flagged that `forceable` alone conflates "narrow relative to the whole
grid" with "narrow relative to what's left." A round late in a large solve
with only 6 lines left and 5 forceable is wide open, not tight, and only
`unresolved` lets a caller tell those apart. Free to compute: `survey_round`
already walks past every resolved line via a `continue`, so counting what
it *doesn't* skip costs nothing beyond a second counter.

The survey (`survey_round`, private) has to check every still-unresolved
line against a *frozen* snapshot taken before that round's own changes
land — it cannot just count how many lines changed during the round's live
application (`apply_round`, also private, shared with plain `propagate`).
`apply_round` applies each line's forces as it finds them (all rows, then
all columns), so a cell forced by row 3 can make col 7 forceable *later in
the same round*, purely as a scan-order artifact — col 7 wasn't
independently available when the round began, it only became solvable
because row 3 happened to be scanned first. Crediting that to the round
would overcount every bottleneck the survey exists to find. See the
`telemetry_does_not_credit_a_round_with_lines_only_unlocked_mid_round` test
for a hand-verified example (a 3×3 "plus sign") where this distinction
changes the reported count from 6 (naive step-count) to the correct 4.

`propagate` and `propagate_with_telemetry` share `apply_round` — the
telemetry variant does not reimplement or alter how forces get applied, it
only adds a read-only survey pass before each round's application. This
guarantees identical final grids and success/failure results between the
two for the same input, verified by
`telemetry_matches_propagate_on_the_final_grid_and_success`.

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
pub struct ForceabilityAtRound { pub forceable: usize, pub unresolved: usize }
pub struct Propagator { pub rows: usize, pub cols: usize, pub row_clues: Vec<Vec<usize>>, pub col_clues: Vec<Vec<usize>> }
impl Propagator {
    pub fn from_puzzle(p: &Puzzle) -> Self;
    pub fn propagate(&self, grid: &mut Vec<CellState>, steps: &mut Vec<SolveStep>, log_steps: bool) -> bool;
    pub fn propagate_with_telemetry(&self, grid: &mut Vec<CellState>, steps: &mut Vec<SolveStep>, log_steps: bool) -> (bool, Vec<ForceabilityAtRound>);
    pub fn min_completion_count(&self, grid: &[CellState]) -> usize;
    pub fn is_complete(&self, grid: &[CellState]) -> bool;
    pub fn check(&self, grid: &[CellState]) -> bool;
}
pub fn count_completions(cells: &[CellState], clues: &[usize]) -> usize;
pub fn enumerate_completions(cells: &[CellState], clues: &[usize]) -> Vec<Vec<CellState>>;

pub struct PropagationSolver;
impl Solver for PropagationSolver { ... }
```

`PropagationSolver` is category 3 exposed as a standalone `Solver` —
propagation alone, returns `Partial` if it can't finish the grid. It's
wired into `nonogram-cli` as `--solver propagation` (see
`discussions/SolverTaxonomy.md` for when/why that flag value moved from
category 2 to this crate) — stale note removed here, this was previously
recorded as still-open when it had already been decided.

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
