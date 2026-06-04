//! Constraint-propagation solver — adapted from the standalone nonogram-solver.
//!
//! Technique-driven, no backtracking. Applies human-readable deduction passes
//! until the grid is fully determined or no further progress can be made.
//! All pass logic is unchanged from the original; only the type bindings have
//! been updated to use `nonogram-core`.

use std::collections::VecDeque;
use nonogram_core::{CellState, LineId, Outcome, Puzzle, Solver, SolveContext, SolveResult};

// ---------------------------------------------------------------------------
// LineMeta — solver-internal bookkeeping, not part of the public API
// ---------------------------------------------------------------------------

/// Per-line bookkeeping for the propagation solver.
///
/// Rather than shrinking clue lists, the solver narrows a *working window*
/// `[start, end)` as Empty cells are confirmed at the edges.
pub struct LineMeta {
    pub clues: Vec<u32>,
    pub start: usize,
    pub end: usize,
    pub complete: bool,
}

impl LineMeta {
    pub fn new(clues: Vec<u32>, len: usize) -> Self {
        LineMeta { clues, start: 0, end: len, complete: false }
    }
    pub fn window_len(&self) -> usize {
        self.end - self.start
    }
}

// ---------------------------------------------------------------------------
// SolverState — mutable grid + meta; built from an immutable &Puzzle
// ---------------------------------------------------------------------------

struct SolverState<'a> {
    puzzle:   &'a Puzzle,
    cells:    Vec<CellState>,
    row_meta: Vec<LineMeta>,
    col_meta: Vec<LineMeta>,
}

impl<'a> SolverState<'a> {
    fn new(puzzle: &'a Puzzle) -> Self {
        let cells = vec![CellState::Unknown; puzzle.width * puzzle.height];
        let row_meta = puzzle.row_clues.iter()
            .map(|c| LineMeta::new(c.clone(), puzzle.width))
            .collect();
        let col_meta = puzzle.col_clues.iter()
            .map(|c| LineMeta::new(c.clone(), puzzle.height))
            .collect();
        SolverState { puzzle, cells, row_meta, col_meta }
    }

    fn row_slice(&self, r: usize) -> &[CellState] {
        let s = r * self.puzzle.width;
        &self.cells[s..s + self.puzzle.width]
    }

    fn row_slice_mut(&mut self, r: usize) -> &mut [CellState] {
        let s = r * self.puzzle.width;
        let w = self.puzzle.width;
        &mut self.cells[s..s + w]
    }

    fn col_vec(&self, c: usize) -> Vec<CellState> {
        (0..self.puzzle.height).map(|r| self.cells[r * self.puzzle.width + c]).collect()
    }

    fn write_col(&mut self, c: usize, col: &[CellState]) {
        let w = self.puzzle.width;
        for (r, &val) in col.iter().enumerate() {
            self.cells[r * w + c] = val;
        }
    }
}

// ---------------------------------------------------------------------------
// Cell-write helper
// ---------------------------------------------------------------------------

fn set_cell(cells: &mut [CellState], idx: usize, val: CellState) -> Result<bool, ()> {
    match cells[idx] {
        CellState::Unknown => { cells[idx] = val; Ok(true) }
        existing if existing == val => Ok(false),
        _ => Err(()),
    }
}

// ---------------------------------------------------------------------------
// Pass functions (logic unchanged from nonogram-solver/src/solver.rs)
// ---------------------------------------------------------------------------

