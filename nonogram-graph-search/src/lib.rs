//! Best-first graph search solver with MRV heuristic.
//!
//! Each node in the search graph is a fully-propagated partial grid. The priority
//! queue (min-heap) always expands the node whose most-constrained unresolved line
//! has the fewest valid completions. Propagation — both the initial cascade before
//! any branching and the cascade after every applied branch — is delegated to
//! `nonogram_propagation::Propagator` (category 3's per-line deductive closure),
//! which this crate depends on rather than reimplementing. Often finishes the
//! puzzle without this crate's own search logic ever running at all; see
//! `nonogram-propagation`'s crate docs for why propagation lives there and not
//! here.
//!
//! What's left in this crate after that split: the heap/`Node` machinery, the MRV
//! line-selection heuristic (`most_constrained`), and the orchestration loop
//! (`solve_counted`/`solve_all_counted`) deciding what to guess and in what order
//! when propagation alone can't finish the grid.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::io::Write;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use nonogram_core::{AllSolutions, CancelToken, CellState, ExhaustiveSolver, LineId, Puzzle, Solver, SolveContext, SolveResult, SolveStep, SolutionState};
use nonogram_propagation::{count_completions, enumerate_completions, Propagator};

// ---------------------------------------------------------------------------
// Search node
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Node {
    min_count: usize,
    grid: Vec<CellState>,
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
// Search state: wraps a Propagator, adds branch selection and application
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Line { Row(usize), Col(usize) }

struct SearchState {
    propagator: Propagator,
}

impl SearchState {
    fn from_puzzle(p: &Puzzle) -> Self {
        SearchState { propagator: Propagator::from_puzzle(p) }
    }

    /// Picks the unresolved line with the fewest valid completions (the MRV
    /// heuristic) and returns every candidate completion for it, to try as
    /// branches. `None` if every line is already fully resolved.
    fn most_constrained(&self, grid: &[CellState]) -> Option<(Line, Vec<Vec<CellState>>)> {
        let p = &self.propagator;
        let mut best: Option<(Line, usize)> = None;
        for r in 0..p.rows {
            let cells = &grid[r * p.cols..(r + 1) * p.cols];
            if cells.iter().all(|&c| c != CellState::Unknown) { continue; }
            let count = count_completions(cells, &p.row_clues[r]);
            if best.map_or(true, |(_, b)| count < b) { best = Some((Line::Row(r), count)); }
        }
        for c in 0..p.cols {
            let cells: Vec<CellState> = (0..p.rows).map(|r| grid[r * p.cols + c]).collect();
            if cells.iter().all(|&c2| c2 != CellState::Unknown) { continue; }
            let count = count_completions(&cells, &p.col_clues[c]);
            if best.map_or(true, |(_, b)| count < b) { best = Some((Line::Col(c), count)); }
        }
        let (line, _) = best?;
        let completions = match line {
            Line::Row(r) => enumerate_completions(&grid[r * p.cols..(r + 1) * p.cols], &p.row_clues[r]),
            Line::Col(c) => {
                let cells: Vec<CellState> = (0..p.rows).map(|r| grid[r * p.cols + c]).collect();
                enumerate_completions(&cells, &p.col_clues[c])
            }
        };
        Some((line, completions))
    }

    fn apply_line(&self, grid: &mut Vec<CellState>, line: Line, completion: &[CellState]) {
        let cols = self.propagator.cols;
        match line {
            Line::Row(r) => { for (c, &v) in completion.iter().enumerate() { grid[r * cols + c] = v; } }
            Line::Col(col) => { for (r, &v) in completion.iter().enumerate() { grid[r * cols + col] = v; } }
        }
    }

    fn solve_counted(
        &self,
        cancel: &CancelToken,
        config: &ProgressConfig,
        mut on_progress: Option<&mut dyn FnMut(ProgressUpdate)>,
    ) -> (SolutionState, Vec<CellState>, Vec<SolveStep>, usize) {
        let p = &self.propagator;
        let mut grid = vec![CellState::Unknown; p.rows * p.cols];
        let mut steps: Vec<SolveStep> = Vec::new();
        let mut pushed = 0usize;
        let mut expanded = 0usize;

        if !p.propagate(&mut grid, &mut steps, config.log_steps) {
            return (SolutionState::Unsolvable, grid, steps, pushed);
        }
        if p.is_complete(&grid) {
            return if p.check(&grid) {
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
        heap.push(Reverse(Node { min_count: p.min_completion_count(&grid), grid, steps }));
        pushed += 1;

        if config.emit_start_snapshot {
            if let Some(cb) = on_progress.as_deref_mut() {
                let known = partial_grid.iter().filter(|&&c| c != CellState::Unknown).count();
                cb(ProgressUpdate {
                    nodes_pushed: pushed,
                    nodes_expanded: expanded,
                    heap_len: heap.len(),
                    best_min_count: p.min_completion_count(&partial_grid),
                    cells_known: known,
                    cells_total: partial_grid.len(),
                    elapsed: start.elapsed(),
                    grid: partial_grid.clone(),
                });
            }
        }

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
                let cols = p.cols;
                let cells_changed: Vec<(usize, usize, CellState)> = match line {
                    Line::Row(r) => completion.iter().enumerate()
                        .filter(|&(c, &v)| grid[r * cols + c] != v)
                        .map(|(c, &v)| (r, c, v))
                        .collect(),
                    Line::Col(col) => completion.iter().enumerate()
                        .filter(|&(r, &v)| grid[r * cols + col] != v)
                        .map(|(r, &v)| (r, col, v))
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
                if !p.propagate(&mut grid, &mut steps, config.log_steps) { continue; }
                if p.is_complete(&grid) {
                    if p.check(&grid) {
                        return (SolutionState::Complete, grid, steps, pushed);
                    }
                    continue;
                }
                let min_count = p.min_completion_count(&grid);
                pushed += 1;

                if watch_progress {
                    let now = Instant::now();
                    let due_meta = config.log_meta_interval
                        .is_some_and(|iv| last_meta_log.is_none_or(|t| now.duration_since(t) >= iv));
                    let due_snap = config.snapshot_interval
                        .is_some_and(|iv| last_snapshot.is_none_or(|t| now.duration_since(t) >= iv));
                    if due_meta || due_snap {
                        let known = grid.iter().filter(|&&c| c != CellState::Unknown).count();
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
                                    grid: grid.clone(),
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

    fn solve_all_counted(&self, cancel: &CancelToken) -> (Vec<(Vec<CellState>, Vec<SolveStep>)>, usize, usize, bool) {
        let p = &self.propagator;
        let mut grid = vec![CellState::Unknown; p.rows * p.cols];
        let mut steps: Vec<SolveStep> = Vec::new();
        let mut expanded = 0usize;
        let mut pushed = 0usize;
        let mut solutions: Vec<(Vec<CellState>, Vec<SolveStep>)> = Vec::new();

        if !p.propagate(&mut grid, &mut steps, false) { return (solutions, expanded, pushed, false); }
        if p.is_complete(&grid) {
            if p.check(&grid) { solutions.push((grid, steps)); }
            return (solutions, expanded, pushed, false);
        }

        let mut heap: BinaryHeap<Reverse<Node>> = BinaryHeap::new();
        heap.push(Reverse(Node { min_count: p.min_completion_count(&grid), grid, steps }));
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
                let cols = p.cols;
                let cells_changed: Vec<(usize, usize, CellState)> = match line {
                    Line::Row(r) => completion.iter().enumerate()
                        .filter(|&(c, &v)| grid[r * cols + c] != v)
                        .map(|(c, &v)| (r, c, v))
                        .collect(),
                    Line::Col(col) => completion.iter().enumerate()
                        .filter(|&(r, &v)| grid[r * cols + col] != v)
                        .map(|(r, &v)| (r, col, v))
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
                if !p.propagate(&mut grid, &mut steps, false) { continue; }
                if p.is_complete(&grid) {
                    if p.check(&grid) { solutions.push((grid, steps)); }
                    continue;
                }
                let min_count = p.min_completion_count(&grid);
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
    /// Fire `on_progress` exactly once, immediately before the heap loop begins,
    /// carrying the post-propagation grid with `nodes_expanded = 0`. This gives
    /// the caller a view of what the initial propagation pass resolved — useful
    /// when propagation does significant work but the puzzle still requires
    /// branching, since the regular `snapshot_interval` throttle only fires
    /// during the heap loop. Has no effect if `on_progress` is `None`.
    /// Independent of `snapshot_interval` — both can be set simultaneously.
    pub emit_start_snapshot: bool,
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
        SolveResult { state, grid, steps }
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
        let solutions = found.into_iter().map(|(grid, steps)| SolveResult {
            state: SolutionState::Complete,
            grid,
            steps,
        }).collect();
        AllSolutions { solutions, nodes_expanded, nodes_pushed, aborted }
    }
}

// ---------------------------------------------------------------------------
// File-backed logger (dev convenience)
// ---------------------------------------------------------------------------

/// Minimal `log::Log` backend that appends formatted records to a file. Not
/// this workspace's production logging story — just enough to point
/// `ProgressConfig { log_steps: true, .. }` at a file while debugging a slow
/// or stuck solve. A binary that wants a real logging setup (env-configurable
/// filters, multiple targets, rotating files, etc.) should install its own
/// `log::Log` implementation instead and skip this one — only one logger can
/// be active per process.
struct FileLogger {
    file: Mutex<std::fs::File>,
    level: log::LevelFilter,
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) { return; }
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(file, "[{:>5} {}] {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() { let _ = file.flush(); }
    }
}

/// Install a simple file-backed logger, appending to `path` (created if it
/// doesn't exist yet). Call once, early — e.g. at the top of `main` in a
/// throwaway debug binary — before running a solve with `ProgressConfig`'s
/// logging flags turned on.
///
/// Errors if a logger is already installed for this process (including a
/// second call to this function), since `log` only permits one global
/// backend. If the binary embedding this crate already sets up its own
/// logger, use that instead of calling this.
pub fn init_file_logger(path: impl AsRef<std::path::Path>, level: log::LevelFilter) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    let logger = FileLogger { file: Mutex::new(file), level };
    log::set_boxed_logger(Box::new(logger))?;
    log::set_max_level(level);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::Log;

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
            ..ProgressConfig::default()
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

    #[test]
    fn file_logger_writes_records() {
        // Exercises FileLogger's write path directly rather than going through
        // log::set_boxed_logger, since that's process-global, one-shot state
        // that would race with other tests running in the same binary.
        let path = std::env::temp_dir().join(format!("nonogram-graph-search-test-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let file = std::fs::OpenOptions::new().create(true).append(true).open(&path).unwrap();
        let logger = FileLogger { file: Mutex::new(file), level: log::LevelFilter::Debug };
        let record = log::Record::builder()
            .args(format_args!("test message"))
            .level(log::Level::Info)
            .target("nonogram_graph_search")
            .build();
        logger.log(&record);
        logger.flush();

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("test message"));
        let _ = std::fs::remove_file(&path);
    }
}
