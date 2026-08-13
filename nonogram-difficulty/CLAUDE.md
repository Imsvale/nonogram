# nonogram-difficulty

Puzzle difficulty rating, 1.0–10.0, derived from *how a puzzle solves*
rather than from comparing which solver crate happens to be able to finish
it. Full design reasoning — including why the "which solver category"
approach was tried and rejected first — lives in
`discussions/DifficultyRating.md`. Read that before changing the formula;
this file covers the crate's own shape and status, not the reasoning behind
it.

## Design in one paragraph

Thin wrapper around `nonogram-propagation`: calls `Propagator` directly
(not `nonogram-graph-search`), because "does this puzzle need branching" is
just "did propagation alone fail to reach `is_complete()`" — simpler and
more robust than trying to recover that fact from a graph-search step
trace. Every judgment call about what a solve trace *means* for a
human-facing difficulty number lives here; `nonogram-propagation` only ever
emits raw telemetry, never a scored opinion.

## Current status

**All four dimensions are now implemented and tested**, driven off
`Propagator::propagate_with_telemetry`:
- `revisit_density` — steps beyond one-per-line, i.e. how much re-deduction
  cross-referencing forced. Verified size-independent under synthetic
  upscaling (`examples/scale_test.rs`).
- `yield_friction` — mean per-step "how little progress did this buy,"
  weighted by `log2(line_length)`, computed by replaying the step trace to
  reconstruct each line's unknown-count immediately before the step that
  touched it.
- `narrowness` — mean, over every productive cascade round with 2+
  unresolved lines, of `1 - (forceable-1)/(unresolved-1)` for that round
  (large = few lines were independently forceable relative to how many
  were still in play). Uses `ForceabilityAtRound` from
  `nonogram-propagation` (delivered by Claude-Propagation). Deliberately a
  **mean**, not the single tightest round — tested both against ~3400 real
  puzzles (`examples/narrowness_probe.rs`) before deciding: the
  single-narrowest-round statistic saturates at its ceiling for ~24% of
  puzzles (common for *some* round in a solve to have exactly one forceable
  line among several unresolved), while the mean spreads across the full
  0–1 range with under 2% at either extreme. `None` if propagation made
  zero qualifying rounds (e.g. a puzzle that stalls immediately).
- `size_tax` — `log2(rows*cols)`, additive, independent of the above.
- `requires_branching` — binary flag, `!propagator.is_complete(&grid)`
  after propagation stalls. Still contributes zero weight to the rating
  directly, by original design — open question, see below.

**Weights are still first-guess placeholders** (`DifficultyWeights::default()`),
not calibrated against anything — there's no labeled difficulty ground
truth anywhere in this repo. Calibrating against the user's two hand-timed
reference puzzles (`puzzles/cult.txt`'s 50×50, 8 hours by hand; `Kaldo.txt`
Kaldogram #3, a 100×100, 3h18m) surfaced three open questions currently
under discussion in `discussions/DifficultyRating.md` (§1/§3/§4 of the
Claude-difficulty entry there) — not yet resolved, don't treat any of these
as decided:
1. The `[1.0, 10.0]` clamp saturates too eagerly — now that `narrowness` is
   wired in, 3 of the 4 Kaldo anchor puzzles hit exactly 10.0 despite very
   different raw scores underneath (cult's 50×50 is still ~2.2× the raw
   score of the next-highest). A clamp replacement (asymptotic rescale or
   similar) is proposed but not implemented.
2. The user's "roughly 50% of a huge-but-easy puzzle's difficulty should
   trace to sheer size" anchor doesn't hold under current weights (`size_tax`
   is ~24% of Kaldogram #3's raw score), and naively raising `size_tax_k`
   breaks the trivial-puzzle-rates-low test rather than fixing it — the
   `log2(cells)` growth curve is too flat, not just mis-scaled.
3. `requires_branching` contributing zero directly means a puzzle that
   stalls early with a short, clean pre-stall trace (Kaldogram #2) can rate
   low despite genuinely requiring a guess.

None of these should be "fixed" by unilaterally changing weights — see the
discussion doc for the reasoning and current state of input.

## Public API

```rust
pub struct DifficultyWeights { pub size_tax_k: f32, pub revisit_weight: f32, pub yield_weight: f32, pub narrowness_weight: f32 }
pub struct DifficultyReport { pub rating: f32, pub size_tax: f32, pub revisit_density: f32, pub yield_friction: f32, pub narrowness: Option<f32>, pub requires_branching: bool }
pub enum DifficultyError { InvalidPuzzle(String), Unsolvable }

pub fn rate(puzzle: &Puzzle) -> Result<DifficultyReport, DifficultyError>;
pub fn rate_with_weights(puzzle: &Puzzle, weights: &DifficultyWeights) -> Result<DifficultyReport, DifficultyError>;
```

`DifficultyReport` exposes every component, not just the final number —
the point is being able to sanity-check *why* a rating came out where it
did, and retune weights without re-solving anything (`rate_with_weights`
takes weights directly for exactly this reason).

## Testing against real puzzles

```
cargo run -p nonogram-difficulty --example rate -- <puzzle-file>            # table, every puzzle, sorted by rating
cargo run -p nonogram-difficulty --example rate -- <puzzle-file> <index>    # full breakdown, one puzzle (1-based)
```

This is the intended calibration loop: run against a puzzle set, eyeball
whether the spread feels right, adjust `DifficultyWeights::default()`,
repeat. No ground-truth labels exist to fit against automatically —
calibration is by feel against real puzzles, per the discussion doc.

## Design Constraints

- Depends on `nonogram-core` and `nonogram-propagation` only. Never
  `nonogram-graph-search` or `nonogram-human-by-ai` — see the discussion
  doc for why (technique-capped or branching-aware traces would corrupt the
  premise that this measures *pure hard-logic* difficulty).
- Keep every component of a rating visible on `DifficultyReport`, not just
  the final number. Collapsing the breakdown away would make the
  calibration loop above impossible.
- `rate_with_weights` must stay solve-free relative to `rate` — i.e.
  weight-only changes shouldn't require touching `Propagator` again. Keep
  the solve (`Propagator::propagate`) and the scoring (component math +
  combination) cleanly separated for this reason.

## Ownership

Owned by **Claude-difficulty** (took over from Claude-graph, who scaffolded
the crate per the discussion doc's handoff plan). `discussions/DifficultyRating.md`
remains the authority on formula/weight direction — it's a living
calibration discussion, not a closed decision record — and anything
needing new `nonogram-propagation` API should still go through that doc
rather than being decided unilaterally here.

## Git Commits

**Before committing, verify scope.**

- Change is entirely within this crate and the workspace compiles without touching anything else → commit from here.
- Change was triggered by a `nonogram-core` or `nonogram-propagation` update, touches files in other crates, or modifies the workspace `Cargo.toml` → defer to **Claude-Main** (workspace root).