fn preprocess(cells: &mut [CellState], meta: &mut LineMeta) -> Result<bool, ()> {
    if meta.complete { return Ok(false); }
    let mut changed = false;

    if meta.clues.is_empty() {
        for c in cells[meta.start..meta.end].iter_mut() {
            if *c == CellState::Unknown { *c = CellState::Empty; changed = true; }
            else if *c == CellState::Filled { return Err(()); }
        }
        meta.complete = true;
        return Ok(changed);
    }

    let total: u32 = meta.clues.iter().sum();
    let gaps = meta.clues.len() - 1;
    let needed = total as usize + gaps;
    if needed == meta.window_len() {
        let mut pos = meta.start;
        for (i, &clue) in meta.clues.iter().enumerate() {
            for _ in 0..clue {
                if set_cell(cells, pos, CellState::Filled)? { changed = true; }
                pos += 1;
            }
            if i + 1 < meta.clues.len() {
                if set_cell(cells, pos, CellState::Empty)? { changed = true; }
                pos += 1;
            }
        }
        meta.complete = true;
    }
    Ok(changed)
}

fn overlap(cells: &mut [CellState], meta: &mut LineMeta) -> Result<bool, ()> {
    if meta.complete || meta.clues.is_empty() { return Ok(false); }

    let total: u32 = meta.clues.iter().sum();
    let gaps = meta.clues.len() as u32 - 1;
    let window = meta.window_len() as u32;
    if total + gaps > window { return Err(()); }

    let slack = window - (total + gaps);
    let mut changed = false;
    let mut ls: u32 = 0;
    for &clue in &meta.clues {
        if clue > slack {
            let fill_start = (ls + slack) as usize + meta.start;
            let fill_end   = (ls + clue)  as usize + meta.start;
            for idx in fill_start..fill_end {
                if set_cell(cells, idx, CellState::Filled)? { changed = true; }
            }
        }
        ls += clue + 1;
    }
    Ok(changed)
}

fn trim(cells: &[CellState], meta: &mut LineMeta) -> bool {
    let (old_start, old_end) = (meta.start, meta.end);
    while meta.start < meta.end && cells[meta.start] == CellState::Empty { meta.start += 1; }
    while meta.end > meta.start && cells[meta.end - 1] == CellState::Empty { meta.end -= 1; }
    meta.start != old_start || meta.end != old_end
}

fn delim(cells: &mut [CellState], meta: &mut LineMeta) -> Result<bool, ()> {
    if meta.complete || meta.clues.is_empty() { return Ok(false); }
    if meta.clues.len() == 1 {
        let clue = meta.clues[0] as usize;
        let window = &cells[meta.start..meta.end];
        let first = window.iter().position(|&c| c == CellState::Filled);
        let last  = window.iter().rposition(|&c| c == CellState::Filled);
        if let (Some(first), Some(last)) = (first, last) {
            let run_len = last - first + 1;
            if run_len > clue { return Err(()); }
            let span_start = if last + 1 > clue { last + 1 - clue } else { 0 };
            let span_end   = (first + clue).min(meta.window_len());
            let mut changed = false;
            for i in 0..span_start {
                if set_cell(cells, meta.start + i, CellState::Empty)? { changed = true; }
            }
            for i in span_end..meta.window_len() {
                if set_cell(cells, meta.start + i, CellState::Empty)? { changed = true; }
            }
            return Ok(changed);
        }
    }
    Ok(false)
}

fn edge_forcing(cells: &mut [CellState], meta: &mut LineMeta) -> Result<bool, ()> {
    if meta.complete || meta.clues.is_empty() || meta.window_len() == 0 { return Ok(false); }
    let mut changed = false;

    if cells[meta.start] == CellState::Filled {
        let clue = meta.clues[0] as usize;
        for i in 1..clue {
            if meta.start + i < meta.end {
                if set_cell(cells, meta.start + i, CellState::Filled)? { changed = true; }
            }
        }
        let after = meta.start + clue;
        if after < meta.end {
            if set_cell(cells, after, CellState::Empty)? { changed = true; }
        }
    }
    if meta.end > meta.start && cells[meta.end - 1] == CellState::Filled {
        let clue = *meta.clues.last().unwrap() as usize;
        for i in 1..clue {
            if meta.end > i + 1 {
                let idx = meta.end - 1 - i;
                if idx >= meta.start {
                    if set_cell(cells, idx, CellState::Filled)? { changed = true; }
                }
            }
        }
        if meta.end > clue && meta.end - clue > meta.start {
            let before = meta.end - clue - 1;
            if set_cell(cells, before, CellState::Empty)? { changed = true; }
        }
    }
    Ok(changed)
}

