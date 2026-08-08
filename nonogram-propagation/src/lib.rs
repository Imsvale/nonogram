//! Category-3 solving: full per-line deductive closure via dynamic
//! programming, with no obligation to be human-interpretable — just to
//! prove everything a hard-logic argument about a single line can prove, as
//! fast as possible. No branching, no search, no named techniques.
//!
//! This is deliberately *not* the same design goal as a human-technique
//! solver (see `nonogram-human-by-ai`, category 2): that crate caps itself
//! at a curated set of named, human-legible passes, on purpose, so its
//! `Partial` result means something legible ("these techniques ran out").
//! This crate has no such cap — a cell here is forced iff it's forced
//! across *every* valid completion of its line, regardless of whether any
//! named pattern would catch it.
//!
//! Extracted from `nonogram-graph-search`, where this logic originated as
//! an internal implementation detail (`propagate()` / `forced_cells_dp`)
//! before being exposed as its own crate. `nonogram-graph-search` (category
//! 4) now depends on this crate for propagation between branches and for
//! the completion-counting/enumeration primitives its MRV heuristic and
//! branch-candidate generation need — none of that is graph-search-specific
//! logic, it's general per-line nonogram deduction that happened to have
//! exactly one caller before this split.
//!
//! Operates directly on `nonogram_core::CellState` rather than a private
//! internal cell type — unlike the crate this was extracted from, this
//! crate has multiple public entry points at the line level (not just one
//! top-level `solve()`), so a separate private enum requiring conversion at
//! every boundary buys nothing: `CellState` already *is* the three-valued
//! Unknown/Filled/Empty representation this logic needs.

use std::collections::HashMap;
use nonogram_core::{CellState, Puzzle, SolutionState, Solver, SolveContext, SolveResult, SolveStep, LineId};

// ---------------------------------------------------------------------------
// Line-level constraint functions (pure; no puzzle-wide state needed)
// ---------------------------------------------------------------------------

/// Count valid completions of a line via memoized DP over `(cells consumed,
/// clues placed so far)` states — never constructs a completion, however
/// large the true count is.
pub fn count_completions(cells: &[CellState], clues: &[usize]) -> usize {
    count_inner(cells, 0, clues, 0, &mut HashMap::new())
}

fn count_inner(
    cells: &[CellState], ci: usize, clues: &[usize], qi: usize,
    memo: &mut HashMap<(usize, usize), usize>,
) -> usize {
    if let Some(&v) = memo.get(&(ci, qi)) { return v; }
    let v = if qi == clues.len() {
        if cells[ci..].iter().all(|&c| c != CellState::Filled) { 1 } else { 0 }
    } else if ci >= cells.len() {
        0
    } else {
        let mut total = 0usize;
        if cells[ci] != CellState::Filled {
            total = total.saturating_add(count_inner(cells, ci + 1, clues, qi, memo));
        }
        let g = clues[qi];
        if ci + g <= cells.len() {
            let fits = cells[ci..ci + g].iter().all(|&c| c != CellState::Empty);
            let after = ci + g == cells.len() || cells[ci + g] != CellState::Filled;
            if fits && after {
                total = total.saturating_add(
                    count_inner(cells, (ci + g + 1).min(cells.len()), clues, qi + 1, memo)
                );
            }
        }
        total
    };
    memo.insert((ci, qi), v);
    v
}

