//! Human-logic solver — 100% author-written, no AI implementation assistance.
//!
//! This crate is a deliberate placeholder. Only the trait wiring here was
//! written with AI help; the solving logic itself is left entirely to the
//! author. When ready, implement `HumanSolver::solve` directly in this file.

use nonogram_core::{Outcome, Puzzle, Solver, SolveResult};

pub struct HumanSolver;

impl Solver for HumanSolver {
    fn solve(&self, _puzzle: &Puzzle) -> SolveResult {
        SolveResult { outcome: Outcome::Stuck, grid: vec![], steps: vec![], aborted: false }
    }
}