fn completed_run(cells: &mut [CellState], meta: &mut LineMeta) -> Result<bool, ()> {
    if meta.complete || meta.clues.is_empty() || meta.window_len() == 0 { return Ok(false); }
    let mut changed = false;

    if cells[meta.start] == CellState::Filled {
        let mut pos = meta.start;
        while pos < meta.end && cells[pos] == CellState::Filled { pos += 1; }
        let run_len = pos - meta.start;
        let bounded_right = pos == meta.end || cells[pos] == CellState::Empty;
        if bounded_right {
            let first = meta.clues[0] as usize;
            if run_len == first {
                meta.clues.remove(0);
                meta.start = pos;
                if meta.start < meta.end && cells[meta.start] == CellState::Empty {
                    meta.start += 1;
                }
                changed = true;
            } else if run_len > first {
                return Err(());
            }
        }
    }

    if !meta.clues.is_empty() && meta.end > meta.start && cells[meta.end - 1] == CellState::Filled {
        let mut pos = meta.end;
        while pos > meta.start && cells[pos - 1] == CellState::Filled { pos -= 1; }
        let run_len = meta.end - pos;
        let bounded_left = pos == meta.start || cells[pos - 1] == CellState::Empty;
        if bounded_left {
            let last = *meta.clues.last().unwrap() as usize;
            if run_len == last {
                meta.clues.pop();
                meta.end = pos;
                if meta.end > meta.start && cells[meta.end - 1] == CellState::Empty {
                    meta.end -= 1;
                }
                changed = true;
            } else if run_len > last {
                return Err(());
            }
        }
    }

    if meta.clues.is_empty() && !meta.complete {
        for i in meta.start..meta.end {
            if set_cell(cells, i, CellState::Empty)? { changed = true; }
        }
        meta.complete = true;
    }
    Ok(changed)
}

fn extend(cells: &mut [CellState], meta: &mut LineMeta) -> Result<bool, ()> {
    if meta.complete || meta.clues.is_empty() || meta.window_len() == 0 { return Ok(false); }

    let n = meta.clues.len();
    let total: u32 = meta.clues.iter().sum();
    let gaps = (n as u32).saturating_sub(1);
    let window = meta.window_len() as u32;
    if total + gaps > window { return Err(()); }
    let slack = (window - (total + gaps)) as usize;

    let mut ls_arr = vec![0usize; n];
    let mut rs_arr = vec![0usize; n];
    {
        let mut pos = 0usize;
        for i in 0..n {
            ls_arr[i] = pos;
            rs_arr[i] = pos + slack;
            pos += meta.clues[i] as usize + 1;
        }
    }

    let mut changed = false;
    let mut wi = 0usize;
    while wi < meta.window_len() {
        if cells[meta.start + wi] != CellState::Filled { wi += 1; continue; }
        let r_start = wi;
        while wi < meta.window_len() && cells[meta.start + wi] == CellState::Filled { wi += 1; }
        let r_end = wi;
        let m = r_end - r_start;
        let mut sole_ci: Option<usize> = None;
        let mut count = 0usize;
        for ci in 0..n {
            let c = meta.clues[ci] as usize;
            if c >= m && ls_arr[ci] <= r_start && rs_arr[ci] + c >= r_end {
                sole_ci = Some(ci);
                count += 1;
                if count > 1 { break; }
            }
        }
        if count == 0 { return Err(()); }
        if count == 1 {
            let ci = sole_ci.unwrap();
            let c  = meta.clues[ci] as usize;
            let lo = ls_arr[ci].max(r_end.saturating_sub(c));
            let hi = rs_arr[ci].min(r_start);
            if lo > hi { return Err(()); }
            let fill_end = (lo + c).min(meta.window_len());
            for j in hi..fill_end {
                if set_cell(cells, meta.start + j, CellState::Filled)? { changed = true; }
            }
        }
    }
    Ok(changed)
}

