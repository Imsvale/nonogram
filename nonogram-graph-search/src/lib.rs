//! Best-first graph search solver with MRV heuristic and intersection propagation.
//!
//! Each node in the search graph is a fully-propagated partial grid. The priority
//! queue (min-heap) always expands the node whose most-constrained unresolved line
//! has the fewest valid completions. Intersection propagation resolves forced cells
//! after every branch, often finishing the puzzle without further search.
//!
//! Internal `Cell` type and arithmetic stay in `usize`; conversion to/from
//! `core::CellState` happens only at the public boundary.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::time::{Duration, Instant};
use nonogram_core::{AllSolutions, CancelToken, CellState, ExhaustiveSolver, LineId, Puzzle, Solver, SolveContext, SolveResult, SolveStep, SolutionState};

// ---------------------------------------------------------------------------
// Internal cell type (private to this crate)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Cell { Unknown, Filled, Empty }

fn to_state(c: Cell) -> CellState {
    match c { Cell::Unknown => CellState::Unknown, Cell::Filled => CellState::Filled, Cell::Empty => CellState::Empty }
}

// ---------------------------------------------------------------------------
// Internal puzzle representation (clues in usize for arithmetic convenience)
// ---------------------------------------------------------------------------

struct SearchState {
    rows: usize,
    cols: usize,
    row_clues: Vec<Vec<usize>>,
    col_clues: Vec<Vec<usize>>,
}