/// Determine forced cells for a line via dynamic programming, without ever
/// materializing individual completions. Returns `None` if no valid
/// completion exists (contradiction); otherwise one `Option<CellState>` per
/// cell — `Some(v)` when every valid completion agrees on `v` at that
/// position.
///
/// This is what `Propagator::propagate` uses instead of
/// `enumerate_completions` + intersecting: those are exponential in the
/// number of completions (a line can validly have tens of millions of
/// them), while this is `O(n * clues.len())` states, each `O(1)` amortized
/// via memoization — nothing is ever enumerated, only counted or asked
/// "does at least one completion exist consistent with this partial
/// choice?"
///
/// Combines two DPs over the same `(ci, qi)` state space `count_inner` uses
/// ("`ci` cells consumed, `qi` clues fully placed"): `fwd_reach`, built by
/// direct forward simulation using the exact same transitions, answers "is
/// this state reachable from the start?"; `count_inner` itself, called
/// as-is, answers "can this state still reach the end?" (its existing
/// purpose for `count_completions`). A cell is forced to Filled or Empty iff,
/// across every combination of a reachable state and a transition out of it
/// that still leads to the end, only one of the two values ever occurs at
/// that position.
///
/// `fwd_reach` has to be its own explicit forward pass rather than a mirror
/// call to `count_inner` on the reversed line — that reversal is tempting
/// (and does hold for the *total count*: reversing a line and its clues
/// preserves the number of completions) but it computes a subtly different,
/// more permissive question for *individual* states. `count_inner`'s
/// transitions bundle a clue's mandatory trailing gap into the same step as
/// placing the clue, so e.g. cells `[Filled, Unknown, Unknown]` with clues
/// `[1, 1]` never actually visits the state "one clue placed, one cell
/// consumed" — placing the first clue at position 0 jumps straight to
/// position 2 (past the mandatory gap at position 1) in a single transition.
/// The reversed-line mirror doesn't know about that bundling and reports the
/// skipped-over state reachable anyway, which then lets a spurious second
/// transition fire through the gap cell and corrupts its forced value. See
/// `forced_cells_dp_matches_exhaustive_enumeration`, which caught this.
fn forced_cells_dp(cells: &[CellState], clues: &[usize]) -> Option<Vec<Option<CellState>>> {
    let n = cells.len();
    let k = clues.len();

    let mut memo = HashMap::new();
    if count_inner(cells, 0, clues, 0, &mut memo) == 0 { return None; }

    let mut fwd_reach = vec![vec![false; k + 1]; n + 1];
    fwd_reach[0][0] = true;
    for ci in 0..n {
        for qi in 0..=k {
            if !fwd_reach[ci][qi] { continue; }

            // Treat cells[ci] as Empty; stay on the same clue.
            if cells[ci] != CellState::Filled {
                fwd_reach[ci + 1][qi] = true;
            }

            // Place clue qi starting exactly at ci (gap included, if any).
            if qi < k {
                let g = clues[qi];
                if ci + g <= n {
                    let fits = cells[ci..ci + g].iter().all(|&c| c != CellState::Empty);
                    let after_ok = ci + g == n || cells[ci + g] != CellState::Filled;
                    if fits && after_ok {
                        let next_ci = if ci + g < n { ci + g + 1 } else { n };
                        fwd_reach[next_ci][qi + 1] = true;
                    }
                }
            }
        }
    }

    let suffix_feasible = |ci: usize, qi: usize, memo: &mut HashMap<(usize, usize), usize>| {
        count_inner(cells, ci, clues, qi, memo) > 0
    };

    let mut can_fill = vec![false; n];
    let mut can_empty = vec![false; n];

    for ci in 0..n {
        for qi in 0..=k {
            if !fwd_reach[ci][qi] { continue; }

            // Treat cells[ci] as Empty; stay on the same clue.
            if cells[ci] != CellState::Filled && suffix_feasible(ci + 1, qi, &mut memo) {
                can_empty[ci] = true;
            }

            // Start clue qi exactly at ci.
            if qi < k {
                let g = clues[qi];
                if ci + g <= n {
                    let fits = cells[ci..ci + g].iter().all(|&c| c != CellState::Empty);
                    let after_ok = ci + g == n || cells[ci + g] != CellState::Filled;
                    if fits && after_ok {
                        let next_ci = if ci + g < n { ci + g + 1 } else { n };
                        if suffix_feasible(next_ci, qi + 1, &mut memo) {
                            for p in ci..ci + g { can_fill[p] = true; }
                            // The mandatory gap cell right after this group (if any) is
                            // consumed *within* this same transition — it jumps straight
                            // to `next_ci`, so (ci + g, qi + 1) is never itself a reachable
                            // DP state to hang an Option-A check off of. Mark it here.
                            if ci + g < n { can_empty[ci + g] = true; }
                        }
                    }
                }
            }
        }
    }

    Some((0..n).map(|i| match (can_fill[i], can_empty[i]) {
        (true, false) => Some(CellState::Filled),
        (false, true) => Some(CellState::Empty),
        _ => None,
    }).collect())
}

/// Materializes every valid completion of a line. Expensive on sparse
/// lines (a 50-cell line can validly have tens of millions of completions)
/// — only safe to call on a line already known to have a small count (via
/// `count_completions`), which is exactly how `nonogram-graph-search` uses
/// it: to produce actual candidate branches for the *one* line chosen
/// because it has the fewest completions. Never call this to test whether a
/// cell is forced — that's what `Propagator::propagate` (via
/// `forced_cells_dp`) is for, and it never enumerates anything.
pub fn enumerate_completions(cells: &[CellState], clues: &[usize]) -> Vec<Vec<CellState>> {
    let mut out = Vec::new();
    let mut buf = cells.to_vec();
    place(cells, &mut buf, 0, clues, 0, &mut out);
    out
}

