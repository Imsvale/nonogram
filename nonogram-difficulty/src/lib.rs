//! Puzzle difficulty rating, derived from *how a puzzle solves* rather than
//! from comparing which solver crate happens to be able to finish it — see
//! `discussions/DifficultyRating.md` for why that alternative was rejected
//! (unstable across solver-implementation changes; conflates "this puzzle
//! is hard" with "this particular solver got faster").
//!
//! Thin wrapper around `nonogram-propagation`: this crate makes every
//! judgment call about what a solve trace means for a human-facing
//! difficulty number, so the solver itself never has to. Drives
//! `Propagator` directly rather than going through `nonogram-graph-search`
//! — "does this puzzle need branching" is answered by whether propagation
//! alone reaches a complete grid, which is simpler and more robust than
//! trying to recover that fact from a graph-search step trace.
//!
//! Scored dimensions, in brief (full reasoning in the discussion doc):
//! - **Revisit density** — steps needed beyond one-per-line, i.e. how much
//!   re-deduction cross-referencing forced.
//! - **Yield friction** — how little progress each step bought, weighted by
//!   line length (long lines are harder to verify by hand at equal yield).
//! - **Narrowness** — mean, across productive cascade rounds, of how few
//!   lines were independently forceable relative to how many were still
//!   unresolved (see [`DifficultyReport::narrowness`] for why mean rather
//!   than the single tightest round). `None` if propagation never had a
//!   round with 2+ unresolved lines to survey.
//! - **Requires branching** — binary only, for now. Not scored in depth;
//!   deliberately deferred (see discussion doc).
//!
//! Plus a log-scaled **size tax**, additive, independent of the above:
//! tracking more lines is real cognitive load even when no individual line
//! is hard.
//!
//! **The weights below are first guesses, not calibrated against anything**
//! — there's no labeled difficulty ground truth in this repo. Expect them
//! to change after testing against real puzzles.

use nonogram_core::{CellState, LineId, Puzzle, SolveStep};
use nonogram_propagation::{ForceabilityAtRound, Propagator};

// ---------------------------------------------------------------------------
// Weights
// ---------------------------------------------------------------------------

/// Tunable coefficients for combining the individual dimensions into a
/// single rating. All provisional — see module docs.
#[derive(Clone, Copy, Debug)]
pub struct DifficultyWeights {
    /// Coefficient on `log2(rows * cols)` — the size tax.
    pub size_tax_k: f32,
    /// Coefficient on revisit density.
    pub revisit_weight: f32,
    /// Coefficient on yield friction.
    pub yield_weight: f32,
    /// Coefficient on narrowness. Currently inert — narrowness is always
    /// `None` until `nonogram-propagation` grows the telemetry hook
    /// described in `discussions/DifficultyRating.md`.
    pub narrowness_weight: f32,
}

