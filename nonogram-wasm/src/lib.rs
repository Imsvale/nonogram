//! Thin `wasm-bindgen` adapter around `nonogram-graph-search`, for `nonogram-web`.
//! No solving logic lives here — see `discussions/WasmPort.md` for the design.

use nonogram_core::{CellState, ExhaustiveSolver, LineId, Puzzle, Solver, SolveContext, SolutionState, SolveStep};
use nonogram_graph_search::GraphSearchSolver;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Deserialize)]
struct Clues {
    row_clues: Vec<Vec<u32>>,
    col_clues: Vec<Vec<u32>>,
}

/// `grid` is row-major, one byte per cell: 0 = Unknown, 1 = Filled, 2 = Empty.
#[derive(Serialize)]
struct WasmSolveResult {
    state: &'static str,
    invalid_reason: Option<String>,
    grid: Vec<u8>,
}

/// For the Analyze button: is this puzzle's solution unique, does finding it require
/// branching/trialing (vs. pure logic alone), and a solution to show if one exists.
#[derive(Serialize)]
struct AnalyzeResult {
    /// 0 = unsolvable, 1 = unique, ≥2 = ambiguous.
    solution_count: usize,
    /// `false` iff propagation alone (no branching) resolved this puzzle one way or the
    /// other — solved, or proven unsolvable. Derived from `AllSolutions::nodes_pushed`
    /// (only incremented once the branching heap is actually touched), not from inspecting
    /// step descriptions: those are free-text for human display, not a stable contract.
    needs_guessing: bool,
    /// The first solution found, if any (`None` iff `solution_count == 0`). Present even
    /// when ambiguous — "a" solution is still useful to show.
    grid: Option<Vec<u8>>,
    /// That solution's step trace, for the solution viewer. `None` under the exact same
    /// condition as `grid` — there's nothing to step through when there's no solution.
    steps: Option<Vec<WasmSolveStep>>,
}

#[derive(Serialize)]
struct WasmLine {
    axis: &'static str,
    index: usize,
}

#[derive(Serialize)]
struct WasmCellChange {
    /// Row-major flat index (`row * width + col`), matching the `grid` encoding.
    idx: u32,
    state: u8,
}

#[derive(Serialize)]
struct WasmSolveStep {
    /// "Logic" or "Trial" — see `step_kind`.
    kind: &'static str,
    line: Option<WasmLine>,
    cells_changed: Vec<WasmCellChange>,
}

/// Display-only classification of a step as "Trial" (a branch/guess) vs. "Logic" (pure
/// propagation), by checking for graph-search's own `"branch"` marker in `description` —
/// not a correctness decision (unlike `needs_guessing`, which uses the structured
/// `nodes_pushed` count instead). Isolated here so a future structured field in
/// `nonogram-core` is a one-line change at this one call site.
fn step_kind(step: &SolveStep) -> &'static str {
    if step.description.contains("branch") { "Trial" } else { "Logic" }
}

fn to_wasm_step(step: &SolveStep, width: usize) -> WasmSolveStep {
    WasmSolveStep {
        kind: step_kind(step),
        line: step.line.map(|l| match l {
            LineId::Row(r) => WasmLine { axis: "row", index: r },
            LineId::Col(c) => WasmLine { axis: "col", index: c },
        }),
        cells_changed: step
            .cells_changed
            .iter()
            .map(|&(r, c, state)| WasmCellChange {
                idx: (r * width + c) as u32,
                state: match state {
                    CellState::Unknown => 0u8,
                    CellState::Filled => 1,
                    CellState::Empty => 2,
                },
            })
            .collect(),
    }
}

fn to_js_err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

fn grid_to_bytes(grid: &[CellState]) -> Vec<u8> {
    grid.iter()
        .map(|c| match c {
            CellState::Unknown => 0u8,
            CellState::Filled => 1,
            CellState::Empty => 2,
        })
        .collect()
}

fn puzzle_from_clues(clues: Clues) -> Puzzle {
    Puzzle {
        name: String::new(),
        answer: None,
        width: clues.col_clues.len(),
        height: clues.row_clues.len(),
        row_clues: clues.row_clues,
        col_clues: clues.col_clues,
        solution: None,
    }
}

