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
use nonogram_core::{AllSolutions, CancelToken, CellState, ExhaustiveSolver, LineId, Outcome, Puzzle, Solver, SolveContext, SolveResult, SolveStep};

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
    fn propagate(&self, grid: &mut Vec<Cell>, steps: &mut Vec<SolveStep>) -> bool {
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

    fn solve_counted(&self, cancel: &CancelToken) -> (Option<(Vec<Cell>, Vec<SolveStep>)>, usize, bool) {
        let mut grid = vec![Cell::Unknown; self.rows * self.cols];
        let mut steps: Vec<SolveStep> = Vec::new();
        let mut pushed = 0usize;

        if !self.propagate(&mut grid, &mut steps) { return (None, pushed, false); }
        if self.is_complete(&grid) {
            return if self.check(&grid) { (Some((grid, steps)), pushed, false) } else { (None, pushed, false) };
        }

        let mut heap: BinaryHeap<Reverse<Node>> = BinaryHeap::new();
        heap.push(Reverse(Node { min_count: self.min_completion_count(&grid), grid, steps }));
        pushed += 1;

        while let Some(Reverse(node)) = heap.pop() {
            if cancel.is_cancelled() {
                return (Some((node.grid, node.steps)), pushed, true);
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
                    steps.push(SolveStep { description: desc, line: Some(line_id), cells_changed });
                }

                self.apply_line(&mut grid, line, &completion);
                if !self.propagate(&mut grid, &mut steps) { continue; }
                if self.is_complete(&grid) {
                    if self.check(&grid) { return (Some((grid, steps)), pushed, false); }
                    continue;
                }
                let min_count = self.min_completion_count(&grid);
                heap.push(Reverse(Node { min_count, grid, steps }));
                pushed += 1;
            }
        }
        (None, pushed, false)
    }

    fn solve_all_counted(&self, cancel: &CancelToken) -> (Vec<(Vec<Cell>, Vec<SolveStep>)>, usize, usize, bool) {
        let mut grid = vec![Cell::Unknown; self.rows * self.cols];
        let mut steps: Vec<SolveStep> = Vec::new();
        let mut expanded = 0usize;
        let mut pushed = 0usize;
        let mut solutions: Vec<(Vec<Cell>, Vec<SolveStep>)> = Vec::new();

        if !self.propagate(&mut grid, &mut steps) { return (solutions, expanded, pushed, false); }
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
                if !self.propagate(&mut grid, &mut steps) { continue; }
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
// Public API
// ---------------------------------------------------------------------------

pub struct GraphSearchSolver;

impl Solver for GraphSearchSolver {
    fn solve(&self, puzzle: &Puzzle, ctx: &SolveContext) -> SolveResult {
        let state = SearchState::from_puzzle(puzzle);
        let (result, _nodes, aborted) = state.solve_counted(&ctx.cancel);
        match result {
            Some((flat, steps)) if !aborted => SolveResult {
                outcome: Outcome::Solved,
                grid: flat.into_iter().map(to_state).collect(),
                steps,
                aborted: false,
            },
            Some((flat, steps)) => SolveResult {
                outcome: Outcome::Stuck,
                grid: flat.into_iter().map(to_state).collect(),
                steps,
                aborted: true,
            },
            None => SolveResult { outcome: Outcome::NoSolution, grid: vec![], steps: vec![], aborted: false },
        }
    }
}

impl ExhaustiveSolver for GraphSearchSolver {
    fn solve_all(&self, puzzle: &Puzzle, ctx: &SolveContext) -> AllSolutions {
        let state = SearchState::from_puzzle(puzzle);
        let (found, nodes_expanded, nodes_pushed, aborted) = state.solve_all_counted(&ctx.cancel);
        let solutions = found.into_iter().map(|(flat, steps)| SolveResult {
            outcome: Outcome::Solved,
            grid: flat.into_iter().map(to_state).collect(),
            steps,
            aborted: false,
        }).collect();
        AllSolutions { solutions, nodes_expanded, nodes_pushed, aborted }
    }
}
