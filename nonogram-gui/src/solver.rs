use std::fmt;
use nonogram_core::{Puzzle, Solver, SolveContext, SolveResult};
use nonogram_propagation::PropagationSolver;
use nonogram_graph_search::GraphSearchSolver;
use nonogram_human::HumanSolver;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SolverKind {
    Propagation,
    GraphSearch,
    Human,
}

impl SolverKind {
    pub const ALL: &'static [Self] = &[
        Self::Propagation,
        Self::GraphSearch,
        Self::Human,
    ];

    pub fn solve(self, puzzle: &Puzzle, ctx: &SolveContext) -> SolveResult {
        match self {
            Self::Propagation => PropagationSolver.solve(puzzle, ctx),
            Self::GraphSearch => GraphSearchSolver.solve(puzzle, ctx),
            Self::Human       => HumanSolver.solve(puzzle, ctx),
        }
    }
}

impl fmt::Display for SolverKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Propagation => write!(f, "Propagation"),
            Self::GraphSearch => write!(f, "Graph Search"),
            Self::Human       => write!(f, "Human"),
        }
    }
}