fn place(orig: &[CellState], buf: &mut Vec<CellState>, ci: usize, clues: &[usize], qi: usize, out: &mut Vec<Vec<CellState>>) {
    let n = orig.len();
    if qi == clues.len() {
        if orig[ci..].iter().any(|&c| c == CellState::Filled) { return; }
        let mut completion = buf.clone();
        for c in &mut completion[ci..] { if *c == CellState::Unknown { *c = CellState::Empty; } }
        out.push(completion);
        return;
    }
    let g = clues[qi];
    let tail: usize = clues[qi + 1..].iter().sum::<usize>() + clues[qi + 1..].len();
    if ci + g + tail > n { return; }
    let max_start = n - g - tail;
    for start in ci..=max_start {
        if start > ci && orig[start - 1] == CellState::Filled { break; }
        if orig[start..start + g].iter().any(|&c| c == CellState::Empty) { continue; }
        if start + g < n && orig[start + g] == CellState::Filled { continue; }
        for i in ci..start { buf[i] = CellState::Empty; }
        for i in start..start + g { buf[i] = CellState::Filled; }
        let next_ci = if start + g < n { buf[start + g] = CellState::Empty; start + g + 1 } else { n };
        place(orig, buf, next_ci, clues, qi + 1, out);
        for i in ci..next_ci { buf[i] = orig[i]; }
    }
}

fn line_matches(cells: &[CellState], clues: &[usize]) -> bool {
    let mut groups = Vec::new();
    let mut run = 0usize;
    for &c in cells {
        match c {
            CellState::Filled => run += 1,
            CellState::Empty  => { if run > 0 { groups.push(run); run = 0; } }
            CellState::Unknown => return false,
        }
    }
    if run > 0 { groups.push(run); }
    groups == clues
}

// ---------------------------------------------------------------------------
// Propagator: puzzle-wide deduction over a grid
// ---------------------------------------------------------------------------

/// Holds a puzzle's dimensions and clues (clues in `usize` for arithmetic
/// convenience — converted from `u32` once, here, at construction) and
/// provides whole-grid deduction over them. Cheap to construct; typically
/// built once per solve and reused across every propagation cascade and
/// branch.
pub struct Propagator {
    pub rows: usize,
    pub cols: usize,
    pub row_clues: Vec<Vec<usize>>,
    pub col_clues: Vec<Vec<usize>>,
}

impl Propagator {
    pub fn from_puzzle(p: &Puzzle) -> Self {
        Propagator {
            rows: p.height,
            cols: p.width,
            row_clues: p.row_clues.iter().map(|c| c.iter().map(|&n| n as usize).collect()).collect(),
            col_clues: p.col_clues.iter().map(|c| c.iter().map(|&n| n as usize).collect()).collect(),
        }
    }

    /// Cascade intersection-forced cells until stable. Returns false on conflict.
    /// Appends one SolveStep per line that produces at least one forced cell.
    /// When `log_steps` is set, each such step is also logged live at `debug`
    /// level via the `log` crate — a terse one-liner, not a branching trace.
    pub fn propagate(&self, grid: &mut Vec<CellState>, steps: &mut Vec<SolveStep>, log_steps: bool) -> bool {
        loop {
            let mut changed = false;
            for r in 0..self.rows {
                let cells = grid[r * self.cols..(r + 1) * self.cols].to_vec();
                if cells.iter().all(|&c| c != CellState::Unknown) { continue; }
                let Some(forced_row) = forced_cells_dp(&cells, &self.row_clues[r]) else { return false; };
                let mut cells_changed = Vec::new();
                for (c, forced) in forced_row.into_iter().enumerate() {
                    if let Some(v) = forced {
                        if grid[r * self.cols + c] != v {
                            grid[r * self.cols + c] = v;
                            changed = true;
                            cells_changed.push((r, c, v));
                        }
                    }
                }
                if !cells_changed.is_empty() {
                    if log_steps { log::debug!("row {r}: propagation forced {} cell(s)", cells_changed.len()); }
                    steps.push(SolveStep {
                        description: format!("row {}: propagation", r),
                        line: Some(LineId::Row(r)),
                        cells_changed,
                    });
                }
            }
            for c in 0..self.cols {
                let cells: Vec<CellState> = (0..self.rows).map(|r| grid[r * self.cols + c]).collect();
                if cells.iter().all(|&c2| c2 != CellState::Unknown) { continue; }
                let Some(forced_col) = forced_cells_dp(&cells, &self.col_clues[c]) else { return false; };
                let mut cells_changed = Vec::new();
                for (r, forced) in forced_col.into_iter().enumerate() {
                    if let Some(v) = forced {
                        if grid[r * self.cols + c] != v {
                            grid[r * self.cols + c] = v;
                            changed = true;
                            cells_changed.push((r, c, v));
                        }
                    }
                }
                if !cells_changed.is_empty() {
                    if log_steps { log::debug!("col {c}: propagation forced {} cell(s)", cells_changed.len()); }
                    steps.push(SolveStep {
                        description: format!("col {}: propagation", c),
                        line: Some(LineId::Col(c)),
                        cells_changed,
                    });
                }
            }
            if !changed { break; }
        }
        true
    }