/// `clues` is `{ row_clues: number[][], col_clues: number[][] }`; width/height are taken from
/// the clue array lengths rather than passed separately, so they can't disagree with them.
#[wasm_bindgen]
pub fn solve(clues: JsValue) -> Result<JsValue, JsValue> {
    let clues: Clues = serde_wasm_bindgen::from_value(clues).map_err(to_js_err)?;
    let puzzle = puzzle_from_clues(clues);
    let result = GraphSearchSolver.solve(&puzzle, &SolveContext::default());

    let (state, invalid_reason) = match &result.state {
        SolutionState::Complete => ("Complete", None),
        SolutionState::Partial => ("Partial", None),
        SolutionState::Unsolvable => ("Unsolvable", None),
        SolutionState::Aborted => ("Aborted", None),
        SolutionState::Invalid(reason) => ("Invalid", Some(reason.clone())),
    };
    let grid = grid_to_bytes(&result.grid);

    serde_wasm_bindgen::to_value(&WasmSolveResult { state, invalid_reason, grid }).map_err(to_js_err)
}

/// Backs the Analyze button: uniqueness, whether branching/trialing was required, and a
/// solution to display. One exhaustive-search call answers all three — see `AnalyzeResult`.
#[wasm_bindgen]
pub fn analyze(clues: JsValue) -> Result<JsValue, JsValue> {
    let clues: Clues = serde_wasm_bindgen::from_value(clues).map_err(to_js_err)?;
    let puzzle = puzzle_from_clues(clues);
    let all = GraphSearchSolver.solve_all(&puzzle, &SolveContext::default());

    let first = all.solutions.first();
    let out = AnalyzeResult {
        solution_count: all.solutions.len(),
        needs_guessing: all.nodes_pushed > 0,
        grid: first.map(|r| grid_to_bytes(&r.grid)),
        steps: first.map(|r| r.steps.iter().map(|s| to_wasm_step(s, puzzle.width)).collect()),
    };
    serde_wasm_bindgen::to_value(&out).map_err(to_js_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Exercises the actual solving path (not the wasm-bindgen boundary, which needs a JS host)
    // directly, the same way the adapter itself calls it.
    #[test]
    fn solves_a_trivial_puzzle() {
        let puzzle = Puzzle {
            name: "t".into(),
            answer: None,
            width: 2,
            height: 2,
            row_clues: vec![vec![2], vec![]],
            col_clues: vec![vec![1], vec![1]],
            solution: None,
        };
        let result = GraphSearchSolver.solve(&puzzle, &SolveContext::default());
        assert_eq!(result.state, SolutionState::Complete);
        assert_eq!(result.grid, vec![CellState::Filled, CellState::Filled, CellState::Empty, CellState::Empty]);
    }

    // n×n grid of single-cell clues: any permutation matrix satisfies every line, so
    // intersection propagation alone forces nothing and the solver must branch — same fixture
    // nonogram-graph-search's own tests use for that reason.
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
    fn analyze_reports_no_guessing_for_a_propagation_only_puzzle() {
        let puzzle = Puzzle {
            name: "t".into(),
            answer: None,
            width: 2,
            height: 2,
            row_clues: vec![vec![2], vec![]],
            col_clues: vec![vec![1], vec![1]],
            solution: None,
        };
        let all = GraphSearchSolver.solve_all(&puzzle, &SolveContext::default());
        assert_eq!(all.solutions.len(), 1);
        assert_eq!(all.nodes_pushed, 0);
    }

    #[test]
    fn analyze_reports_guessing_and_ambiguity_for_a_permutation_puzzle() {
        let puzzle = permutation_puzzle(2);
        let all = GraphSearchSolver.solve_all(&puzzle, &SolveContext::default());
        // Both 2x2 permutation matrices are valid solutions, and finding either one
        // requires a branch — propagation alone can't place a single cell.
        assert_eq!(all.solutions.len(), 2);
        assert!(all.nodes_pushed > 0);
    }

    #[test]
    fn step_kind_finds_no_trial_steps_in_a_propagation_only_solve() {
        let puzzle = Puzzle {
            name: "t".into(),
            answer: None,
            width: 2,
            height: 2,
            row_clues: vec![vec![2], vec![]],
            col_clues: vec![vec![1], vec![1]],
            solution: None,
        };
        let result = GraphSearchSolver.solve(&puzzle, &SolveContext::default());
        assert!(result.steps.iter().all(|s| step_kind(s) == "Logic"));
    }

    #[test]
    fn step_kind_finds_a_trial_step_in_a_permutation_solve() {
        let puzzle = permutation_puzzle(2);
        let all = GraphSearchSolver.solve_all(&puzzle, &SolveContext::default());
        let steps = &all.solutions[0].steps;
        assert!(steps.iter().any(|s| step_kind(s) == "Trial"));
    }
}
