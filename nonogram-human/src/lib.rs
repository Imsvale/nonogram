//! Human-logic solver — 100% author-written, no AI implementation assistance.
//!
//! This crate is a deliberate placeholder. Only the trait wiring here was
//! written with AI help; the solving logic itself is left entirely to the
//! author. When ready, implement `HumanSolver::solve` directly in this file.

use nonogram_core::{CellState, Puzzle, Solver, SolveResult, SolutionState};

pub struct HumanSolver;

impl Solver for HumanSolver {
    fn solve(&self, puzzle: &Puzzle, _ctx: &nonogram_core::SolveContext) -> SolveResult {
        SolveResult {
            state: SolutionState::Partial,
            grid: vec![CellState::Unknown; puzzle.width * puzzle.height],
            steps: vec![],
        }
    }
}