fn split(cells: &mut [CellState], meta: &mut LineMeta) -> Result<bool, ()> {
    if meta.complete || meta.window_len() == 0 { return Ok(false); }

    let mut subs: Vec<(usize, usize)> = Vec::new();
    {
        let mut i = meta.start;
        while i < meta.end {
            if cells[i] == CellState::Empty { i += 1; continue; }
            let start = i;
            while i < meta.end && cells[i] != CellState::Empty { i += 1; }
            subs.push((start, i));
        }
    }
    if subs.len() <= 1 { return Ok(false); }

    let n = meta.clues.len();
    let m = subs.len();
    let sw = |j: usize| subs[j].1 - subs[j].0;
    let has_filled: Vec<bool> = (0..m)
        .map(|j| (subs[j].0..subs[j].1).any(|i| cells[i] == CellState::Filled))
        .collect();

    let mut can_fwd = vec![vec![false; m + 1]; n + 1];
    can_fwd[0][0] = true;
    for j in 1..=m {
        if !has_filled[j - 1] { can_fwd[0][j] = can_fwd[0][j - 1]; }
        for k in 1..=n {
            if !has_filled[j - 1] && can_fwd[k][j - 1] { can_fwd[k][j] = true; continue; }
            let sj = sw(j - 1);
            let mut total: u32 = 0;
            for p in 1..=k {
                total += meta.clues[k - p];
                if p > 1 { total += 1; }
                if total as usize > sj { break; }
                if can_fwd[k - p][j - 1] { can_fwd[k][j] = true; break; }
            }
        }
    }
    if !can_fwd[n][m] { return Err(()); }

    let mut can_bwd = vec![vec![false; m + 1]; n + 1];
    for j in 0..=m { can_bwd[n][j] = true; }
    for k in (0..n).rev() {
        for j in (0..m).rev() {
            if !has_filled[j] && can_bwd[k][j + 1] { can_bwd[k][j] = true; continue; }
            let sj = sw(j);
            let mut total: u32 = 0;
            for p in 1..=(n - k) {
                total += meta.clues[k + p - 1];
                if p > 1 { total += 1; }
                if total as usize > sj { break; }
                if can_bwd[k + p][j + 1] { can_bwd[k][j] = true; break; }
            }
        }
    }
    if !can_bwd[0][0] { return Err(()); }

    let mut changed = false;
    for j in 0..m {
        let sj = sw(j);
        let mut valid: Vec<(usize, usize)> = Vec::new();
        for k_start in 0..=n {
            if !can_fwd[k_start][j] { continue; }
            if !has_filled[j] && can_bwd[k_start][j + 1] {
                valid.push((k_start, k_start));
            }
            let mut total: u32 = 0;
            for p in 1..=(n - k_start) {
                total += meta.clues[k_start + p - 1];
                if p > 1 { total += 1; }
                if total as usize > sj { break; }
                if can_bwd[k_start + p][j + 1] {
                    valid.push((k_start, k_start + p));
                }
            }
        }
        valid.sort_unstable();
        valid.dedup();
        if valid.is_empty() { return Err(()); }

        if valid.iter().all(|&(s, e)| s == e) {
            for i in subs[j].0..subs[j].1 {
                if set_cell(cells, i, CellState::Empty)? { changed = true; }
            }
            continue;
        }
        if valid.len() == 1 {
            let (ks, ke) = valid[0];
            let mut sub_meta = LineMeta::new(meta.clues[ks..ke].to_vec(), 0);
            sub_meta.start = subs[j].0;
            sub_meta.end   = subs[j].1;
            if run_core_passes(cells, &mut sub_meta)? { changed = true; }
        }
    }
    Ok(changed)
}