impl Default for DifficultyWeights {
    fn default() -> Self {
        // First-guess constants, not fitted to anything. Chosen so a small
        // (5x5), trivially-forced puzzle lands near the bottom of the scale
        // and a large (50x50) puzzle with real revisit/friction lands in the
        // upper half, with room above for genuinely hard cases before the
        // clamp at 10.0 bites. Expect these to move after real testing.
        DifficultyWeights {
            size_tax_k: 0.15,
            revisit_weight: 2.0,
            yield_weight: 4.0,
            narrowness_weight: 3.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

/// Full breakdown of a difficulty rating, not just the final number — the
/// point of exposing every component is so the number can be sanity-checked
/// and the weights retuned without re-solving anything.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DifficultyReport {
    /// Final rating, 1.0-10.0, already rounded to one decimal.
    pub rating: f32,
    /// `log2(rows * cols) * weights.size_tax_k`.
    pub size_tax: f32,
    /// `max(0, steps / (rows+cols) - 1)`, before weighting.
    pub revisit_density: f32,
    /// Mean per-step `1 - resolved/unknowns_before`, weighted by
    /// `log2(line_length)`, before weighting. Zero if propagation forced
    /// nothing at all (e.g. an already-solved grid).
    pub yield_friction: f32,
    /// Mean, across every productive cascade round, of that round's
    /// `1 - (forceable - 1) / (unresolved - 1)` (large = few lines were
    /// independently forceable relative to how many were still in play —
    /// a tight bottleneck; 0 = every unresolved line was forceable, wide
    /// open). Rounds where `unresolved < 2` are excluded (undefined: only
    /// one line could possibly be "the" bottleneck, which isn't a
    /// bottleneck). `None` if propagation made zero productive rounds with
    /// `unresolved >= 2` at all (e.g. a puzzle that stalls immediately).
    ///
    /// Mean, not the single narrowest round's score: tested both against
    /// ~3400 real puzzles (`examples/narrowness_probe.rs`) before picking —
    /// the single-narrowest-round statistic saturates at its ceiling for
    /// ~24% of puzzles (it only takes one round anywhere in the solve with
    /// exactly one forceable line among several unresolved, which turns out
    /// to be common), while the mean spreads out over the full range with
    /// under 2% at either extreme. See `discussions/DifficultyRating.md`
    /// for the numbers behind this call.
    pub narrowness: Option<f32>,
    /// Whether propagation alone (this crate's `Propagator`) could not
    /// fully resolve the grid — i.e. a search-capable solver would need to
    /// guess. Binary only; see module docs for why this isn't scored
    /// further yet.
    pub requires_branching: bool,
}

/// Why a rating couldn't be produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DifficultyError {
    /// Row/column clue totals disagree — the puzzle is structurally invalid.
    InvalidPuzzle(String),
    /// Propagation hit a contradiction — no valid solution exists at all.
    /// Difficulty isn't meaningful for a puzzle with no solution.
    Unsolvable,
}

// ---------------------------------------------------------------------------
// Rating
// ---------------------------------------------------------------------------

/// Rate a puzzle's difficulty using the default weights. See
/// [`rate_with_weights`] to override them.
pub fn rate(puzzle: &Puzzle) -> Result<DifficultyReport, DifficultyError> {
    rate_with_weights(puzzle, &DifficultyWeights::default())
}

/// Rate a puzzle's difficulty with explicit weights — the entry point to
/// use while calibrating, so a whole puzzle set can be re-scored without
/// re-solving anything at the call site (solving is cheap; this is about
/// making the weight-tuning loop fast).
pub fn rate_with_weights(puzzle: &Puzzle, weights: &DifficultyWeights) -> Result<DifficultyReport, DifficultyError> {
    if !clue_totals_match(puzzle) {
        let row_total: u32 = puzzle.row_clues.iter().flat_map(|r| r.iter()).sum();
        let col_total: u32 = puzzle.col_clues.iter().flat_map(|c| c.iter()).sum();
        return Err(DifficultyError::InvalidPuzzle(format!(
            "row clues sum to {row_total} but column clues sum to {col_total}"
        )));
    }

    let propagator = Propagator::from_puzzle(puzzle);
    let mut grid = vec![CellState::Unknown; propagator.rows * propagator.cols];
    let mut steps: Vec<SolveStep> = Vec::new();
    let (ok, rounds) = propagator.propagate_with_telemetry(&mut grid, &mut steps, false);
    if !ok {
        return Err(DifficultyError::Unsolvable);
    }

    let requires_branching = !propagator.is_complete(&grid);

    let size_tax = size_tax(&propagator) * weights.size_tax_k;
    let revisit_density = revisit_density(&propagator, &steps);
    let yield_friction = yield_friction(&propagator, &steps);
    let narrowness = narrowness(&rounds);

    let raw = size_tax
        + revisit_density * weights.revisit_weight
        + yield_friction * weights.yield_weight
        + narrowness.unwrap_or(0.0) * weights.narrowness_weight;

    let rating = (1.0 + raw).clamp(1.0, 10.0);
    let rating = (rating * 10.0).round() / 10.0;

    Ok(DifficultyReport {
        rating,
        size_tax,
        revisit_density,
        yield_friction,
        narrowness,
        requires_branching,
    })
}

fn clue_totals_match(puzzle: &Puzzle) -> bool {
    let row_total: u32 = puzzle.row_clues.iter().flat_map(|r| r.iter()).sum();
    let col_total: u32 = puzzle.col_clues.iter().flat_map(|c| c.iter()).sum();
    row_total == col_total
}

// ---------------------------------------------------------------------------
// Dimension 1: revisit density
// ---------------------------------------------------------------------------

fn revisit_density(propagator: &Propagator, steps: &[SolveStep]) -> f32 {
    let lines = (propagator.rows + propagator.cols) as f32;
    if lines == 0.0 { return 0.0; }
    (steps.len() as f32 / lines - 1.0).max(0.0)
}

// ---------------------------------------------------------------------------
// Dimension 2: yield friction
// ---------------------------------------------------------------------------

fn yield_friction(propagator: &Propagator, steps: &[SolveStep]) -> f32 {
    let mut grid = vec![CellState::Unknown; propagator.rows * propagator.cols];
    let mut weighted_friction_sum = 0.0f64;
    let mut weight_sum = 0.0f64;

    for step in steps {
        if step.cells_changed.is_empty() { continue; }
        let Some(line) = step.line else { continue; };

        let line_len = match line {
            LineId::Row(_) => propagator.cols,
            LineId::Col(_) => propagator.rows,
        };
        let unknowns_before = count_unknowns(&grid, propagator, line);
        if unknowns_before > 0 {
            let resolved = step.cells_changed.len() as f64;
            let friction = 1.0 - resolved / unknowns_before as f64;
            // floor line_len at 2 so log2 never goes to 0 (or negative for
            // a 1-cell line) and silently zero out that step's weight.
            let weight = (line_len.max(2) as f64).log2();
            weighted_friction_sum += friction * weight;
            weight_sum += weight;
        }

        for &(r, c, v) in &step.cells_changed {
            grid[r * propagator.cols + c] = v;
        }
    }

    if weight_sum > 0.0 { (weighted_friction_sum / weight_sum) as f32 } else { 0.0 }
}

fn count_unknowns(grid: &[CellState], propagator: &Propagator, line: LineId) -> usize {
    match line {
        LineId::Row(r) => grid[r * propagator.cols..(r + 1) * propagator.cols]
            .iter().filter(|&&c| c == CellState::Unknown).count(),
        LineId::Col(c) => (0..propagator.rows)
            .filter(|&r| grid[r * propagator.cols + c] == CellState::Unknown).count(),
    }
}

// ---------------------------------------------------------------------------
// Dimension 3: narrowness
// ---------------------------------------------------------------------------

/// See the `narrowness` doc comment on [`DifficultyReport`] for why this is
/// a mean over rounds rather than the single tightest round.
fn narrowness(rounds: &[ForceabilityAtRound]) -> Option<f32> {
    let mut sum = 0.0f64;
    let mut count = 0usize;
    for r in rounds {
        if r.unresolved < 2 {
            continue;
        }
        let score = 1.0 - (r.forceable as f64 - 1.0) / (r.unresolved as f64 - 1.0);
        sum += score;
        count += 1;
    }
    if count == 0 { None } else { Some((sum / count as f64) as f32) }
}

// ---------------------------------------------------------------------------
// Size tax
// ---------------------------------------------------------------------------

fn size_tax(propagator: &Propagator) -> f32 {
    let cells = (propagator.rows * propagator.cols).max(1) as f32;
    cells.log2()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nonogram_core::CellState;

    fn solved_puzzle(n: usize) -> Puzzle {
        // n x n checkerboard-ish trivial puzzle: single full-width/height
        // clue per line, so every line is forced in one pass, no revisits.
        Puzzle {
            name: "trivial".into(),
            answer: None,
            width: n,
            height: n,
            row_clues: vec![vec![n as u32]; n],
            col_clues: vec![vec![n as u32]; n],
            solution: None,
        }
    }

    fn permutation_puzzle(n: usize) -> Puzzle {
        Puzzle {
            name: "permutation".into(),
            answer: None,
            width: n,
            height: n,
            row_clues: vec![vec![1]; n],
            col_clues: vec![vec![1]; n],
            solution: None,
        }
    }

    #[test]
    fn trivial_puzzle_rates_low_and_needs_no_branching() {
        let report = rate(&solved_puzzle(10)).unwrap();
        assert!(!report.requires_branching);
        assert_eq!(report.revisit_density, 0.0);
        assert_eq!(report.yield_friction, 0.0);
        assert!(report.rating < 3.0, "expected a low rating for an all-filled trivial puzzle, got {}", report.rating);
    }

    #[test]
    fn permutation_puzzle_requires_branching() {
        // No line has any forced cell in an all-Unknown permutation grid —
        // propagation alone can never complete it.
        let report = rate(&permutation_puzzle(6)).unwrap();
        assert!(report.requires_branching);
    }

    #[test]
    fn rejects_mismatched_clue_totals() {
        let puzzle = Puzzle {
            name: "bad".into(),
            answer: None,
            width: 2,
            height: 2,
            row_clues: vec![vec![2], vec![0]],
            col_clues: vec![vec![1], vec![0]],
            solution: None,
        };
        assert_eq!(
            rate(&puzzle),
            Err(DifficultyError::InvalidPuzzle("row clues sum to 2 but column clues sum to 1".into()))
        );
    }

    #[test]
    fn rating_stays_in_bounds() {
        for n in [3, 10, 25, 50] {
            let report = rate(&solved_puzzle(n)).unwrap();
            assert!((1.0..=10.0).contains(&report.rating), "n={n} rating={}", report.rating);
        }
    }

    #[test]
    fn custom_weights_change_the_rating() {
        let puzzle = solved_puzzle(20);
        let low = rate_with_weights(&puzzle, &DifficultyWeights { size_tax_k: 0.0, ..DifficultyWeights::default() }).unwrap();
        let high = rate_with_weights(&puzzle, &DifficultyWeights { size_tax_k: 5.0, ..DifficultyWeights::default() }).unwrap();
        assert!(high.rating > low.rating);
    }

    #[test]
    fn count_unknowns_reads_rows_and_cols_correctly() {
        let propagator = Propagator::from_puzzle(&solved_puzzle(3));
        let grid = vec![
            CellState::Filled, CellState::Unknown, CellState::Empty,
            CellState::Unknown, CellState::Unknown, CellState::Filled,
            CellState::Empty, CellState::Filled, CellState::Unknown,
        ];
        // row1 = [Unknown, Unknown, Filled] -> 2 unknowns
        assert_eq!(count_unknowns(&grid, &propagator, LineId::Row(1)), 2);
        // col2 = [Empty, Filled, Unknown] -> 1 unknown
        assert_eq!(count_unknowns(&grid, &propagator, LineId::Col(2)), 1);
    }
}