impl SearchState {
    fn from_puzzle(p: &Puzzle) -> Self {
        SearchState {
            rows: p.height,
            cols: p.width,
            row_clues: p.row_clues.iter().map(|c| c.iter().map(|&n| n as usize).collect()).collect(),
            col_clues: p.col_clues.iter().map(|c| c.iter().map(|&n| n as usize).collect()).collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Constraint functions (logic unchanged from nonogram-solver-graph-search)
// ---------------------------------------------------------------------------

fn count_completions(cells: &[Cell], clues: &[usize]) -> usize {
    count_inner(cells, 0, clues, 0, &mut HashMap::new())
}

fn count_inner(
    cells: &[Cell], ci: usize, clues: &[usize], qi: usize,
    memo: &mut HashMap<(usize, usize), usize>,
) -> usize {
    if let Some(&v) = memo.get(&(ci, qi)) { return v; }
    let v = if qi == clues.len() {
        if cells[ci..].iter().all(|&c| c != Cell::Filled) { 1 } else { 0 }
    } else if ci >= cells.len() {
        0
    } else {
        let mut total = 0usize;
        if cells[ci] != Cell::Filled {
            total = total.saturating_add(count_inner(cells, ci + 1, clues, qi, memo));
        }
        let g = clues[qi];
        if ci + g <= cells.len() {
            let fits = cells[ci..ci + g].iter().all(|&c| c != Cell::Empty);
            let after = ci + g == cells.len() || cells[ci + g] != Cell::Filled;
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

fn enumerate_completions(cells: &[Cell], clues: &[usize]) -> Vec<Vec<Cell>> {
    let mut out = Vec::new();
    let mut buf = cells.to_vec();
    place(cells, &mut buf, 0, clues, 0, &mut out);
    out
}

fn place(orig: &[Cell], buf: &mut Vec<Cell>, ci: usize, clues: &[usize], qi: usize, out: &mut Vec<Vec<Cell>>) {
    let n = orig.len();
    if qi == clues.len() {
        if orig[ci..].iter().any(|&c| c == Cell::Filled) { return; }
        let mut completion = buf.clone();
        for c in &mut completion[ci..] { if *c == Cell::Unknown { *c = Cell::Empty; } }
        out.push(completion);
        return;
    }
    let g = clues[qi];
    let tail: usize = clues[qi + 1..].iter().sum::<usize>() + clues[qi + 1..].len();
    if ci + g + tail > n { return; }
    let max_start = n - g - tail;
    for start in ci..=max_start {
        if start > ci && orig[start - 1] == Cell::Filled { break; }
        if orig[start..start + g].iter().any(|&c| c == Cell::Empty) { continue; }
        if start + g < n && orig[start + g] == Cell::Filled { continue; }
        for i in ci..start { buf[i] = Cell::Empty; }
        for i in start..start + g { buf[i] = Cell::Filled; }
        let next_ci = if start + g < n { buf[start + g] = Cell::Empty; start + g + 1 } else { n };
        place(orig, buf, next_ci, clues, qi + 1, out);
        for i in ci..next_ci { buf[i] = orig[i]; }
    }
}

fn forced_cells(completions: &[Vec<Cell>]) -> Vec<Option<Cell>> {
    let n = completions[0].len();
    (0..n).map(|i| {
        let v = completions[0][i];
        if completions[1..].iter().all(|c| c[i] == v) { Some(v) } else { None }
    }).collect()
}

fn line_matches(cells: &[Cell], clues: &[usize]) -> bool {
    let mut groups = Vec::new();
    let mut run = 0usize;
    for &c in cells {
        match c {
            Cell::Filled => run += 1,
            Cell::Empty  => { if run > 0 { groups.push(run); run = 0; } }
            Cell::Unknown => return false,
        }
    }
    if run > 0 { groups.push(run); }
    groups == clues
}

// ---------------------------------------------------------------------------
// Search node
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Node {
    min_count: usize,
    grid: Vec<Cell>,
    steps: Vec<SolveStep>,
}

// steps are not part of node identity or ordering
impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool { self.min_count == other.min_count && self.grid == other.grid }
}
impl Eq for Node {}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering { self.min_count.cmp(&other.min_count) }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
}

// ---------------------------------------------------------------------------
// Solver implementation
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Line { Row(usize), Col(usize) }


impl SearchState {
    /// Cascade intersection-forced cells until stable. Returns false on conflict.
    /// Appends one SolveStep per line that produces at least one forced cell.
    /// When `log_steps` is set, each such step is also logged live at `debug`
    /// level via the `log` crate — a terse one-liner, not a branching trace.
    fn propagate(&self, grid: &mut Vec<Cell>, steps: &mut Vec<SolveStep>, log_steps: bool) -> bool {
        loop {
            let mut changed = false;
            for r in 0..self.rows {
                let cells = grid[r * self.cols..(r + 1) * self.cols].to_vec();
                if cells.iter().all(|&c| c != Cell::Unknown) { continue; }
                let completions = enumerate_completions(&cells, &self.row_clues[r]);
                if completions.is_empty() { return false; }
                let mut cells_changed = Vec::new();
                for (c, forced) in forced_cells(&completions).into_iter().enumerate() {
                    if let Some(v) = forced {
                        if grid[r * self.cols + c] != v {
                            grid[r * self.cols + c] = v;
                            changed = true;
                            cells_changed.push((r, c, to_state(v)));
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
                let cells: Vec<Cell> = (0..self.rows).map(|r| grid[r * self.cols + c]).collect();
                if cells.iter().all(|&c2| c2 != Cell::Unknown) { continue; }
                let completions = enumerate_completions(&cells, &self.col_clues[c]);
                if completions.is_empty() { return false; }
                let mut cells_changed = Vec::new();
                for (r, forced) in forced_cells(&completions).into_iter().enumerate() {
                    if let Some(v) = forced {
                        if grid[r * self.cols + c] != v {
                            grid[r * self.cols + c] = v;
                            changed = true;
                            cells_changed.push((r, c, to_state(v)));
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

    fn most_constrained(&self, grid: &[Cell]) -> Option<(Line, Vec<Vec<Cell>>)> {
        let mut best: Option<(Line, usize)> = None;
        for r in 0..self.rows {
            let cells = &grid[r * self.cols..(r + 1) * self.cols];
            if cells.iter().all(|&c| c != Cell::Unknown) { continue; }
            let count = count_completions(cells, &self.row_clues[r]);
            if best.map_or(true, |(_, b)| count < b) { best = Some((Line::Row(r), count)); }
        }
        for c in 0..self.cols {
            let cells: Vec<Cell> = (0..self.rows).map(|r| grid[r * self.cols + c]).collect();
            if cells.iter().all(|&c2| c2 != Cell::Unknown) { continue; }
            let count = count_completions(&cells, &self.col_clues[c]);
            if best.map_or(true, |(_, b)| count < b) { best = Some((Line::Col(c), count)); }
        }
        let (line, _) = best?;
        let completions = match line {
            Line::Row(r) => enumerate_completions(&grid[r * self.cols..(r + 1) * self.cols], &self.row_clues[r]),
            Line::Col(c) => {
                let cells: Vec<Cell> = (0..self.rows).map(|r| grid[r * self.cols + c]).collect();
                enumerate_completions(&cells, &self.col_clues[c])
            }
        };
        Some((line, completions))
    }

    fn apply_line(&self, grid: &mut Vec<Cell>, line: Line, completion: &[Cell]) {
        match line {
            Line::Row(r) => { for (c, &v) in completion.iter().enumerate() { grid[r * self.cols + c] = v; } }
            Line::Col(col) => { for (r, &v) in completion.iter().enumerate() { grid[r * self.cols + col] = v; } }
        }
    }

    fn min_completion_count(&self, grid: &[Cell]) -> usize {
        let mut min = usize::MAX;
        for r in 0..self.rows {
            let cells = &grid[r * self.cols..(r + 1) * self.cols];
            if cells.iter().any(|&c| c == Cell::Unknown) {
                min = min.min(count_completions(cells, &self.row_clues[r]));
            }
        }
        for c in 0..self.cols {
            let cells: Vec<Cell> = (0..self.rows).map(|r| grid[r * self.cols + c]).collect();
            if cells.iter().any(|&c2| c2 == Cell::Unknown) {
                min = min.min(count_completions(&cells, &self.col_clues[c]));
            }
        }
        min
    }

    fn is_complete(&self, grid: &[Cell]) -> bool { grid.iter().all(|&c| c != Cell::Unknown) }

    fn check(&self, grid: &[Cell]) -> bool {
        for r in 0..self.rows {
            if !line_matches(&grid[r * self.cols..(r + 1) * self.cols], &self.row_clues[r]) { return false; }
        }
        for c in 0..self.cols {
            let cells: Vec<Cell> = (0..self.rows).map(|r| grid[r * self.cols + c]).collect();
            if !line_matches(&cells, &self.col_clues[c]) { return false; }
        }
        true
    }

    fn solve_counted(
        &self,
        cancel: &CancelToken,
        config: &ProgressConfig,
        mut on_progress: Option<&mut dyn FnMut(ProgressUpdate)>,
    ) -> (SolutionState, Vec<Cell>, Vec<SolveStep>, usize) {
        let mut grid = vec![Cell::Unknown; self.rows * self.cols];
        let mut steps: Vec<SolveStep> = Vec::new();
        let mut pushed = 0usize;
        let mut expanded = 0usize;

        if !self.propagate(&mut grid, &mut steps, config.log_steps) {
            return (SolutionState::Unsolvable, grid, steps, pushed);
        }
        if self.is_complete(&grid) {
            return if self.check(&grid) {
                (SolutionState::Complete, grid, steps, pushed)
            } else {
                (SolutionState::Unsolvable, grid, steps, pushed)
            };
        }

        // Save the post-propagation state: the definitive progress that holds
        // regardless of which branch is taken. Returned if the heap exhausts.
        let partial_grid = grid.clone();
        let partial_steps = steps.clone();

        let watch_progress = config.log_meta_interval.is_some() || config.snapshot_interval.is_some();
        let start = Instant::now();
        let mut last_meta_log: Option<Instant> = None;
        let mut last_snapshot: Option<Instant> = None;

        let mut heap: BinaryHeap<Reverse<Node>> = BinaryHeap::new();
        heap.push(Reverse(Node { min_count: self.min_completion_count(&grid), grid, steps }));
        pushed += 1;

        while let Some(Reverse(node)) = heap.pop() {
            expanded += 1;
            if cancel.is_cancelled() {
                return (SolutionState::Aborted, node.grid, node.steps, pushed);
            }
            let Some((line, completions)) = self.most_constrained(&node.grid) else { continue; };
            for completion in completions {
                let mut grid = node.grid.clone();
                let mut steps = node.steps.clone();

                // Record cells that this branch assignment introduces (Unknown → known)
                let line_id = match line { Line::Row(r) => LineId::Row(r), Line::Col(c) => LineId::Col(c) };
                let cells_changed: Vec<(usize, usize, CellState)> = match line {
                    Line::Row(r) => completion.iter().enumerate()
                        .filter(|&(c, &v)| grid[r * self.cols + c] != v)
                        .map(|(c, &v)| (r, c, to_state(v)))
                        .collect(),
                    Line::Col(col) => completion.iter().enumerate()
                        .filter(|&(r, &v)| grid[r * self.cols + col] != v)
                        .map(|(r, &v)| (r, col, to_state(v)))
                        .collect(),
                };
                if !cells_changed.is_empty() {
                    let desc = match line_id {
                        LineId::Row(r) => format!("row {}: branch", r),
                        LineId::Col(c) => format!("col {}: branch", c),
                    };
                    if config.log_steps { log::debug!("{desc} ({} cell(s))", cells_changed.len()); }
                    steps.push(SolveStep { description: desc, line: Some(line_id), cells_changed });
                }

                self.apply_line(&mut grid, line, &completion);
                if !self.propagate(&mut grid, &mut steps, config.log_steps) { continue; }
                if self.is_complete(&grid) {
                    if self.check(&grid) {
                        return (SolutionState::Complete, grid, steps, pushed);
                    }
                    continue;
                }
                let min_count = self.min_completion_count(&grid);
                pushed += 1;

                if watch_progress {
                    let now = Instant::now();
                    let due_meta = config.log_meta_interval
                        .is_some_and(|iv| last_meta_log.is_none_or(|t| now.duration_since(t) >= iv));
                    let due_snap = config.snapshot_interval
                        .is_some_and(|iv| last_snapshot.is_none_or(|t| now.duration_since(t) >= iv));
                    if due_meta || due_snap {
                        let known = grid.iter().filter(|&&c| c != Cell::Unknown).count();
                        let total = grid.len();
                        let heap_len = heap.len() + 1;
                        if due_meta {
                            log::info!(
                                "search: pushed={pushed} expanded={expanded} heap={heap_len} \
                                 min_count={min_count} known={known}/{total} elapsed={:.1}s",
                                start.elapsed().as_secs_f32(),
                            );
                            last_meta_log = Some(now);
                        }
                        if due_snap {
                            if let Some(cb) = on_progress.as_deref_mut() {
                                cb(ProgressUpdate {
                                    nodes_pushed: pushed,
                                    nodes_expanded: expanded,
                                    heap_len,
                                    best_min_count: min_count,
                                    cells_known: known,
                                    cells_total: total,
                                    elapsed: start.elapsed(),
                                    grid: grid.iter().map(|&c| to_state(c)).collect(),
                                });
                            }
                            last_snapshot = Some(now);
                        }
                    }
                }

                heap.push(Reverse(Node { min_count, grid, steps }));
            }
        }
        (SolutionState::Unsolvable, partial_grid, partial_steps, pushed)
    }

    fn solve_all_counted(&self, cancel: &CancelToken) -> (Vec<(Vec<Cell>, Vec<SolveStep>)>, usize, usize, bool) {
        let mut grid = vec![Cell::Unknown; self.rows * self.cols];
        let mut steps: Vec<SolveStep> = Vec::new();
        let mut expanded = 0usize;
        let mut pushed = 0usize;
        let mut solutions: Vec<(Vec<Cell>, Vec<SolveStep>)> = Vec::new();

        if !self.propagate(&mut grid, &mut steps, false) { return (solutions, expanded, pushed, false); }
        if self.is_complete(&grid) {
            if self.check(&grid) { solutions.push((grid, steps)); }
            return (solutions, expanded, pushed, false);
        }

        let mut heap: BinaryHeap<Reverse<Node>> = BinaryHeap::new();
        heap.push(Reverse(Node { min_count: self.min_completion_count(&grid), grid, steps }));
        pushed += 1;

        while let Some(Reverse(node)) = heap.pop() {
            expanded += 1;
            if cancel.is_cancelled() {
                return (solutions, expanded, pushed, true);
            }
            let Some((line, completions)) = self.most_constrained(&node.grid) else { continue; };
            for completion in completions {
                let mut grid = node.grid.clone();
                let mut steps = node.steps.clone();

                let line_id = match line { Line::Row(r) => LineId::Row(r), Line::Col(c) => LineId::Col(c) };
                let cells_changed: Vec<(usize, usize, CellState)> = match line {
                    Line::Row(r) => completion.iter().enumerate()
                        .filter(|&(c, &v)| grid[r * self.cols + c] != v)
                        .map(|(c, &v)| (r, c, to_state(v)))
                        .collect(),
                    Line::Col(col) => completion.iter().enumerate()
                        .filter(|&(r, &v)| grid[r * self.cols + col] != v)
                        .map(|(r, &v)| (r, col, to_state(v)))
                        .collect(),
                };
                if !cells_changed.is_empty() {
                    let desc = match line_id {
                        LineId::Row(r) => format!("row {}: branch", r),
                        LineId::Col(c) => format!("col {}: branch", c),
                    };
                    steps.push(SolveStep { description: desc, line: Some(line_id), cells_changed });
                }

                self.apply_line(&mut grid, line, &completion);
                if !self.propagate(&mut grid, &mut steps, false) { continue; }
                if self.is_complete(&grid) {
                    if self.check(&grid) { solutions.push((grid, steps)); }
                    continue;
                }
                let min_count = self.min_completion_count(&grid);
                heap.push(Reverse(Node { min_count, grid, steps }));
                pushed += 1;
            }
        }
        (solutions, expanded, pushed, false)
    }
}

// ---------------------------------------------------------------------------
// Progress observability (opt-in; see GraphSearchSolver::solve_with_progress)
// ---------------------------------------------------------------------------

/// Controls the opt-in observability hooks in [`GraphSearchSolver::solve_with_progress`].
///
/// Every field defaults to disabled, so `ProgressConfig::default()` behaves
/// identically to the plain `Solver::solve` path: no extra logging, no snapshot
/// clones, no behavioural difference. Turn on only what you need — each flag
/// has its own cost, paid only while it's on.
#[derive(Clone, Debug, Default)]
pub struct ProgressConfig {
    /// Log every propagation or branch step that changed at least one cell, via
    /// the `log` crate at `debug` level. One line per step (line + cell count),
    /// not a dump of the branching search itself — discarded branches are never
    /// logged, only the steps that survive on the winning path so far.
    pub log_steps: bool,
    /// Log a one-line search summary via the `log` crate at `info` level, no
    /// more often than this interval.
    pub log_meta_interval: Option<Duration>,
    /// Invoke the `on_progress` callback with a grid snapshot, no more often
    /// than this interval. Has no effect if `on_progress` is `None`.
    pub snapshot_interval: Option<Duration>,
}

/// A point-in-time snapshot of search progress, delivered to the `on_progress`
/// callback passed to [`GraphSearchSolver::solve_with_progress`].
#[derive(Clone, Debug)]
pub struct ProgressUpdate {
    /// Heap nodes pushed so far (candidate branches generated).
    pub nodes_pushed: usize,
    /// Heap nodes popped so far (candidate branches expanded).
    pub nodes_expanded: usize,
    /// Current heap size — how many unexplored branches are queued.
    pub heap_len: usize,
    /// Fewest valid completions among the tightest unresolved line in this node.
    /// Lower means the search is closer to being forced; a rough proxy for how
    /// constrained the current branch is.
    pub best_min_count: usize,
    /// Cells resolved (Filled or Empty) in this snapshot's grid.
    pub cells_known: usize,
    /// Total cells in the grid.
    pub cells_total: usize,
    /// Wall-clock time since the search (post-initial-propagation) began.
    pub elapsed: Duration,
    /// Grid snapshot, present only because `ProgressConfig::snapshot_interval`
    /// was set — building it costs a full grid clone, skipped otherwise.
    pub grid: Vec<CellState>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub struct GraphSearchSolver;

fn clue_totals_match(puzzle: &Puzzle) -> bool {
    let row_total: u32 = puzzle.row_clues.iter().flat_map(|r| r.iter()).sum();
    let col_total: u32 = puzzle.col_clues.iter().flat_map(|c| c.iter()).sum();
    row_total == col_total
}

impl GraphSearchSolver {
    /// Like `Solver::solve`, but with opt-in observability: `config` controls
    /// what gets logged and how often `on_progress` is invoked with a
    /// structured [`ProgressUpdate`]. Pass `&ProgressConfig::default()` and
    /// `None` for behaviour identical to `Solver::solve`.
    ///
    /// Intended for long-running solves (large or difficult puzzles) where a
    /// caller wants to watch what the search is doing rather than block
    /// silently until it finishes.
    pub fn solve_with_progress(
        &self,
        puzzle: &Puzzle,
        ctx: &SolveContext,
        config: &ProgressConfig,
        on_progress: Option<&mut dyn FnMut(ProgressUpdate)>,
    ) -> SolveResult {
        if !clue_totals_match(puzzle) {
            let row_total: u32 = puzzle.row_clues.iter().flat_map(|r| r.iter()).sum();
            let col_total: u32 = puzzle.col_clues.iter().flat_map(|c| c.iter()).sum();
            return SolveResult { state: SolutionState::Invalid(format!("row clues sum to {row_total} but column clues sum to {col_total}")), grid: vec![], steps: vec![] };
        }
        let search = SearchState::from_puzzle(puzzle);
        let (state, grid, steps, _nodes) = search.solve_counted(&ctx.cancel, config, on_progress);
        SolveResult { state, grid: grid.into_iter().map(to_state).collect(), steps }
    }
}

impl Solver for GraphSearchSolver {
    fn solve(&self, puzzle: &Puzzle, ctx: &SolveContext) -> SolveResult {
        self.solve_with_progress(puzzle, ctx, &ProgressConfig::default(), None)
    }
}

impl ExhaustiveSolver for GraphSearchSolver {
    fn solve_all(&self, puzzle: &Puzzle, ctx: &SolveContext) -> AllSolutions {
        if !clue_totals_match(puzzle) {
            return AllSolutions { solutions: vec![], nodes_expanded: 0, nodes_pushed: 0, aborted: false };
        }
        let state = SearchState::from_puzzle(puzzle);
        let (found, nodes_expanded, nodes_pushed, aborted) = state.solve_all_counted(&ctx.cancel);
        let solutions = found.into_iter().map(|(flat, steps)| SolveResult {
            state: SolutionState::Complete,
            grid: flat.into_iter().map(to_state).collect(),
            steps,
        }).collect();
        AllSolutions { solutions, nodes_expanded, nodes_pushed, aborted }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // n×n grid of single-cell clues: any permutation matrix satisfies every
    // line, so intersection propagation alone forces nothing and the solver
    // must branch — exactly the code path progress observability hooks into.
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
    fn solve_with_progress_default_config_matches_plain_solve() {
        let puzzle = permutation_puzzle(4);
        let ctx = SolveContext::default();
        let solver = GraphSearchSolver;
        let plain = solver.solve(&puzzle, &ctx);
        let observed = solver.solve_with_progress(&puzzle, &ctx, &ProgressConfig::default(), None);
        assert_eq!(plain.state, observed.state);
        assert_eq!(plain.grid.len(), observed.grid.len());
        assert_eq!(plain.state, SolutionState::Complete);
    }

    #[test]
    fn progress_callback_fires_during_branching_solve() {
        let puzzle = permutation_puzzle(4);
        let ctx = SolveContext::default();
        let solver = GraphSearchSolver;
        let config = ProgressConfig {
            log_steps: true,
            log_meta_interval: Some(Duration::ZERO),
            snapshot_interval: Some(Duration::ZERO),
        };
        let mut snapshots = 0usize;
        let mut on_progress = |update: ProgressUpdate| {
            snapshots += 1;
            assert_eq!(update.cells_total, 16);
            assert!(update.cells_known <= update.cells_total);
            assert_eq!(update.grid.len(), update.cells_total);
        };
        let result = solver.solve_with_progress(&puzzle, &ctx, &config, Some(&mut on_progress));
        assert_eq!(result.state, SolutionState::Complete);
        assert!(snapshots > 0, "expected at least one progress snapshot for a branching solve");
    }

    #[test]
    fn progress_disabled_by_default_never_invokes_callback() {
        let puzzle = permutation_puzzle(4);
        let ctx = SolveContext::default();
        let solver = GraphSearchSolver;
        let mut calls = 0usize;
        let mut on_progress = |_: ProgressUpdate| { calls += 1; };
        // on_progress is Some, but every ProgressConfig flag is off, so it must never fire.
        solver.solve_with_progress(&puzzle, &ctx, &ProgressConfig::default(), Some(&mut on_progress));
        assert_eq!(calls, 0);
    }
}