fn run_core_passes(cells: &mut [CellState], meta: &mut LineMeta) -> Result<bool, ()> {
    let t = trim(cells, meta);
    let p = preprocess(cells, meta)?;
    let o = overlap(cells, meta)?;
    let e = edge_forcing(cells, meta)?;
    let r = completed_run(cells, meta)?;
    let x = extend(cells, meta)?;
    let d = delim(cells, meta)?;
    Ok(t | p | o | e | r | x | d)
}

fn run_passes_on_line(cells: &mut [CellState], meta: &mut LineMeta) -> Result<bool, ()> {
    let c = run_core_passes(cells, meta)?;
    let s = split(cells, meta)?;
    Ok(c | s)
}

// ---------------------------------------------------------------------------
// Internal solve function
// ---------------------------------------------------------------------------

fn solve_internal(state: &mut SolverState, ctx: &SolveContext) -> (Outcome, bool) {
    let width  = state.puzzle.width;
    let height = state.puzzle.height;

    for r in 0..height {
        let mut row = state.row_slice(r).to_vec();
        if run_passes_on_line(&mut row, &mut state.row_meta[r]).is_err() {
            return (Outcome::NoSolution, false);
        }
        state.row_slice_mut(r).copy_from_slice(&row);
    }
    for c in 0..width {
        let mut col = state.col_vec(c);
        if run_passes_on_line(&mut col, &mut state.col_meta[c]).is_err() {
            return (Outcome::NoSolution, false);
        }
        state.write_col(c, &col);
    }

    let mut queue: VecDeque<LineId> = VecDeque::new();
    for r in 0..height {
        if !state.row_meta[r].complete { queue.push_back(LineId::Row(r)); }
    }
    for c in 0..width {
        if !state.col_meta[c].complete { queue.push_back(LineId::Col(c)); }
    }

    while let Some(line_id) = queue.pop_front() {
        if ctx.cancel.is_cancelled() {
            return (Outcome::Stuck, true);
        }
        let changed = match line_id {
            LineId::Row(r) => {
                if state.row_meta[r].complete { continue; }
                let mut row = state.row_slice(r).to_vec();
                let result = run_passes_on_line(&mut row, &mut state.row_meta[r]);
                state.row_slice_mut(r).copy_from_slice(&row);
                match result {
                    Err(()) => return (Outcome::NoSolution, false),
                    Ok(c)   => c,
                }
            }
            LineId::Col(c) => {
                if state.col_meta[c].complete { continue; }
                let mut col  = state.col_vec(c);
                let orig     = col.clone();
                let result   = run_passes_on_line(&mut col, &mut state.col_meta[c]);
                let actually_changed = col != orig;
                state.write_col(c, &col);
                match result {
                    Err(()) => return (Outcome::NoSolution, false),
                    Ok(_)   => actually_changed,
                }
            }
        };

        if changed {
            match line_id {
                LineId::Row(_) => {
                    for c in 0..width {
                        if !state.col_meta[c].complete { queue.push_back(LineId::Col(c)); }
                    }
                }
                LineId::Col(_) => {
                    for r in 0..height {
                        if !state.row_meta[r].complete { queue.push_back(LineId::Row(r)); }
                    }
                }
            }
        }
    }

    let outcome = if state.cells.iter().all(|&c| c != CellState::Unknown) {
        Outcome::Solved
    } else {
        Outcome::Stuck
    };
    (outcome, false)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub struct PropagationSolver;

impl Solver for PropagationSolver {
    fn solve(&self, puzzle: &Puzzle) -> SolveResult {
        self.solve_with(puzzle, &SolveContext::default())
    }

    fn solve_with(&self, puzzle: &Puzzle, ctx: &SolveContext) -> SolveResult {
        let mut state = SolverState::new(puzzle);
        let (outcome, aborted) = solve_internal(&mut state, ctx);
        SolveResult { outcome, grid: state.cells, steps: vec![], aborted }
    }
}
