//! Thin `wasm-bindgen` adapter around `nonogram-graph-search`, for `nonogram-web`.
//! No solving logic lives here — see `discussions/WasmPort.md` for the design.

use nonogram_core::{CellState, Puzzle, Solver, SolveContext, SolutionState};
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

fn to_js_err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// `clues` is `{ row_clues: number[][], col_clues: number[][] }`; width/height are taken from
/// the clue array lengths rather than passed separately, so they can't disagree with them.
#[wasm_bindgen]
pub fn solve(clues: JsValue) -> Result<JsValue, JsValue> {
    let clues: Clues = serde_wasm_bindgen::from_value(clues).map_err(to_js_err)?;
    let puzzle = Puzzle {
        name: String::new(),
        answer: None,
        width: clues.col_clues.len(),
        height: clues.row_clues.len(),
        row_clues: clues.row_clues,
        col_clues: clues.col_clues,
        solution: None,
    };

    let result = GraphSearchSolver.solve(&puzzle, &SolveContext::default());

    let (state, invalid_reason) = match &result.state {
        SolutionState::Complete => ("Complete", None),
        SolutionState::Partial => ("Partial", None),
        SolutionState::Unsolvable => ("Unsolvable", None),
        SolutionState::Aborted => ("Aborted", None),
        SolutionState::Invalid(reason) => ("Invalid", Some(reason.clone())),
    };
    let grid = result
        .grid
        .iter()
        .map(|c| match c {
            CellState::Unknown => 0u8,
            CellState::Filled => 1,
            CellState::Empty => 2,
        })
        .collect();

    serde_wasm_bindgen::to_value(&WasmSolveResult { state, invalid_reason, grid }).map_err(to_js_err)
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
}
