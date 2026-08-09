use std::fmt;
use nonogram_core::{AllSolutions, ExhaustiveSolver, Puzzle, Solver, SolveContext, SolveResult};
use nonogram_propagation::PropagationSolver;
use nonogram_human_by_ai::PropagationSolver as SenseiSolver;
use nonogram_graph_search::GraphSearchSolver;
use nonogram_human::HumanSolver;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SolverKind {
    Manual,      // Human player — no solver, just manual grid editing
    Cuttlefish,  // Graph search with MRV heuristic
    DeepRed,     // Full per-line hard-logic deduction (DP, no technique cap)
    Sensei,      // Named human-legible technique passes (overlap, edge-forcing, …)
    Noobie,      // Human-logic stub
}

impl SolverKind {
    pub const ALL: &'static [Self] = &[
        Self::Manual,
        Self::Noobie,
        Self::Sensei,
        Self::DeepRed,
        Self::Cuttlefish,
    ];

    pub fn is_machine(self) -> bool {
        !matches!(self, Self::Manual)
    }

    pub fn solve(self, puzzle: &Puzzle, ctx: &SolveContext) -> SolveResult {
        match self {
            Self::Manual     => unreachable!("Manual mode has no solver"),
            Self::Noobie     => HumanSolver.solve(puzzle, ctx),
            Self::Sensei     => SenseiSolver.solve(puzzle, ctx),
            Self::DeepRed    => PropagationSolver.solve(puzzle, ctx),
            Self::Cuttlefish => GraphSearchSolver.solve(puzzle, ctx),
            
            
        }
    }

    pub fn supports_exhaustive(self) -> bool {
        matches!(self, Self::Cuttlefish)
    }

    pub fn solve_all(self, puzzle: &Puzzle, ctx: &SolveContext) -> Option<AllSolutions> {
        match self {
            Self::Cuttlefish => Some(GraphSearchSolver.solve_all(puzzle, ctx)),
            _ => None,
        }
    }
}

impl fmt::Display for SolverKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manual     => write!(f, "Manual"),
            Self::Noobie     => write!(f, "Noobie"),
            Self::Sensei     => write!(f, "Sensei"),
            Self::DeepRed    => write!(f, "Deep Red"),
            Self::Cuttlefish => write!(f, "Cuttlefish"),
        }
    }
}