    /// Fewest valid completions among all unresolved lines — the MRV signal.
    /// `usize::MAX` if every line is already fully resolved.
    pub fn min_completion_count(&self, grid: &[CellState]) -> usize {
        let mut min = usize::MAX;
        for r in 0..self.rows {
            let cells = &grid[r * self.cols..(r + 1) * self.cols];
            if cells.iter().any(|&c| c == CellState::Unknown) {
                min = min.min(count_completions(cells, &self.row_clues[r]));
            }
        }
        for c in 0..self.cols {
            let cells: Vec<CellState> = (0..self.rows).map(|r| grid[r * self.cols + c]).collect();
            if cells.iter().any(|&c2| c2 == CellState::Unknown) {
                min = min.min(count_completions(&cells, &self.col_clues[c]));
            }
        }
        min
    }

    pub fn is_complete(&self, grid: &[CellState]) -> bool { grid.iter().all(|&c| c != CellState::Unknown) }

    /// Validates a fully-assigned grid against every row/col clue.
    pub fn check(&self, grid: &[CellState]) -> bool {
        for r in 0..self.rows {
            if !line_matches(&grid[r * self.cols..(r + 1) * self.cols], &self.row_clues[r]) { return false; }
        }
        for c in 0..self.cols {
            let cells: Vec<CellState> = (0..self.rows).map(|r| grid[r * self.cols + c]).collect();
            if !line_matches(&cells, &self.col_clues[c]) { return false; }
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Public solver: propagation alone, no branching
// ---------------------------------------------------------------------------

/// Solves using only `Propagator::propagate` — no backtracking, no search.
/// Returns `SolutionState::Partial` if propagation stalls before the grid
/// is fully determined; a stronger solver (e.g. `GraphSearchSolver`) may
/// make further progress from there. Unlike `nonogram-human-by-ai`'s
/// `HumanByAiSolver`, which caps itself at a curated set of named
/// techniques, this exhausts everything a per-line hard-logic argument can
/// prove — `Partial` here means "propagation alone cannot fully solve this
/// puzzle," not "these particular techniques ran out."
pub struct PropagationSolver;

fn clue_totals_match(puzzle: &Puzzle) -> bool {
    let row_total: u32 = puzzle.row_clues.iter().flat_map(|r| r.iter()).sum();
    let col_total: u32 = puzzle.col_clues.iter().flat_map(|c| c.iter()).sum();
    row_total == col_total
}

impl Solver for PropagationSolver {
    fn solve(&self, puzzle: &Puzzle, _ctx: &SolveContext) -> SolveResult {
        if !clue_totals_match(puzzle) {
            let row_total: u32 = puzzle.row_clues.iter().flat_map(|r| r.iter()).sum();
            let col_total: u32 = puzzle.col_clues.iter().flat_map(|c| c.iter()).sum();
            return SolveResult { state: SolutionState::Invalid(format!("row clues sum to {row_total} but column clues sum to {col_total}")), grid: vec![], steps: vec![] };
        }
        let propagator = Propagator::from_puzzle(puzzle);
        let mut grid = vec![CellState::Unknown; propagator.rows * propagator.cols];
        let mut steps = Vec::new();
        if !propagator.propagate(&mut grid, &mut steps, false) {
            return SolveResult { state: SolutionState::Unsolvable, grid, steps };
        }
        let state = if propagator.is_complete(&grid) {
            if propagator.check(&grid) { SolutionState::Complete } else { SolutionState::Unsolvable }
        } else {
            SolutionState::Partial
        };
        SolveResult { state, grid, steps }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Old exhaustive-enumeration definition of forced cells, kept only in
    // tests as an oracle to check `forced_cells_dp` against: materialize
    // every completion, then intersect. This predates the DP rewrite —
    // exponential, but trivially correct, which makes it a good reference
    // implementation for small lines.
    fn old_forced_cells(cells: &[CellState], clues: &[usize]) -> Option<Vec<Option<CellState>>> {
        let completions = enumerate_completions(cells, clues);
        if completions.is_empty() { return None; }
        let n = completions[0].len();
        Some((0..n).map(|i| {
            let v = completions[0][i];
            if completions[1..].iter().all(|c| c[i] == v) { Some(v) } else { None }
        }).collect())
    }

    fn run_lengths(cells: &[CellState]) -> Vec<usize> {
        let mut out = Vec::new();
        let mut run = 0usize;
        for &c in cells {
            if c == CellState::Filled { run += 1; } else if run > 0 { out.push(run); run = 0; }
        }
        if run > 0 { out.push(run); }
        out
    }

    #[test]
    fn forced_cells_dp_matches_exhaustive_enumeration() {
        // Exhaustive, not random: for every line length up to 7, every full
        // Filled/Empty assignment (defining a clue list), and every subset of
        // positions hidden as Unknown, the DP and the old enumerate-then-
        // intersect approach must agree exactly. ~87k cases, all fast because
        // n is small.
        for n in 1..=7usize {
            for bits in 0u32..(1 << n) {
                let full: Vec<CellState> = (0..n)
                    .map(|i| if (bits >> i) & 1 == 1 { CellState::Filled } else { CellState::Empty })
                    .collect();
                let clues = run_lengths(&full);
                for hide in 0u32..(1 << n) {
                    let cells: Vec<CellState> = (0..n)
                        .map(|i| if (hide >> i) & 1 == 1 { CellState::Unknown } else { full[i] })
                        .collect();
                    let expected = old_forced_cells(&cells, &clues);
                    let actual = forced_cells_dp(&cells, &clues);
                    assert_eq!(
                        expected, actual,
                        "n={n} full={full:?} clues={clues:?} cells={cells:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn forced_cells_dp_detects_contradiction() {
        // clue [3] needs 3 consecutive Filled cells somewhere in a line of 4,
        // but every placement of that run covers position 1, which is fixed Empty.
        let cells = vec![CellState::Unknown, CellState::Empty, CellState::Unknown, CellState::Unknown];
        assert_eq!(forced_cells_dp(&cells, &[3]), None);
    }

    #[test]
    fn forced_cells_dp_handles_huge_completion_counts() {
        // The line that motivated this rewrite: 50 cells, clues summing to far
        // less than the line, admitting 86,493,225 completions per this crate's
        // own (memoized, non-materializing) counter — enumerate_completions
        // would never finish building all of them.
        let clues = vec![2, 1, 1, 2, 1, 1, 5, 1, 1, 1, 1, 4];
        let cells = vec![CellState::Unknown; 50];
        assert_eq!(count_completions(&cells, &clues), 86_493_225);
        let forced = forced_cells_dp(&cells, &clues).expect("line is feasible");
        assert_eq!(forced.len(), 50);
    }

    // n×n grid of single-cell clues: any permutation matrix satisfies every
    // line, so intersection propagation alone forces nothing — a puzzle this
    // solver can only ever return Partial on, never Complete.
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
    fn propagation_solver_returns_partial_when_it_cannot_finish_alone() {
        let puzzle = permutation_puzzle(4);
        let ctx = SolveContext::default();
        let result = PropagationSolver.solve(&puzzle, &ctx);
        assert_eq!(result.state, SolutionState::Partial);
        // No line has any forced cell in an all-Unknown permutation grid.
        assert!(result.grid.iter().all(|&c| c == CellState::Unknown));
    }

    #[test]
    fn propagation_solver_rejects_mismatched_clue_totals() {
        let puzzle = Puzzle {
            name: "bad".into(),
            answer: None,
            width: 2,
            height: 2,
            row_clues: vec![vec![2], vec![0]],
            col_clues: vec![vec![1], vec![0]],
            solution: None,
        };
        let ctx = SolveContext::default();
        let result = PropagationSolver.solve(&puzzle, &ctx);
        assert!(matches!(result.state, SolutionState::Invalid(_)));
    }
}
